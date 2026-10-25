//! HFPredict for GNOME.

mod app;
mod bands_card;
mod canvas;
mod dialogs;
mod first_run;
mod format;
mod i18n;
mod location;
mod palette;
mod reach_card;
mod station;
mod sun_card;
mod window;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, gio, glib};
use hfpredict_core::Core;
use hfpredict_core::quality::Quality;

use crate::app::{App, Change};
use crate::dialogs::APP_ID;
use crate::window::Window;

const STYLE: &str = r#"
.hfp-card { padding: 18px; border-radius: 16px; }
.hfp-pill { border-radius: 999px; padding: 3px 12px; font-weight: bold; }
.hfp-status { border-radius: 999px; padding: 1px 10px; font-size: 0.8em; font-weight: bold; letter-spacing: 0.05em; border: 1px solid currentColor; }
.hfp-live { color: var(--success-color); }
.hfp-offline { color: var(--warning-color); }
.hfp-bar { padding: 4px 10px 10px 10px; }
.hfp-strip { margin-bottom: 2px; }
button.hfp-chip { border-radius: 999px; padding: 3px 12px; min-height: 26px; font-weight: normal; }
togglebutton.hfp-band, button.hfp-band { border-radius: 999px; padding: 4px 14px; min-height: 30px; font-weight: bold; font-feature-settings: "tnum"; }
togglebutton.hfp-chip-toggle { border-radius: 999px; padding: 4px 14px; }
togglebutton.hfp-band:checked, togglebutton.hfp-chip-toggle:checked { background: var(--accent-bg-color); color: var(--accent-fg-color); }
.hfp-inset { padding: 10px 12px; border-radius: 12px; background: alpha(currentColor, 0.05); }
.hfp-probe { padding: 4px 10px; border-radius: 8px; font-feature-settings: "tnum"; }
.hfp-title-button { padding: 2px 12px; }
.hfp-floating { border-radius: 18px; padding: 8px 14px 6px 14px; }
scale.hfp-hours marks label { font-size: 0.85em; font-feature-settings: "tnum"; }
.hfp-table label { font-feature-settings: "tnum"; }
"#;

/// Dim reds on black for field use at night: every libadwaita colour
/// variable, redefined.
const LOW_LIGHT: &str = r#"
:root {
  --accent-color: #ff5a40; --accent-bg-color: #6e140c; --accent-fg-color: #ffb4a6;
  --destructive-color: #ff5a40; --destructive-bg-color: #6e140c; --destructive-fg-color: #ffb4a6;
  --success-color: #ff5a40; --warning-color: #c2351f; --error-color: #ff5a40;
  --window-bg-color: #000000; --window-fg-color: #d9402a;
  --view-bg-color: #040000; --view-fg-color: #d9402a;
  --headerbar-bg-color: #070000; --headerbar-fg-color: #d9402a; --headerbar-border-color: #2a0703;
  --headerbar-backdrop-color: #000000; --headerbar-shade-color: rgba(0,0,0,0.6);
  --sidebar-bg-color: #050000; --sidebar-fg-color: #d9402a;
  --card-bg-color: #100201; --card-fg-color: #d9402a; --card-shade-color: rgba(0,0,0,0.8);
  --dialog-bg-color: #070000; --dialog-fg-color: #d9402a;
  --popover-bg-color: #0c0101; --popover-fg-color: #d9402a;
  --thumbnail-bg-color: #0c0101; --thumbnail-fg-color: #d9402a;
  --shade-color: rgba(0,0,0,0.7); --scrollbar-outline-color: #000000;
}
scale slider { background: #6e140c; box-shadow: none; outline-color: #ff5a40; }
scale trough, scale highlight { background: #2a0703; }
scale highlight { background: #6e140c; }
.osd, button.osd { background: rgba(20, 2, 1, 0.85); color: #d9402a; }
checkbutton check, switch { background: #2a0703; color: #ffb4a6; }
switch:checked, checkbutton check:checked { background: #6e140c; }
switch slider { background: #d9402a; }
entry, spinbutton, searchbar entry { background: #120202; color: #d9402a; }
"#;

thread_local! {
    static LOW_LIGHT_CSS: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
    static DYNAMIC_CSS: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
}

pub fn set_low_light(on: bool) {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    LOW_LIGHT_CSS.with(|cell| {
        let mut cell = cell.borrow_mut();
        if let Some(p) = cell.take() {
            gtk::style_context_remove_provider_for_display(&display, &p);
        }
        if on {
            let p = gtk::CssProvider::new();
            p.load_from_string(LOW_LIGHT);
            gtk::style_context_add_provider_for_display(
                &display,
                &p,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
            *cell = Some(p);
        }
    });
}

/// The quality pill's colours follow the palette.
fn set_dynamic_css(app: &App) {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let c = app.view.borrow().colors;
    let text_on = |bg: &gdk::RGBA| {
        let l = 0.2126 * bg.red() + 0.7152 * bg.green() + 0.0722 * bg.blue();
        if l > 0.5 {
            "rgba(0,0,0,0.85)"
        } else {
            "#ffffff"
        }
    };
    let mut css = String::new();
    for (q, i) in [
        ("closed", 0usize),
        ("weak", 1),
        ("patchy", 2),
        ("reliable", 3),
    ] {
        let bg = c.cells[i];
        let fg = if i == 0 {
            "inherit".to_string()
        } else {
            text_on(&bg).to_string()
        };
        css.push_str(&format!(
            ".hfp-q-{q} {{ background: {}; color: {fg}; }}\n",
            palette::css(&bg)
        ));
    }
    // Traffic-light text (the band table's numbers, the probe's chance).
    for q in [
        Quality::Reliable,
        Quality::Patchy,
        Quality::Weak,
        Quality::Closed,
    ] {
        css.push_str(&format!(
            ".hfp-sem-{} {{ color: {}; font-weight: bold; }}\n",
            q.key(),
            palette::css(&c.semaphore(Some(q)))
        ));
    }
    DYNAMIC_CSS.with(|cell| {
        let mut cell = cell.borrow_mut();
        let p = cell.get_or_insert_with(|| {
            let p = gtk::CssProvider::new();
            gtk::style_context_add_provider_for_display(
                &display,
                &p,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            p
        });
        p.load_from_string(&css);
    });
}

fn dirs() -> (PathBuf, PathBuf) {
    let env = |k: &str| std::env::var_os(k).map(PathBuf::from);
    (
        env("HFPREDICT_DATA_DIR").unwrap_or_else(|| glib::user_data_dir().join("hfpredict")),
        env("HFPREDICT_CACHE_DIR").unwrap_or_else(|| glib::user_cache_dir().join("hfpredict")),
    )
}

fn main() -> glib::ExitCode {
    let (data, cache) = dirs();
    let core = Arc::new(Core::open(data, cache));
    // The time-zone map takes a moment to load: load it before it is needed.
    std::thread::spawn(|| hfpredict_core::localtime::zone_name(0.0, 0.0));
    let lang = i18n::resolve(core.state().settings.language.as_deref());
    i18n::init(&lang);

    let application = adw::Application::builder().application_id(APP_ID).build();
    let core2 = core.clone();
    application.connect_startup(move |_| {
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let css = gtk::CssProvider::new();
        css.load_from_string(STYLE);
        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        // Development runs find the icons in the source tree; installed
        // builds have them in the icon theme.
        let theme = gtk::IconTheme::for_display(&display);
        theme.add_search_path(concat!(env!("CARGO_MANIFEST_DIR"), "/data/icons"));
        dialogs::apply_theme(core2.state().settings.theme);
    });
    application.connect_activate(move |application| {
        if let Some(w) = application.active_window() {
            w.present();
            return;
        }
        // The app's language decides the direction, not the system's: GTK
        // sets it from the locale during start-up, so this comes after.
        gtk::Widget::set_default_direction(if i18n::is_rtl() {
            gtk::TextDirection::Rtl
        } else {
            gtk::TextDirection::Ltr
        });
        let app = App::new(core.clone());
        let w = window::build(application, &app);
        // Every handler holds the window weakly; the window itself holds the
        // one strong reference until it is destroyed (GTK drops signal
        // handlers on dispose, which breaks the cycle).
        app::keep_alive(&w.window, w.clone());
        install_actions(application, &w);
        set_dynamic_css(&app);
        app.listen(|app, change| {
            if change == Change::Colors {
                set_dynamic_css(app);
            }
        });
        app.reload();
        app.refresh_readings();
        start_timers(&app);
        w.window.present();
        if std::env::var_os("HFPREDICT_DEBUG").is_some() {
            eprintln!(
                "hfpredict: direction default={:?} window={:?} lang={}",
                gtk::Widget::default_direction(),
                w.window.direction(),
                i18n::lang()
            );
        }
        screenshot_hook(&w);
        drop(w);
    });
    application.run()
}

type WindowAction = Box<dyn Fn(&Rc<Window>)>;

fn install_actions(application: &adw::Application, w: &Rc<Window>) {
    let add = |name: &str, f: WindowAction| {
        let action = gio::SimpleAction::new(name, None);
        let w = Rc::downgrade(w);
        action.connect_activate(move |_, _| {
            if let Some(w) = w.upgrade() {
                f(&w);
            }
        });
        application.add_action(&action);
    };
    add("preferences", Box::new(dialogs::preferences));
    add("help", Box::new(dialogs::help));
    add("about", Box::new(dialogs::about));
    add("station", Box::new(station::open));
    add("path", Box::new(|w| location::open(w, location::End::From)));
    let quit = gio::SimpleAction::new("quit", None);
    let app2 = application.clone();
    quit.connect_activate(move |_, _| app2.quit());
    application.add_action(&quit);
    application.set_accels_for_action("app.preferences", &["<Control>comma"]);
    application.set_accels_for_action("app.help", &["F1"]);
    application.set_accels_for_action("app.quit", &["<Control>q"]);
    application.set_accels_for_action("app.station", &["<Control>e"]);
    application.set_accels_for_action("app.path", &["<Control>l"]);
    application.set_accels_for_action("win.refresh", &["<Control>r", "F5"]);
    application.set_accels_for_action("win.map-fullscreen", &["F11"]);
    application.set_accels_for_action("window.close", &["<Control>w"]);
}

fn start_timers(app: &Rc<App>) {
    let a = Rc::downgrade(app);
    glib::timeout_add_seconds_local(60, move || {
        let Some(app) = a.upgrade() else {
            return glib::ControlFlow::Break;
        };
        app.tick();
        glib::ControlFlow::Continue
    });
    // NOAA publishes the flux daily and Kp every three hours, and GIRO a
    // sounding every quarter hour: half an hour is plenty.
    let a = Rc::downgrade(app);
    glib::timeout_add_seconds_local(30 * 60, move || {
        let Some(app) = a.upgrade() else {
            return glib::ControlFlow::Break;
        };
        app.refresh_readings();
        glib::ControlFlow::Continue
    });
}

/// For UI tests: `HFPREDICT_SCREENSHOT=out.png` saves the window after
/// `HFPREDICT_SCREENSHOT_DELAY` seconds (default 8) and quits.
/// `HFPREDICT_SIZE=WxH` sizes the window, `HFPREDICT_OPEN=station|path|
/// preferences|help|about|fullscreen` opens a dialog (or the full-screen
/// map) first, `HFPREDICT_BAND`, `HFPREDICT_HOUR`, `HFPREDICT_ZOOM` set
/// the view and `HFPREDICT_PROBE=x,y` (fractions of the map) taps it.
fn screenshot_hook(w: &Rc<Window>) {
    let Ok(path) = std::env::var("HFPREDICT_SCREENSHOT") else {
        return;
    };
    if let Some((width, height)) = std::env::var("HFPREDICT_SIZE").ok().and_then(|s| {
        let (a, b) = s.split_once('x')?;
        Some((a.parse::<i32>().ok()?, b.parse::<i32>().ok()?))
    }) {
        w.window.set_default_size(width, height);
    }
    let delay: u64 = std::env::var("HFPREDICT_SCREENSHOT_DELAY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);
    let band = std::env::var("HFPREDICT_BAND").ok().and_then(|s| {
        hfpredict_core::bands::Band::DISPLAY
            .into_iter()
            .find(|b| b.label() == s)
    });
    let hour = std::env::var("HFPREDICT_HOUR")
        .ok()
        .and_then(|s| s.parse::<u32>().ok());
    if band.is_some() || hour.is_some() {
        let (b0, h0) = {
            let v = w.app.view.borrow();
            (v.band, v.hour)
        };
        w.app.select(band.unwrap_or(b0), hour.unwrap_or(h0));
    }
    if let Some(zoom) = std::env::var("HFPREDICT_ZOOM")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
    {
        w.app.set_map_view(
            hfpredict_core::projection::MapView {
                scale: zoom,
                cx: 0.5,
                cy: 0.5,
            },
            true,
        );
    }
    // Dialogs open through the same actions the menu and the buttons use,
    // and the hook holds the window weakly, so a screenshot shows what a
    // reader would get.
    if let Some((x, y)) = std::env::var("HFPREDICT_PROBE").ok().and_then(|s| {
        let (a, b) = s.split_once(',')?;
        Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?))
    }) {
        let app = w.app.clone();
        glib::timeout_add_local_once(Duration::from_secs(delay.saturating_sub(2)), move || {
            app.probe(x * 600.0, y * 600.0, 600.0);
        });
    }
    let open = std::env::var("HFPREDICT_OPEN").unwrap_or_default();
    let application = w.window.application();
    let weak = Rc::downgrade(w);
    glib::timeout_add_local_once(Duration::from_millis(1500), move || {
        let action = match open.as_str() {
            "station" | "preferences" | "help" | "about" => open.as_str(),
            "fullscreen" => {
                if let Some(w) = weak.upgrade() {
                    let _ = WidgetExt::activate_action(&w.window, "win.map-fullscreen", None);
                }
                return;
            }
            "path" => {
                if let Some(w) = weak.upgrade() {
                    location::open(&w, location::End::To);
                }
                return;
            }
            _ => return,
        };
        if let Some(a) = application {
            a.activate_action(action, None);
        }
    });
    // `HFPREDICT_SEARCH=text` types into the first search box on screen
    // (first run or the path dialog), as a reader would.
    if let Ok(text) = std::env::var("HFPREDICT_SEARCH") {
        let window = w.window.clone();
        glib::timeout_add_local_once(Duration::from_millis(3000), move || {
            fn find(w: &gtk::Widget) -> Option<gtk::SearchEntry> {
                if let Some(e) = w.downcast_ref::<gtk::SearchEntry>()
                    && e.is_mapped()
                {
                    return Some(e.clone());
                }
                let mut child = w.first_child();
                while let Some(c) = child {
                    if let Some(e) = find(&c) {
                        return Some(e);
                    }
                    child = c.next_sibling();
                }
                None
            }
            if let Some(e) = find(window.upcast_ref()) {
                e.set_text(&text);
            }
        });
    }
    let window = w.window.clone();
    glib::timeout_add_local_once(Duration::from_secs(delay), move || {
        let window = &window;
        let (width, height) = (window.width(), window.height());
        let paintable = gtk::WidgetPaintable::new(Some(window));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
        if let (Some(node), Some(renderer)) = (snapshot.to_node(), window.renderer()) {
            let texture = renderer.render_texture(
                &node,
                Some(&gtk::graphene::Rect::new(
                    0.0,
                    0.0,
                    width as f32,
                    height as f32,
                )),
            );
            if let Err(e) = texture.save_to_png(&path) {
                eprintln!("hfpredict: screenshot: {e}");
            }
        }
        if let Some(a) = window.application() {
            a.quit();
        }
    });
}
