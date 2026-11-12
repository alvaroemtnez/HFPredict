//! "Who can hear me now": the headline, the coverage map and the hour
//! track.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, glib};
use hfpredict_core::app::MapDetail;
use hfpredict_core::projection::MapView;
use hfpredict_core::quality::Quality;
use hfpredict_core::tiles::{self, Screen};

use crate::app::{App, Change, PAST_HOURS};
use crate::canvas::{self, Canvas};
use crate::format;
use crate::i18n::{tr, trf};
use crate::palette;

type ExpandFn = Box<dyn Fn(bool)>;

/// Room the floating hour slider takes at the map's foot, so the probe
/// and the spinner sit above it.
const FLOAT_CLEARANCE: i32 = 128;

/// A drag of logical pixels, held to the map's screen.
fn pan(app: &App, view: MapView, dx: f64, dy: f64) -> MapView {
    match app.screen() {
        Some((screen, k)) => tiles::panned(view, dx * f64::from(k), dy * f64::from(k), &screen),
        None => view,
    }
}

/// A zoom about a point in logical pixels, which stays put.
fn zoom_at(app: &App, view: MapView, factor: f64, x: f64, y: f64) -> MapView {
    match app.screen() {
        Some((screen, k)) => {
            let k = f64::from(k);
            tiles::zoomed(view, factor, x * k, y * k, &screen)
        }
        None => view,
    }
}

/// Marks the map canvas while enlarged.
const ENLARGED: &str = "hfp-map-enlarged";

/// The whole globe's square on the canvas, centred: its side and top-left
/// corner. The shorter side in the card (the globe fits, and zoomed in the
/// map fills the rest), the longer one enlarged (the rest is cut off).
fn square(c: &Canvas) -> (f64, f64, f64) {
    let full = c.has_css_class(ENLARGED);
    let (w, h) = (f64::from(c.width().max(1)), f64::from(c.height().max(1)));
    let side = if full { w.max(h) } else { w.min(h) };
    (side, (w - side) / 2.0, (h - side) / 2.0)
}

/// The map's natural width, logical pixels.
const MAP_WIDTH: i32 = 720;

/// The map's height in the card for a width: a square as wide as the card
/// in one column (as on a phone), a landscape 4:3 beside the other cards.
fn card_height(wide: bool, width: i32) -> i32 {
    if wide { width * 3 / 4 } else { width }
}

pub struct ReachCard {
    pub root: gtk::Box,
    headline: gtk::Label,
    quality: gtk::Label,
    map: Canvas,
    spinner: adw::Spinner,
    /// What is under the last click: text and "set as destination".
    probe: gtk::Box,
    probe_text: gtk::Label,
    detail: gtk::Label,
    reach_line: gtk::Label,
    legend: gtk::Box,
    scale: gtk::Scale,
    local_label: gtk::Label,
    /// Holds the map and what floats over it.
    holder: gtk::Box,
    /// Two columns: the map is a landscape rectangle, not a square.
    wide: Cell<bool>,
    overlay: gtk::Overlay,
    /// The hour slider and its labels: under the map, or floating on it.
    hours: gtk::Box,
    fullscreen: gtk::Button,
    expanded: Rc<Cell<bool>>,
    /// Told when the map goes full screen or comes back.
    pub on_expand: RefCell<Option<ExpandFn>>,
    /// Set while the code moves the slider, so its signal is not read as
    /// the reader's.
    syncing: Cell<bool>,
}

fn label(classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .build();
    for c in classes {
        l.add_css_class(c);
    }
    l
}

fn osd_button(icon: &str, tooltip: &str) -> gtk::Button {
    let b = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .build();
    b.add_css_class("osd");
    b.add_css_class("circular");
    b.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    b
}

impl ReachCard {
    pub fn new(app: &Rc<App>) -> Rc<ReachCard> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.add_css_class("card");
        root.add_css_class("hfp-card");

        let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let headline = label(&["title-4"]);
        headline.set_hexpand(true);
        let quality = gtk::Label::new(None);
        quality.add_css_class("hfp-pill");
        quality.set_valign(gtk::Align::Start);
        top.append(&headline);
        top.append(&quality);
        root.append(&top);

        let map = Canvas::new();
        map.set_widths(200, MAP_WIDTH);
        map.set_height_for_width(|w| card_height(false, w));
        map.set_focusable(true);
        map.add_css_class("hfp-map");
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&map));

        let controls = gtk::Box::new(gtk::Orientation::Vertical, 6);
        controls.set_halign(gtk::Align::End);
        controls.set_valign(gtk::Align::Start);
        controls.set_margin_top(8);
        controls.set_margin_end(8);
        let zoom_in = osd_button("zoom-in-symbolic", &tr("map.zoomIn"));
        let zoom_out = osd_button("zoom-out-symbolic", &tr("map.zoomOut"));
        let fit = osd_button("zoom-fit-best-symbolic", &tr("map.fitReach"));
        let globe = osd_button("zoom-original-symbolic", &tr("map.wholeGlobe"));
        let fullscreen = osd_button("view-fullscreen-symbolic", &tr("map.fullscreen"));
        for b in [&zoom_in, &zoom_out, &fit, &globe, &fullscreen] {
            controls.append(b);
        }
        overlay.add_overlay(&controls);

        let probe = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let probe_text = gtk::Label::new(None);
        probe.append(&probe_text);
        let to_here = gtk::Button::with_label(&tr("map.setDestination"));
        to_here.add_css_class("flat");
        to_here.add_css_class("hfp-chip");
        {
            let a = Rc::downgrade(app);
            to_here.connect_clicked(move |_| {
                if let Some(app) = a.upgrade() {
                    app.probe_to_destination();
                }
            });
        }
        probe.append(&to_here);
        probe.add_css_class("osd");
        probe.add_css_class("hfp-probe");
        probe.set_halign(gtk::Align::Start);
        probe.set_valign(gtk::Align::End);
        probe.set_margin_start(8);
        probe.set_margin_bottom(8);
        probe.set_visible(false);
        overlay.add_overlay(&probe);

        let spinner = adw::Spinner::new();
        spinner.set_halign(gtk::Align::End);
        spinner.set_valign(gtk::Align::End);
        spinner.set_margin_end(14);
        spinner.set_margin_bottom(14);
        spinner.set_size_request(24, 24);
        overlay.add_overlay(&spinner);

        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        holder.append(&overlay);
        root.append(&holder);

        let detail = label(&["dim-label", "caption"]);
        let legend = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        legend.add_css_class("caption");
        let reach_line = label(&["dim-label"]);
        root.append(&detail);
        root.append(&legend);
        root.append(&reach_line);

        let hours = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let local_label = label(&["caption-heading", "dim-label"]);
        local_label.set_margin_top(6);
        hours.append(&local_label);
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 23.0, 1.0);
        scale.set_draw_value(false);
        scale.set_round_digits(0);
        scale.add_css_class("hfp-hours");
        scale.update_property(&[gtk::accessible::Property::Label(&tr("time.hourSlider"))]);
        hours.append(&scale);
        let utc_label = label(&["caption-heading", "dim-label"]);
        utc_label.set_text(&tr("time.utcAt"));
        hours.append(&utc_label);
        root.append(&hours);

        let card = Rc::new(ReachCard {
            root,
            headline,
            quality,
            map,
            spinner,
            probe,
            probe_text,
            detail,
            reach_line,
            legend,
            scale,
            local_label,
            holder,
            wide: Cell::new(false),
            overlay,
            hours,
            fullscreen,
            expanded: Rc::new(Cell::new(false)),
            on_expand: RefCell::new(None),
            syncing: Cell::new(false),
        });
        card.wire(app, &zoom_in, &zoom_out, &fit, &globe);
        card
    }

    fn wire(
        self: &Rc<Self>,
        app: &Rc<App>,
        zoom_in: &gtk::Button,
        zoom_out: &gtk::Button,
        fit: &gtk::Button,
        globe: &gtk::Button,
    ) {
        // The map is drawn from tiles (see App::pump_tiles): every level
        // from the whole earth down to the one that matches the screen, so
        // a coarser tile shows wherever a finer one is not drawn yet. A
        // gesture only moves textures.
        let a = Rc::downgrade(app);
        let me = Rc::downgrade(self);
        self.map.set_draw(move |_, s, w, h| {
            let (Some(app), Some(me)) = (a.upgrade(), me.upgrade()) else {
                return;
            };
            let full = me.expanded.get();
            let clip = canvas::rect(0.0, 0.0, w, h);
            s.push_rounded_clip(&gtk::gsk::RoundedRect::from_rect(clip, 12.0));
            if full {
                let disc = app.view.borrow().colors.map.disc;
                s.append_color(&palette::rgba_of(disc), &clip);
            }
            if let Some((screen, scale)) = app.screen() {
                let view = app.view.borrow().map_view;
                let k = f64::from(scale);
                let frame = app.frame();
                // At rest the frame is exactly the screen: drawn 1:1, sharp
                // and seamless, and nothing else is needed.
                let exact = frame
                    .as_ref()
                    .is_some_and(|f| f.view == view && f.screen == screen);
                if exact {
                    if let Some(f) = frame.as_ref() {
                        s.append_texture(
                            &f.texture,
                            &canvas::rect(
                                0.0,
                                0.0,
                                (screen.width / k) as f32,
                                (screen.height / k) as f32,
                            ),
                        );
                    }
                } else {
                    // A gesture: tiles of the current content (never older)
                    // under the moving frame fill what it does not cover;
                    // zoomed in past it, the finest ones go over it, sharper.
                    let held = app.tiles.borrow();
                    let fresh = app.signature();
                    let z = tiles::level_for(view, &screen);
                    let draw_tiles = |levels: std::ops::RangeInclusive<u32>| {
                        for p in levels.flat_map(|l| tiles::visible(view, &screen, l)) {
                            let Some(e) = held.get(&p.tile).filter(|e| e.signature == fresh) else {
                                continue;
                            };
                            e.used.set(app.next_use());
                            // Half a pixel of overlap hides the seams while moving.
                            let r = canvas::rect(
                                (p.left / k) as f32 - 0.5,
                                (p.top / k) as f32 - 0.5,
                                (p.size / k) as f32 + 1.0,
                                (p.size / k) as f32 + 1.0,
                            );
                            s.append_scaled_texture(
                                &e.texture,
                                gtk::gsk::ScalingFilter::Linear,
                                &r,
                            );
                        }
                    };
                    draw_tiles(0..=z);
                    let mut stretched = false;
                    if let Some(f) = frame.as_ref() {
                        // Where the frame's pixels land in the view on screen.
                        let kf = f.screen.side * f.view.scale;
                        let kc = screen.side * view.scale;
                        let u0 = f.view.cx - 0.5 / f.view.scale - f.screen.x0 / kf;
                        let v0 = f.view.cy - 0.5 / f.view.scale - f.screen.y0 / kf;
                        let left = screen.x0 + (u0 - (view.cx - 0.5 / view.scale)) * kc;
                        let top = screen.y0 + (v0 - (view.cy - 0.5 / view.scale)) * kc;
                        let r = canvas::rect(
                            (left / k) as f32,
                            (top / k) as f32,
                            (f.screen.width / kf * kc / k) as f32,
                            (f.screen.height / kf * kc / k) as f32,
                        );
                        s.append_scaled_texture(&f.texture, gtk::gsk::ScalingFilter::Linear, &r);
                        stretched = kc / kf > 1.05;
                    }
                    if stretched {
                        draw_tiles(z..=z);
                    }
                }
            }
            s.pop();
        });
        let a = Rc::downgrade(app);
        self.map.set_on_resize(move |c, w, h| {
            if let Some(app) = a.upgrade() {
                let (side, x0, y0) = square(c);
                // The surface's own scale, fractional (1.25, 1.5…) where the
                // desktop scales so: the frame then has exactly the screen's
                // pixels. The widget's whole-number factor would be resampled.
                let k = c
                    .native()
                    .and_then(|n| n.surface())
                    .map(|surface| surface.scale())
                    .filter(|k| *k > 0.0)
                    .unwrap_or_else(|| f64::from(c.scale_factor().max(1)));
                app.set_map_screen(
                    Screen {
                        side: side * k,
                        x0: x0 * k,
                        y0: y0 * k,
                        width: f64::from(w) * k,
                        height: f64::from(h) * k,
                    },
                    k as f32,
                );
            }
        });
        // Moved to a screen of another scale: measure (and draw) again.
        self.map.connect_scale_factor_notify(|c| c.queue_resize());
        let map = self.map.clone();
        *app.redraw.borrow_mut() = Some(Box::new(move || map.queue_draw()));

        // Gestures: drag pans, pinch and scroll zoom, a click probes.
        let side = |c: &Canvas| square(c).0;
        let offset = |c: &Canvas| square(c).1;
        let offset_y = |c: &Canvas| square(c).2;
        let drag = gtk::GestureDrag::new();
        let start_view = Rc::new(Cell::new(MapView::default()));
        {
            let a = Rc::downgrade(app);
            let sv = start_view.clone();
            drag.connect_drag_begin(move |_, _, _| {
                if let Some(app) = a.upgrade() {
                    sv.set(app.view.borrow().map_view);
                }
            });
        }
        {
            let a = Rc::downgrade(app);
            let sv = start_view.clone();
            let map = self.map.clone();
            drag.connect_drag_update(move |_, dx, dy| {
                if let Some(app) = a.upgrade() {
                    app.set_map_view(pan(&app, sv.get(), dx, dy), false);
                    map.queue_draw();
                }
            });
        }
        {
            let a = Rc::downgrade(app);
            let sv = start_view.clone();
            let map = self.map.clone();
            drag.connect_drag_end(move |_, dx, dy| {
                if let Some(app) = a.upgrade() {
                    if dx.hypot(dy) > 3.0 {
                        app.set_map_view(pan(&app, sv.get(), dx, dy), true);
                    }
                    map.queue_draw();
                }
            });
        }
        self.map.add_controller(drag);

        let zoom = gtk::GestureZoom::new();
        {
            let a = Rc::downgrade(app);
            let sv = start_view.clone();
            zoom.connect_begin(move |_, _| {
                if let Some(app) = a.upgrade() {
                    sv.set(app.view.borrow().map_view);
                }
            });
        }
        {
            let a = Rc::downgrade(app);
            let sv = start_view.clone();
            let map = self.map.clone();
            zoom.connect_scale_changed(move |g, k| {
                if let (Some(app), Some((x, y))) = (a.upgrade(), g.bounding_box_center()) {
                    let v = zoom_at(&app, sv.get(), k, x, y);
                    app.set_map_view(v, false);
                    map.queue_draw();
                }
            });
        }
        {
            let a = Rc::downgrade(app);
            zoom.connect_end(move |_, _| {
                if let Some(app) = a.upgrade() {
                    let v = app.view.borrow().map_view;
                    app.set_map_view(v, true);
                }
            });
        }
        self.map.add_controller(zoom);

        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let pointer = Rc::new(Cell::new((0.0, 0.0)));
        let motion = gtk::EventControllerMotion::new();
        {
            let p = pointer.clone();
            motion.connect_motion(move |_, x, y| p.set((x, y)));
        }
        self.map.add_controller(motion);
        {
            let a = Rc::downgrade(app);
            let map = self.map.clone();
            let p = pointer.clone();
            scroll.connect_scroll(move |_, _, dy| {
                if let Some(app) = a.upgrade() {
                    let (x, y) = p.get();
                    let v0 = app.view.borrow().map_view;
                    let v = zoom_at(&app, v0, if dy < 0.0 { 1.25 } else { 0.8 }, x, y);
                    app.set_map_view(v, false);
                    map.queue_draw();
                }
                glib::Propagation::Stop
            });
        }
        self.map.add_controller(scroll);

        let click = gtk::GestureClick::new();
        {
            let a = Rc::downgrade(app);
            let map = self.map.clone();
            click.connect_released(move |_, n, x, y| {
                if let Some(app) = a.upgrade()
                    && n == 1
                {
                    app.probe(x - offset(&map), y - offset_y(&map), side(&map));
                }
            });
        }
        self.map.add_controller(click);

        let zoom_by = |k: f64| {
            let a = Rc::downgrade(app);
            let map = self.map.clone();
            move |_: &gtk::Button| {
                if let Some(app) = a.upgrade() {
                    let (w, h) = (f64::from(map.width()), f64::from(map.height()));
                    let v0 = app.view.borrow().map_view;
                    let v = zoom_at(&app, v0, k, w / 2.0, h / 2.0);
                    app.set_map_view(v, true);
                    map.queue_draw();
                }
            }
        };
        zoom_in.connect_clicked(zoom_by(1.6));
        zoom_out.connect_clicked(zoom_by(1.0 / 1.6));
        {
            let a = Rc::downgrade(app);
            let map = self.map.clone();
            fit.connect_clicked(move |_| {
                if let Some(app) = a.upgrade() {
                    app.fit_reach();
                    map.queue_draw();
                }
            });
        }
        {
            let me = Rc::downgrade(self);
            self.fullscreen.connect_clicked(move |_| {
                if let Some(me) = me.upgrade() {
                    me.set_expanded(!me.expanded.get());
                }
            });
        }
        // Escape leaves full screen.
        let keys = gtk::EventControllerKey::new();
        {
            let me = Rc::downgrade(self);
            keys.connect_key_pressed(move |_, key, _, _| {
                if let Some(me) = me.upgrade()
                    && key == gdk::Key::Escape
                    && me.expanded.get()
                {
                    me.set_expanded(false);
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
        }
        self.root.add_controller(keys);
        {
            let a = Rc::downgrade(app);
            let map = self.map.clone();
            globe.connect_clicked(move |_| {
                if let Some(app) = a.upgrade() {
                    app.set_map_view(MapView::default(), true);
                    map.queue_draw();
                }
            });
        }

        {
            let a = Rc::downgrade(app);
            let me = Rc::downgrade(self);
            self.scale.connect_value_changed(move |s| {
                let (Some(app), Some(me)) = (a.upgrade(), me.upgrade()) else {
                    return;
                };
                if me.syncing.get() {
                    return;
                }
                let (band, start) = {
                    let v = app.view.borrow();
                    (v.band, v.track_start())
                };
                let hour = (start + s.value().round() as u32) % 24;
                if hour != app.view.borrow().hour {
                    app.select(band, hour);
                }
            });
        }

        let me = Rc::downgrade(self);
        app.listen(move |app, change| {
            if let Some(me) = me.upgrade() {
                me.update(app, change);
            }
        });
        self.update(app, Change::Saved);
    }

    fn update(&self, app: &App, change: Change) {
        let v = app.view.borrow();
        let imperial = v.imperial();
        let from = v
            .saved
            .from
            .clone()
            .unwrap_or_else(hfpredict_core::places::Place::greenwich);

        // Headline: the chance to the destination, or the share of
        // directions, at this band and hour.
        let cell = v
            .forecast
            .as_ref()
            .and_then(|f| f.cell(v.band, v.hour).map(|c| (f, *c)));
        match cell {
            Some((f, c)) => {
                let percent = format::percent(c.reliability);
                let hour = format::hour(v.hour);
                let text = match &f.to {
                    Some(to) => trf(
                        "reach.answer",
                        &[
                            ("band", v.band.label()),
                            ("place", &to.name),
                            ("hour", &hour),
                            ("percent", &percent),
                        ],
                    ),
                    None => trf(
                        "reach.answerAnywhere",
                        &[
                            ("band", v.band.label()),
                            ("hour", &hour),
                            ("percent", &percent),
                        ],
                    ),
                };
                self.headline.set_text(&text);
                let q = Quality::of(c.reliability);
                self.quality.set_text(&format::quality(q));
                self.quality.set_visible(true);
                for k in ["reliable", "patchy", "weak", "closed"] {
                    self.quality.remove_css_class(&format!("hfp-q-{k}"));
                }
                self.quality.add_css_class(&format!("hfp-q-{}", q.key()));
                self.quality
                    .set_tooltip_text(Some(&tr(&format!("qualityDescription.{}", q.key()))));
            }
            None => {
                self.headline.set_text(&tr("status.loading"));
                self.quality.set_visible(false);
            }
        }

        // Status lines under the map.
        self.spinner.set_visible(v.map_working);
        let detail = match (&v.map_error, v.map_detail) {
            (Some(_), _) => tr("reach.mapUnavailable"),
            (None, MapDetail::None) => tr("reach.mapLoading"),
            (None, MapDetail::Coarse) => tr("reach.detailCoarse"),
            (None, MapDetail::Fine) => tr("reach.detailWorld"),
            (None, MapDetail::Patch) => tr("reach.detailNear"),
        };
        self.detail.set_text(&detail);
        let reach = v
            .status
            .and_then(|s| s.reach)
            .or_else(|| v.reach.iter().find(|r| r.band == v.band).map(|r| r.reach));
        let nvis = v.status.and_then(|s| s.nvis_km);
        let line = match (reach, nvis) {
            (Some(r), Some(km)) => trf(
                "reach.reachAndNvis",
                &[
                    ("band", v.band.label()),
                    ("percent", &format::percent(r)),
                    ("distance", &format::distance(km, imperial)),
                ],
            ),
            (Some(r), None) => trf(
                "reach.reachLine",
                &[("band", v.band.label()), ("percent", &format::percent(r))],
            ),
            (None, _) => String::new(),
        };
        self.reach_line.set_text(&line);
        self.reach_line
            .set_visible(!line.is_empty() && !self.expanded.get());
        if let Some(r) = reach {
            self.map
                .update_property(&[gtk::accessible::Property::Description(&trf(
                    "map.description",
                    &[("band", v.band.label()), ("percent", &format::percent(r))],
                ))]);
        }

        match &v.probe {
            Some(p) => {
                let args = [
                    ("grid", p.grid.as_str()),
                    ("distance", &format::distance(p.distance_km, imperial)),
                    ("bearing", &format::degrees(p.bearing_deg)),
                ];
                // The point's own chance once worked out, in its traffic-light
                // colour (the bubble is dark, so the dark-theme shades);
                // until then the map's quality there.
                let markup = match (p.chance, p.quality) {
                    (Some(c), _) => {
                        let colour = hfpredict_core::render::semaphore(
                            Some(Quality::of(c)),
                            true,
                            v.colors.low_light,
                        );
                        let percent = format!(
                            "<span foreground=\"#{:06X}\" weight=\"bold\">{}</span>",
                            colour & 0xFF_FFFF,
                            glib::markup_escape_text(&format::percent(c))
                        );
                        let escaped: Vec<(&str, String)> = args
                            .iter()
                            .map(|(k, v)| (*k, glib::markup_escape_text(v).to_string()))
                            .collect();
                        let mut all: Vec<(&str, &str)> =
                            escaped.iter().map(|(k, v)| (*k, v.as_str())).collect();
                        all.push(("percent", &percent));
                        trf("reach.probeChance", &all)
                    }
                    (None, Some(q)) => glib::markup_escape_text(&trf(
                        "reach.probeQuality",
                        &[args[0], args[1], args[2], ("quality", &format::quality(q))],
                    ))
                    .to_string(),
                    (None, None) => {
                        glib::markup_escape_text(&trf("reach.probe", &args)).to_string()
                    }
                };
                self.probe_text.set_markup(&markup);
                self.probe.set_visible(true);
            }
            None => self.probe.set_visible(false),
        }

        if matches!(change, Change::Colors | Change::Saved | Change::Map) {
            self.fill_legend(&v.colors, nvis.is_some());
        }

        // The hour track: PAST_HOURS behind now, then the day ahead.
        if matches!(
            change,
            Change::Saved | Change::Clock | Change::Selection | Change::Forecast
        ) {
            self.syncing.set(true);
            let start = v.track_start();
            self.scale.clear_marks();
            // Legal local time at the station, summer time and half-hour
            // zones included, for the moment each mark stands for.
            let first = v.now - v.now.rem_euclid(3600) - i64::from(PAST_HOURS) * 3600;
            for i in 0..24u32 {
                let utc = (start + i) % 24;
                let labelled = i % 4 == PAST_HOURS % 4;
                let top = if i == PAST_HOURS {
                    Some(tr("time.now"))
                } else if labelled {
                    Some(hfpredict_core::localtime::clock(
                        from.lat,
                        from.lon,
                        first + i64::from(i) * 3600,
                    ))
                } else {
                    None
                };
                let bottom = if labelled {
                    Some(format::hour(utc))
                } else {
                    None
                };
                self.scale
                    .add_mark(f64::from(i), gtk::PositionType::Top, top.as_deref());
                self.scale
                    .add_mark(f64::from(i), gtk::PositionType::Bottom, bottom.as_deref());
            }
            self.scale.set_value(f64::from((v.hour + 24 - start) % 24));
            self.local_label
                .set_text(&trf("time.localAt", &[("place", &from.name)]));
            self.syncing.set(false);
        }
        drop(v);
        self.map.queue_draw();
    }

    /// Full screen: the map takes all the room the window has (the other
    /// cards hide, the window goes full screen); the headline, the probe
    /// and the hour slider stay.
    pub fn is_expanded(&self) -> bool {
        self.expanded.get()
    }

    /// One column or two: the map's shape in the card follows.
    pub fn set_wide(&self, wide: bool) {
        self.wide.set(wide);
        if !self.expanded.get() {
            self.map.set_height_for_width(move |w| card_height(wide, w));
        }
    }

    pub fn set_expanded(&self, on: bool) {
        if on == self.expanded.get() {
            return;
        }
        self.expanded.set(on);
        if on {
            self.map.add_css_class(ENLARGED);
        } else {
            self.map.remove_css_class(ENLARGED);
        }
        if on {
            // The map takes the card's room; the hour slider floats over
            // its foot.
            self.map.set_widths(120, 120);
            self.map.set_height_for_width(|_| 120);
            self.root.remove(&self.hours);
            self.hours.add_css_class("osd");
            self.hours.add_css_class("hfp-floating");
            self.hours.set_valign(gtk::Align::End);
            self.hours.set_margin_start(12);
            self.hours.set_margin_end(12);
            self.hours.set_margin_bottom(12);
            self.overlay.add_overlay(&self.hours);
            self.probe.set_margin_bottom(FLOAT_CLEARANCE);
            self.spinner.set_margin_bottom(FLOAT_CLEARANCE + 6);
        } else {
            self.overlay.remove_overlay(&self.hours);
            self.hours.remove_css_class("osd");
            self.hours.remove_css_class("hfp-floating");
            self.hours.set_valign(gtk::Align::Fill);
            self.hours.set_margin_start(0);
            self.hours.set_margin_end(0);
            self.hours.set_margin_bottom(0);
            self.root.append(&self.hours);
            self.map.set_widths(200, MAP_WIDTH);
            let wide = self.wide.get();
            self.map.set_height_for_width(move |w| card_height(wide, w));
            self.probe.set_margin_bottom(8);
            self.spinner.set_margin_bottom(14);
        }
        self.overlay.set_vexpand(on);
        self.holder.set_vexpand(on);
        self.root.set_vexpand(on);
        for w in [
            self.detail.upcast_ref::<gtk::Widget>(),
            self.legend.upcast_ref(),
        ] {
            w.set_visible(!on);
        }
        self.reach_line
            .set_visible(!on && !self.reach_line.text().is_empty());
        let (icon, tip) = if on {
            ("view-restore-symbolic", tr("map.exitFullscreen"))
        } else {
            ("view-fullscreen-symbolic", tr("map.fullscreen"))
        };
        self.fullscreen.set_icon_name(icon);
        self.fullscreen.set_tooltip_text(Some(&tip));
        self.fullscreen
            .update_property(&[gtk::accessible::Property::Label(&tip)]);
        if let Some(f) = self.on_expand.borrow().as_ref() {
            f(on);
        }
        self.map.queue_resize();
        self.map.queue_draw();
        self.map.grab_focus();
    }

    fn fill_legend(&self, colors: &palette::Colors, nvis: bool) {
        while let Some(c) = self.legend.first_child() {
            self.legend.remove(&c);
        }
        let swatches = |list: Vec<gdk::RGBA>, text: String, round: bool| {
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let c = Canvas::new();
            let n = list.len() as f32;
            c.set_widths(
                (14.0 * n + 3.0 * (n - 1.0)) as i32,
                (14.0 * n + 3.0 * (n - 1.0)) as i32,
            );
            c.set_height_for_width(|_| 14);
            c.set_valign(gtk::Align::Center);
            c.set_draw(move |_, s, _, _| {
                for (i, col) in list.iter().enumerate() {
                    let x = i as f32 * 17.0;
                    canvas::fill_round(
                        s,
                        canvas::rect(x, 0.0, 14.0, 14.0),
                        if round { 7.0 } else { 4.0 },
                        col,
                    );
                }
            });
            b.append(&c);
            b.append(&gtk::Label::new(Some(&text)));
            b
        };
        let argb = |c: u32| {
            gdk::RGBA::new(
                ((c >> 16) & 255) as f32 / 255.0,
                ((c >> 8) & 255) as f32 / 255.0,
                (c & 255) as f32 / 255.0,
                ((c >> 24) & 255) as f32 / 255.0,
            )
        };
        self.legend.append(&swatches(
            vec![colors.cells[1], colors.cells[2], colors.cells[3]],
            tr("reach.legendReach"),
            false,
        ));
        let mut night = argb(colors.map.night);
        night.set_alpha((night.alpha() * 2.0).min(1.0));
        self.legend
            .append(&swatches(vec![night], tr("reach.legendNight"), false));
        if nvis {
            self.legend.append(&swatches(
                vec![argb(colors.map.nvis)],
                tr("reach.legendNvis"),
                true,
            ));
        }
        self.legend
            .update_property(&[gtk::accessible::Property::Label(&tr("a11y.mapLegend"))]);
    }
}
