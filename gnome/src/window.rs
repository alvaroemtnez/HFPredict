//! The main window: the path and station on top, the band chips, then the
//! map beside (or above) the band grid.

use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;
use hfpredict_core::antenna::{self, AntennaKind};
use hfpredict_core::bands::Band;
use hfpredict_core::quality::Quality;

use crate::app::{App, Change};
use crate::bands_card::BandsCard;
use crate::canvas::{self, Canvas};
use crate::format;
use crate::i18n::{tr, trf};
use crate::reach_card::ReachCard;
use crate::sun_card::SunCard;
use crate::{first_run, location, station};

/// Two columns from this width on.
const WIDE: &str = "min-width: 960sp";

pub struct Window {
    pub app: Rc<App>,
    /// The cards: their listeners only hold weak references, so the window
    /// keeps them alive.
    keep: std::cell::RefCell<Vec<Rc<dyn std::any::Any>>>,
    pub window: adw::ApplicationWindow,
    pub toasts: adw::ToastOverlay,
    stack: gtk::Stack,
}

pub fn build(application: &adw::Application, app: &Rc<App>) -> Rc<Window> {
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title(tr("app.name"))
        .default_width(1180)
        .default_height(860)
        .width_request(360)
        .height_request(480)
        .build();
    let toasts = adw::ToastOverlay::new();
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    toasts.set_child(Some(&stack));
    window.set_content(Some(&toasts));

    let w = Rc::new(Window {
        app: app.clone(),
        keep: Default::default(),
        window: window.clone(),
        toasts,
        stack: stack.clone(),
    });

    stack.add_named(&main_page(app, &w), Some("main"));
    stack.add_named(&first_run::page(&w), Some("first-run"));
    let show = {
        let stack = stack.clone();
        move |app: &App| {
            let first = app.view.borrow().saved.from.is_none();
            stack.set_visible_child_name(if first { "first-run" } else { "main" });
        }
    };
    show(app);
    app.listen(move |app, change| {
        if change == Change::Saved {
            show(app);
        }
    });

    // The window follows the desktop's accent and dark style.
    let sm = adw::StyleManager::default();
    for signal in ["notify::accent-color-rgba", "notify::dark"] {
        let a = Rc::downgrade(app);
        sm.connect_local(signal, false, move |_| {
            if let Some(app) = a.upgrade() {
                app.restyle();
            }
            None
        });
    }
    w
}

impl Window {
    pub fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(text));
    }

    pub fn show_main(&self) {
        self.stack.set_visible_child_name("main");
    }
}

fn main_page(app: &Rc<App>, w: &Rc<Window>) -> gtk::Widget {
    let view = adw::ToolbarView::new();
    view.set_top_bar_style(adw::ToolbarStyle::Raised);

    // Header: the path is the title, and opens the path picker.
    let header = adw::HeaderBar::new();
    let title = gtk::Label::new(None);
    title.add_css_class("heading");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let subtitle = gtk::Label::new(None);
    subtitle.add_css_class("caption");
    subtitle.add_css_class("dim-label");
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let titles = gtk::Box::new(gtk::Orientation::Vertical, 0);
    titles.set_valign(gtk::Align::Center);
    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    title_row.set_halign(gtk::Align::Center);
    title_row.append(&title);
    title_row.append(&gtk::Image::from_icon_name("pan-down-symbolic"));
    titles.append(&title_row);
    titles.append(&subtitle);
    let path_button = gtk::Button::builder().child(&titles).build();
    path_button.add_css_class("flat");
    path_button.add_css_class("hfp-title-button");
    {
        let a = Rc::downgrade(app);
        let w = Rc::downgrade(w);
        path_button.connect_clicked(move |_| {
            if let (Some(app), Some(w)) = (a.upgrade(), w.upgrade()) {
                let _ = app;
                location::open(&w, location::End::From);
            }
        });
    }
    header.set_title_widget(Some(&path_button));

    let menu = gio::Menu::new();
    let section = gio::Menu::new();
    section.append(Some(&tr("menu.refresh")), Some("win.refresh"));
    menu.append_section(None, &section);
    let section = gio::Menu::new();
    section.append(Some(&tr("menu.preferences")), Some("app.preferences"));
    section.append(Some(&tr("menu.help")), Some("app.help"));
    section.append(Some(&tr("menu.about")), Some("app.about"));
    menu.append_section(None, &section);
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .primary(true)
        .tooltip_text(tr("menu.open"))
        .build();
    header.pack_end(&menu_button);

    let pill = gtk::Label::new(None);
    pill.add_css_class("hfp-status");
    pill.set_valign(gtk::Align::Center);
    header.pack_end(&pill);
    view.add_top_bar(&header);

    // The station and the band chips, under the header.
    let bar = gtk::Box::new(gtk::Orientation::Vertical, 6);
    bar.add_css_class("hfp-bar");
    let strip = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let strip_scroll = gtk::ScrolledWindow::builder()
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&strip)
        .build();
    strip_scroll.add_css_class("hfp-strip");
    bar.append(&strip_scroll);
    let bands = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    bands.set_halign(gtk::Align::Center);
    let band_scroll = gtk::ScrolledWindow::builder()
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&bands)
        .build();
    bar.append(&band_scroll);
    view.add_top_bar(&bar);

    let mut chips: Vec<(Band, gtk::ToggleButton, Canvas)> = vec![];
    let mut group: Option<gtk::ToggleButton> = None;
    for band in Band::DISPLAY {
        let label = gtk::Label::new(Some(band.label()));
        let dot = Canvas::new();
        dot.set_widths(8, 8);
        dot.set_height_for_width(|_| 8);
        dot.set_valign(gtk::Align::Center);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        content.append(&dot);
        content.append(&label);
        let b = gtk::ToggleButton::builder().child(&content).build();
        b.add_css_class("hfp-band");
        b.update_property(&[gtk::accessible::Property::Label(&trf(
            "a11y.pinBand",
            &[("band", band.label())],
        ))]);
        if let Some(g) = &group {
            b.set_group(Some(g));
        } else {
            group = Some(b.clone());
        }
        let a = Rc::downgrade(app);
        b.connect_toggled(move |b| {
            if let Some(app) = a.upgrade()
                && b.is_active()
                && app.view.borrow().band != band
            {
                let hour = app.view.borrow().hour;
                app.select(band, hour);
            }
        });
        bands.append(&b);
        chips.push((band, b, dot));
    }

    // The two cards, side by side when wide.
    let reach = ReachCard::new(app);
    let all = BandsCard::new(app);
    let sun = SunCard::new(app);
    w.keep.borrow_mut().extend([
        reach.clone() as Rc<dyn std::any::Any>,
        all.clone(),
        sun.clone(),
    ]);
    let columns = gtk::Box::new(gtk::Orientation::Vertical, 18);
    columns.set_homogeneous(false);
    reach.root.set_hexpand(true);
    all.root.set_hexpand(true);
    all.root.set_valign(gtk::Align::Start);
    columns.append(&reach.root);
    columns.append(&all.root);
    let page = gtk::Box::new(gtk::Orientation::Vertical, 18);
    page.set_margin_top(18);
    page.set_margin_bottom(24);
    page.set_margin_start(12);
    page.set_margin_end(12);
    page.append(&columns);
    page.append(&sun.root);
    let clamp = adw::Clamp::builder()
        .maximum_size(1500)
        .tightening_threshold(1200)
        .child(&page)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&clamp)
        .vexpand(true)
        .build();
    view.set_content(Some(&scroll));

    // The enlarged map hides everything else on the page; the window
    // stays as it is.
    {
        let (all, sun) = (all.root.clone(), sun.root.clone());
        let strip = strip_scroll.clone();
        *reach.on_expand.borrow_mut() = Some(Box::new(move |on| {
            all.set_visible(!on);
            sun.set_visible(!on);
            strip.set_visible(!on);
        }));
    }
    let toggle = gio::SimpleAction::new("map-fullscreen", None);
    {
        let r = Rc::downgrade(&reach);
        toggle.connect_activate(move |_, _| {
            if let Some(r) = r.upgrade() {
                r.set_expanded(!r.is_expanded());
            }
        });
    }
    w.window.add_action(&toggle);
    let breakpoint =
        adw::Breakpoint::new(adw::BreakpointCondition::parse(WIDE).expect("valid condition"));
    breakpoint.add_setter(
        &columns,
        "orientation",
        Some(&gtk::Orientation::Horizontal.to_value()),
    );
    breakpoint.add_setter(&columns, "homogeneous", Some(&true.to_value()));
    w.window.add_breakpoint(breakpoint);

    // Actions on the window.
    let refresh = gio::SimpleAction::new("refresh", None);
    {
        let a = Rc::downgrade(app);
        let w = Rc::downgrade(w);
        refresh.connect_activate(move |_, _| {
            if let (Some(app), Some(w)) = (a.upgrade(), w.upgrade()) {
                if app.view.borrow().saved.settings.network {
                    app.refresh_readings();
                } else {
                    w.toast(&tr("spaceWeather.networkOff"));
                }
            }
        });
    }
    w.window.add_action(&refresh);

    let strip_ref = strip.clone();
    let update = {
        let w = Rc::downgrade(w);
        move |app: &App, change: Change| {
            let v = app.view.borrow();
            let from = v
                .saved
                .from
                .clone()
                .unwrap_or_else(hfpredict_core::places::Place::greenwich);
            let to = v.saved.to.clone();
            title.set_text(&match &to {
                Some(t) => format!("{} → {}", from.name, t.name),
                None => from.name.clone(),
            });
            let imperial = v.imperial();
            subtitle.set_text(&match v.forecast.as_ref().filter(|f| f.to.is_some()) {
                Some(f) => {
                    let hops = f.hops.unwrap_or(1);
                    let args = [
                        (
                            "distance",
                            format::distance(f.distance_km.unwrap_or(0.0), imperial),
                        ),
                        ("bearing", format::degrees(f.bearing_deg.unwrap_or(0.0))),
                        ("hops", hops.to_string()),
                    ];
                    let args: Vec<(&str, &str)> =
                        args.iter().map(|(k, v)| (*k, v.as_str())).collect();
                    trf(
                        if hops == 1 {
                            "path.summaryOneHop"
                        } else {
                            "path.summary"
                        },
                        &args,
                    )
                }
                None => tr("path.everyDirection"),
            });
            path_button.update_property(&[gtk::accessible::Property::Label(&trf(
                "a11y.changeLocation",
                &[("place", &from.name)],
            ))]);

            let live = app.core.is_nowcast(v.now);
            pill.set_text(&if live {
                tr("status.live")
            } else {
                tr("status.offline")
            });
            pill.set_css_classes(&["hfp-status", if live { "hfp-live" } else { "hfp-offline" }]);
            let hint = match (&v.saved.space_weather, live) {
                (Some(sw), true) => {
                    trf("status.liveHint", &[("time", &format::hm(sw.observed_at))])
                }
                _ => tr("status.offlineHint"),
            };
            pill.set_tooltip_text(Some(&format!("{hint}\n\n{}", tr("status.liveExplain"))));

            for (band, button, dot) in &chips {
                if *band == v.band && !button.is_active() {
                    button.set_active(true);
                }
                let r = v
                    .forecast
                    .as_ref()
                    .and_then(|f| f.cell(*band, v.hour))
                    .map(|c| c.reliability);
                let colour = r.map(|r| v.colors.cell(Quality::of(r)));
                let ring = v.colors.dim;
                dot.set_draw(move |_, s, w, h| {
                    match colour {
                        Some(c) => canvas::fill_round(s, canvas::rect(0.0, 0.0, w, h), w / 2.0, &c),
                        None => canvas::outline_round(
                            s,
                            canvas::rect(0.0, 0.0, w, h),
                            w / 2.0,
                            1.0,
                            &ring,
                        ),
                    }
                    if colour.is_some_and(|c| c.alpha() < 0.2) {
                        canvas::outline_round(s, canvas::rect(0.0, 0.0, w, h), w / 2.0, 1.0, &ring);
                    }
                });
            }

            if matches!(change, Change::Saved | Change::Forecast) {
                drop(v);
                if let Some(w) = w.upgrade() {
                    fill_strip(app, &w, &strip_ref);
                }
            }
        }
    };
    update(app, Change::Saved);
    app.listen(update);
    view.upcast()
}

/// The station, as a row of chips that each open the station editor.
fn fill_strip(app: &App, w: &Rc<Window>, strip: &gtk::Box) {
    while let Some(c) = strip.first_child() {
        strip.remove(&c);
    }
    let v = app.view.borrow();
    let st = v.saved.station();
    let imperial = v.imperial();
    let mode = tr(&format!("station.mode.{}", st.mode.key()));
    let db = st.mode.required_snr_db().to_string();
    let antenna_name = tr(&format!("station.antenna.{}", antenna_key(st.antenna.kind)));
    let mut chips: Vec<(String, &str, String)> = vec![];
    let name = if st.name.is_empty() {
        tr("station.unnamed")
    } else {
        st.name.clone()
    };
    if v.saved.stations.len() > 1 || !st.name.is_empty() {
        chips.push((name, "radio-symbolic", tr("station.strip.hint")));
    }
    chips.push((
        mode.clone(),
        "",
        trf("station.strip.mode", &[("mode", &mode), ("db", &db)]),
    ));
    chips.push((
        format::watts(st.watts),
        "",
        trf("station.strip.power", &[("watts", &format::int(st.watts))]),
    ));
    if st.antenna.kind == AntennaKind::Isotropic {
        chips.push((antenna_name, "", tr("station.strip.isotropic")));
    } else {
        let h = format::height(st.antenna.height_m, imperial);
        chips.push((
            format!("{antenna_name} · {h}"),
            "",
            trf(
                "station.strip.antenna",
                &[("antenna", &antenna_name), ("height", &h)],
            ),
        ));
        if st.antenna.kind.uses_beam() {
            let lobes: Vec<String> = antenna::lobes(st.antenna.beam_deg, st.antenna.kind)
                .iter()
                .map(|d| format::degrees(*d))
                .collect();
            let text = format!("{}°", lobes.join("/"));
            chips.push((
                text.clone(),
                "",
                trf("station.strip.lobes", &[("lobes", &text)]),
            ));
        }
    }
    drop(v);
    for (text, _icon, tip) in chips {
        let b = gtk::Button::with_label(&text);
        b.add_css_class("hfp-chip");
        b.set_tooltip_text(Some(&tip));
        let wk = Rc::downgrade(w);
        b.connect_clicked(move |_| {
            if let Some(w) = wk.upgrade() {
                station::open(&w);
            }
        });
        strip.append(&b);
    }
}

pub fn antenna_key(kind: AntennaKind) -> &'static str {
    match kind {
        AntennaKind::Isotropic => "isotropic",
        AntennaKind::Dipole => "dipole",
        AntennaKind::InvertedV => "invertedV",
        AntennaKind::Vertical => "vertical",
        AntennaKind::InvertedL => "invertedL",
        AntennaKind::Yagi => "yagi",
    }
}
