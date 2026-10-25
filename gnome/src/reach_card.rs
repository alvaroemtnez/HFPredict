//! "Who can hear me now": the headline, the coverage map and the hour
//! track.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, glib, graphene};
use hfpredict_core::app::MapDetail;
use hfpredict_core::projection::MapView;
use hfpredict_core::quality::Quality;

use crate::app::{App, Change, PAST_HOURS};
use crate::canvas::{self, Canvas};
use crate::format;
use crate::i18n::{tr, trf};
use crate::palette;

type ExpandFn = Box<dyn Fn(bool)>;

/// Room the floating hour slider takes at the map's foot, so the probe
/// and the spinner sit above it.
const FLOAT_CLEARANCE: i32 = 128;

/// Marks the map canvas while enlarged.
const ENLARGED: &str = "hfp-map-enlarged";

/// Largest square the enlarged map is rendered at, device pixels.
const FULL_MAX_PX: i32 = 1800;

/// The map's square on the canvas: its side and top-left corner. The
/// shorter side in the card, the longer one enlarged (the rest is cut off).
fn square(c: &Canvas) -> (f64, f64, f64) {
    let full = c.has_css_class(ENLARGED);
    let (w, h) = (f64::from(c.width().max(1)), f64::from(c.height().max(1)));
    let side = if full { w.max(h) } else { w.min(h) };
    (
        side,
        (w - side) / 2.0,
        if full { (h - side) / 2.0 } else { 0.0 },
    )
}

/// Largest the map is drawn, logical pixels.
const MAP_MAX: i32 = 720;

pub struct ReachCard {
    pub root: gtk::Box,
    headline: gtk::Label,
    quality: gtk::Label,
    map: Canvas,
    spinner: adw::Spinner,
    probe: gtk::Label,
    detail: gtk::Label,
    reach_line: gtk::Label,
    legend: gtk::Box,
    scale: gtk::Scale,
    local_label: gtk::Label,
    /// The map's frame: square, as large as the room it is given.
    frame: gtk::AspectFrame,
    /// Holds the square frame, or the bare map when enlarged.
    holder: gtk::Box,
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
    texture: RefCell<Option<(gdk::Texture, MapView)>>,
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
        map.set_widths(200, MAP_MAX);
        map.set_height_for_width(|w| w.min(MAP_MAX));
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

        let probe = gtk::Label::new(None);
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

        let frame = gtk::AspectFrame::new(0.5, 0.0, 1.0, false);
        frame.set_child(Some(&overlay));
        let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        holder.append(&frame);
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
            detail,
            reach_line,
            legend,
            scale,
            local_label,
            frame,
            holder,
            overlay,
            hours,
            fullscreen,
            expanded: Rc::new(Cell::new(false)),
            on_expand: RefCell::new(None),
            syncing: Cell::new(false),
            texture: RefCell::new(None),
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
        // The map draws the last texture, moved and scaled from the view it
        // was rendered for to the view on screen, so a gesture follows the
        // fingers at once while the next render is worked out.
        let me = Rc::downgrade(self);
        let a = Rc::downgrade(app);
        self.map.set_draw(move |_, s, w, h| {
            let (Some(me), Some(app)) = (me.upgrade(), a.upgrade()) else {
                return;
            };
            let full = me.expanded.get();
            let side = if full { w.max(h) } else { w.min(h) };
            let (x0, y0) = ((w - side) / 2.0, (h - side) / 2.0);
            let current = app.view.borrow().map_view;
            // The square in the card; the whole area when enlarged.
            let clip = if full {
                canvas::rect(0.0, 0.0, w, h)
            } else {
                canvas::rect(x0, 0.0, side, side)
            };
            s.push_rounded_clip(&gtk::gsk::RoundedRect::from_rect(clip, 12.0));
            if full {
                let disc = app.view.borrow().colors.map.disc;
                s.append_color(&palette::rgba_of(disc), &clip);
            }
            if let Some((texture, at)) = me.texture.borrow().as_ref() {
                let k = (current.scale / at.scale) as f32;
                let t = |c_at: f64, c_now: f64| {
                    ((c_at - c_now - 0.5 / at.scale + 0.5 / current.scale) * current.scale) as f32
                        * side
                };
                s.save();
                s.translate(&graphene::Point::new(
                    x0 + t(at.cx, current.cx),
                    y0 + t(at.cy, current.cy),
                ));
                s.scale(k, k);
                s.append_texture(texture, &canvas::rect(0.0, 0.0, side, side));
                s.restore();
            }
            s.pop();
        });
        let a = Rc::downgrade(app);
        let full = self.expanded.clone();
        self.map.set_on_resize(move |c, w, h| {
            if let Some(app) = a.upgrade() {
                let side = if full.get() { w.max(h) } else { w.min(h) }.max(1);
                let scale = c.scale_factor().max(1);
                // Enlarged, the square is held to FULL_MAX_PX and scaled up.
                let px = (side * scale).min(if full.get() { FULL_MAX_PX } else { i32::MAX });
                app.set_map_size(px as u32, px as f32 / side as f32);
            }
        });
        let me = Rc::downgrade(self);
        *app.texture_sink.borrow_mut() = Some(Box::new(move |texture, view| {
            if let Some(me) = me.upgrade() {
                *me.texture.borrow_mut() = Some((texture.upcast(), view));
                me.map.queue_draw();
            }
        }));

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
                    app.set_map_view(sv.get().panned(dx, dy, side(&map)), false);
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
                        app.set_map_view(sv.get().panned(dx, dy, side(&map)), true);
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
                    let v = sv
                        .get()
                        .zoomed_at(k, x - offset(&map), y - offset_y(&map), side(&map));
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
                    let v = app.view.borrow().map_view.zoomed_at(
                        if dy < 0.0 { 1.25 } else { 0.8 },
                        x - offset(&map),
                        y - offset_y(&map),
                        side(&map),
                    );
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
                    let s = side(&map);
                    let v = app.view.borrow().map_view.zoomed_at(k, s / 2.0, s / 2.0, s);
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
                let text = match p.quality {
                    Some(q) => trf(
                        "reach.probeQuality",
                        &[args[0], args[1], args[2], ("quality", &format::quality(q))],
                    ),
                    None => trf("reach.probe", &args),
                };
                self.probe.set_text(&text);
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
            let offset = (from.lon / 15.0).round() as i32;
            for i in 0..24u32 {
                let utc = (start + i) % 24;
                let local = (utc as i32 + offset).rem_euclid(24) as u32;
                let labelled = i % 4 == PAST_HOURS % 4;
                let top = if i == PAST_HOURS {
                    Some(tr("time.now"))
                } else if labelled {
                    Some(format::hour(local))
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
            // The map leaves its square frame and takes the card's room;
            // the hour slider floats over its foot.
            self.frame.set_child(None::<&gtk::Widget>);
            self.holder.remove(&self.frame);
            self.holder.append(&self.overlay);
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
            self.holder.remove(&self.overlay);
            self.frame.set_child(Some(&self.overlay));
            self.holder.append(&self.frame);
            self.map.set_widths(200, MAP_MAX);
            self.map.set_height_for_width(|w| w.min(MAP_MAX));
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
