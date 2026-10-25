//! The app's state and everything that changes it.
//!
//! One `App` per window. Core calls run on worker threads
//! (`gio::spawn_blocking`) and their answers come back to the main loop,
//! where a generation number throws away any answer a newer request has
//! overtaken (the reader moved the slider again before the map arrived).

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::{gio, glib};
use hfpredict_core::Core;
use hfpredict_core::app::{BandReach, MapDetail, MapOptions, MapStatus, Probe};
use hfpredict_core::bands::Band;
use hfpredict_core::forecast::Forecast;
use hfpredict_core::projection::MapView;
use hfpredict_core::store::{SavedState, Settings, Theme};

use crate::palette::{self, Colors};

/// How many hours behind "now" the hour track starts.
pub const PAST_HOURS: u32 = 6;

/// How long the hour must stay put before a fine map is worked out.
const FINE_DELAY_MS: u64 = 280;

/// How long after the last move a pan or zoom counts as settled.
const SETTLE_MS: u64 = 180;

/// `HFPREDICT_DEBUG=1` prints how long each step takes.
pub fn debug(what: &str, started: std::time::Instant) {
    if std::env::var_os("HFPREDICT_DEBUG").is_some() {
        eprintln!("hfpredict: {what}: {} ms", started.elapsed().as_millis());
    }
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Keeps `value` alive as long as `widget` lives. UI objects here hand
/// their signal handlers weak references to themselves, so something must
/// hold the strong one: the widget they belong to (GTK drops the handler,
/// and with it the value, when the widget is destroyed).
pub fn keep_alive<T: 'static>(widget: &impl gtk::prelude::IsA<gtk::Widget>, value: Rc<T>) {
    use gtk::prelude::WidgetExt;
    let cell = RefCell::new(Some(value));
    widget.as_ref().connect_destroy(move |_| {
        cell.borrow_mut().take();
    });
}

/// Runs `work` on a worker thread and `done` with its answer on the main
/// loop. A panic in `work` is dropped (the UI shows the previous state).
pub fn spawn<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(T) + 'static,
) {
    glib::spawn_future_local(async move {
        if let Ok(v) = gio::spawn_blocking(work).await {
            done(v);
        }
    });
}

/// What the window is showing.
pub struct View {
    pub saved: SavedState,
    pub forecast: Option<Forecast>,
    pub forecast_error: Option<String>,
    pub band: Band,
    /// UTC hour on screen.
    pub hour: u32,
    pub now: i64,
    pub map_view: MapView,
    pub reach: Vec<BandReach>,
    pub status: Option<MapStatus>,
    pub map_detail: MapDetail,
    pub map_working: bool,
    pub map_error: Option<String>,
    pub probe: Option<Probe>,
    pub show_table: bool,
    pub colors: Colors,
}

impl View {
    pub fn now_hour(&self) -> u32 {
        (self.now.rem_euclid(86_400) / 3600) as u32
    }

    /// The first hour on the track.
    pub fn track_start(&self) -> u32 {
        (self.now_hour() + 24 - PAST_HOURS) % 24
    }

    pub fn imperial(&self) -> bool {
        let locale = glib::language_names()
            .first()
            .map(|s| s.to_string())
            .unwrap_or_default();
        self.saved.settings.units.imperial_for(&locale)
    }

    pub fn low_light(&self) -> bool {
        self.saved.settings.theme == Theme::LowLight
    }
}

/// Something the window redraws when the state changes.
pub type Listener = Box<dyn Fn(&App, Change)>;

/// Receives each rendered map and the view it was rendered for.
pub type TextureSink = Box<dyn Fn(gtk::gdk::MemoryTexture, MapView)>;

/// What changed, so listeners redraw only their part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Settings, stations or places: almost everything.
    Saved,
    Forecast,
    /// Band or hour.
    Selection,
    /// The map image or its status lines.
    Map,
    Readings,
    Colors,
    Clock,
}

pub struct App {
    pub core: Arc<Core>,
    pub view: RefCell<View>,
    listeners: RefCell<Vec<Listener>>,
    forecast_gen: Cell<u64>,
    map_gen: Cell<u64>,
    /// The map widget's size in device pixels, and whether a render is
    /// running / wanted again.
    map_px: Cell<u32>,
    density: Cell<f32>,
    rendering: Cell<bool>,
    render_again: Cell<bool>,
    /// A gesture is under way: render at half size, and settle shortly
    /// after the last move.
    draft: Cell<bool>,
    settle: RefCell<Option<glib::SourceId>>,
    pub texture_sink: RefCell<Option<TextureSink>>,
    me: RefCell<Weak<App>>,
}

impl App {
    pub fn new(core: Arc<Core>) -> Rc<App> {
        let saved = core.state();
        let now = unix_now();
        let low = saved.settings.theme == Theme::LowLight;
        let band = saved.band;
        let app = Rc::new(App {
            core,
            view: RefCell::new(View {
                band,
                hour: (now.rem_euclid(86_400) / 3600) as u32,
                now,
                saved,
                forecast: None,
                forecast_error: None,
                map_view: MapView::default(),
                reach: vec![],
                status: None,
                map_detail: MapDetail::None,
                map_working: false,
                map_error: None,
                probe: None,
                show_table: false,
                colors: palette::current(low),
            }),
            listeners: RefCell::new(vec![]),
            forecast_gen: Cell::new(0),
            map_gen: Cell::new(0),
            map_px: Cell::new(0),
            density: Cell::new(1.0),
            rendering: Cell::new(false),
            render_again: Cell::new(false),
            draft: Cell::new(false),
            settle: RefCell::new(None),
            texture_sink: RefCell::new(None),
            me: RefCell::new(Weak::new()),
        });
        *app.me.borrow_mut() = Rc::downgrade(&app);
        app
    }

    fn rc(&self) -> Rc<App> {
        self.me
            .borrow()
            .upgrade()
            .expect("the app outlives its callbacks")
    }

    pub fn listen(&self, f: impl Fn(&App, Change) + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    pub fn emit(&self, change: Change) {
        for l in self.listeners.borrow().iter() {
            l(self, change);
        }
    }

    // ---- Saved state ---------------------------------------------------

    /// Applies a change to the saved state through the core (which saves
    /// it), then recomputes what depends on it.
    pub fn save(&self, change: impl FnOnce(&Core) -> Result<SavedState, hfpredict_core::Error>) {
        match change(&self.core) {
            Ok(saved) => {
                self.view.borrow_mut().saved = saved;
                self.emit(Change::Saved);
                self.reload();
            }
            Err(e) => eprintln!("hfpredict: could not save: {e}"),
        }
    }

    pub fn set_settings(&self, settings: Settings) {
        let theme_changed = self.view.borrow().saved.settings.theme != settings.theme;
        self.save(|c| c.set_settings(settings));
        if theme_changed {
            self.restyle();
        }
    }

    /// Re-reads the desktop's colours (accent, dark, low light).
    pub fn restyle(&self) {
        let low = self.view.borrow().low_light();
        self.view.borrow_mut().colors = palette::current(low);
        self.emit(Change::Colors);
        self.render_map();
    }

    // ---- Forecast and maps ---------------------------------------------

    /// Everything from scratch: the band table and the map.
    pub fn reload(&self) {
        let generation = self.forecast_gen.get() + 1;
        self.forecast_gen.set(generation);
        {
            let mut v = self.view.borrow_mut();
            v.reach.clear();
            v.status = None;
            v.probe = None;
        }
        let core = self.core.clone();
        let now = self.view.borrow().now;
        let me = self.rc();
        let t0 = std::time::Instant::now();
        spawn(
            move || core.forecast(now),
            move |result| {
                debug("forecast", t0);
                if me.forecast_gen.get() != generation {
                    return;
                }
                {
                    let mut v = me.view.borrow_mut();
                    match result {
                        Ok(f) => {
                            v.forecast = Some(f);
                            v.forecast_error = None;
                        }
                        Err(e) => v.forecast_error = Some(e.to_string()),
                    }
                }
                me.emit(Change::Forecast);
            },
        );
        self.update_map();
    }

    pub fn select(&self, band: Band, hour: u32) {
        let band_changed = {
            let mut v = self.view.borrow_mut();
            let changed = v.band != band;
            v.band = band;
            v.hour = hour % 24;
            v.probe = None;
            changed
        };
        if band_changed {
            self.view.borrow_mut().saved.band = band;
            let _ = self.core.set_band(band);
        }
        self.emit(Change::Selection);
        self.update_map();
    }

    /// The coarse grid at once, then the fine one, then a patch if zoomed.
    pub fn update_map(&self) {
        let generation = self.map_gen.get() + 1;
        self.map_gen.set(generation);
        let (band, hour, now) = {
            let mut v = self.view.borrow_mut();
            v.map_working = true;
            v.map_error = None;
            (v.band, v.hour, v.now)
        };
        self.emit(Change::Map);
        self.render_map();
        let core = self.core.clone();
        let me = self.rc();
        let t0 = std::time::Instant::now();
        spawn(
            move || {
                let reach = core.coarse_maps(hour, now);
                (reach, core.has_fine_map(band, hour, now))
            },
            move |(result, held)| {
                debug("coarse maps", t0);
                if me.map_gen.get() != generation {
                    return;
                }
                match result {
                    Ok(reach) => me.view.borrow_mut().reach = reach,
                    Err(e) => me.view.borrow_mut().map_error = Some(e.to_string()),
                }
                me.emit(Change::Map);
                me.render_map();
                // A fine map not yet held waits until the reader stops on
                // an hour: sliding across the day then costs a coarse run
                // per hour, not a fine one each, and the last stop wins.
                let delay = if held { 0 } else { FINE_DELAY_MS };
                glib::timeout_add_local_once(std::time::Duration::from_millis(delay), move || {
                    me.fine_map(generation, band, hour, now, t0)
                });
            },
        );
    }

    /// The fine whole-world map, then a patch if zoomed in, unless a newer
    /// request has come in since.
    fn fine_map(&self, generation: u64, band: Band, hour: u32, now: i64, t0: std::time::Instant) {
        if self.map_gen.get() != generation {
            return;
        }
        let core = self.core.clone();
        let view = self.view.borrow().map_view;
        let me = self.rc();
        spawn(
            move || {
                let fine = core.fine_map(band, hour, now);
                if fine.is_ok() {
                    let _ = core.patch_map(hour, now, view);
                }
                fine
            },
            move |fine| {
                debug("fine map", t0);
                if me.map_gen.get() != generation {
                    return;
                }
                {
                    let mut v = me.view.borrow_mut();
                    v.map_working = false;
                    match fine {
                        Ok(status) => v.status = Some(status),
                        Err(e) => {
                            eprintln!("hfpredict: fine map: {e}");
                            v.map_error = Some(e.to_string());
                        }
                    }
                }
                me.emit(Change::Map);
                me.render_map();
                me.prefetch(generation);
            },
        );
    }

    /// Readies the rest of the day while the reader looks (see
    /// `Core::prefetch`), one map at a time, until anything new is asked.
    fn prefetch(&self, generation: u64) {
        if self.map_gen.get() != generation || self.view.borrow().map_working {
            return;
        }
        let (band, hour, now) = {
            let v = self.view.borrow();
            (v.band, v.hour, v.now)
        };
        let core = self.core.clone();
        let me = self.rc();
        spawn(
            move || core.prefetch(band, hour, now),
            move |r| {
                if let Ok(Some(_)) = r {
                    me.prefetch(generation);
                }
            },
        );
    }

    /// The map widget's size changed.
    pub fn set_map_size(&self, px: u32, density: f32) {
        if px != self.map_px.get() || density != self.density.get() {
            self.map_px.set(px);
            self.density.set(density);
            self.render_map();
        }
    }

    /// A pan or zoom: redraw at once, and work out a patch when the gesture
    /// settles.
    pub fn set_map_view(&self, view: MapView, settled: bool) {
        self.view.borrow_mut().map_view = view;
        if let Some(id) = self.settle.borrow_mut().take() {
            id.remove();
        }
        self.draft.set(!settled);
        self.render_map();
        if !settled {
            // Full size once the moves stop, even with no gesture end
            // (a scroll wheel has none).
            let me = Rc::downgrade(&self.rc());
            let id = glib::timeout_add_local_once(
                std::time::Duration::from_millis(SETTLE_MS),
                move || {
                    if let Some(me) = me.upgrade() {
                        me.settle.borrow_mut().take();
                        let v = me.view.borrow().map_view;
                        me.set_map_view(v, true);
                    }
                },
            );
            *self.settle.borrow_mut() = Some(id);
        }
        if settled {
            let (hour, now) = {
                let v = self.view.borrow();
                (v.hour, v.now)
            };
            let generation = self.map_gen.get();
            let core = self.core.clone();
            let me = self.rc();
            spawn(
                move || core.patch_map(hour, now, view),
                move |r| {
                    if me.map_gen.get() == generation && r.unwrap_or(false) {
                        me.render_map();
                        me.emit(Change::Map);
                    }
                },
            );
        }
    }

    /// Draws the map from what has been computed. One render at a time; a
    /// request during one is folded into a single follow-up.
    pub fn render_map(&self) {
        let (px, density) = if self.draft.get() {
            (self.map_px.get() / 2, self.density.get() / 2.0)
        } else {
            (self.map_px.get(), self.density.get())
        };
        if px < 16 {
            return;
        }
        if self.rendering.get() {
            self.render_again.set(true);
            return;
        }
        self.rendering.set(true);
        let (band, hour, now, view, palette, highlight) = {
            let v = self.view.borrow();
            (
                v.band,
                v.hour,
                v.now,
                v.map_view,
                v.colors.map,
                v.probe.clone(),
            )
        };
        let core = self.core.clone();
        let me = self.rc();
        let options = MapOptions {
            show_night: true,
            show_nvis: true,
            show_path: true,
            highlight,
        };
        spawn(
            move || core.render_map(band, hour, now, view, px, density, palette, options),
            move |image| {
                me.rendering.set(false);
                let changed = me.view.borrow().map_detail != image.detail;
                me.view.borrow_mut().map_detail = image.detail;
                if changed {
                    me.emit(Change::Map);
                }
                let size = image.size as i32;
                let bytes = glib::Bytes::from_owned(image.pixels);
                let texture = gtk::gdk::MemoryTexture::new(
                    size,
                    size,
                    gtk::gdk::MemoryFormat::R8g8b8a8Premultiplied,
                    &bytes,
                    (size * 4) as usize,
                );
                if let Some(sink) = me.texture_sink.borrow().as_ref() {
                    sink(texture, view);
                }
                if me.render_again.replace(false) {
                    me.render_map();
                }
            },
        );
    }

    pub fn fit_reach(&self) {
        let (band, hour, now) = {
            let v = self.view.borrow();
            (v.band, v.hour, v.now)
        };
        let view = self.core.fit_view(band, hour, now);
        self.set_map_view(view, true);
    }

    pub fn probe(&self, x: f64, y: f64, size: f64) {
        let (band, hour, now, view) = {
            let v = self.view.borrow();
            (v.band, v.hour, v.now, v.map_view)
        };
        let probe = self.core.probe(band, hour, now, view, x, y, size);
        // A second click on the same cell clears it.
        let same = {
            let v = self.view.borrow();
            matches!((&v.probe, &probe), (Some(a), Some(b)) if a.grid == b.grid)
        };
        self.view.borrow_mut().probe = if same { None } else { probe };
        self.emit(Change::Map);
        self.render_map();
    }

    // ---- Online readings -----------------------------------------------

    /// Fetches NOAA and GIRO when the network is allowed; a now-cast that
    /// starts or changes recomputes the forecast.
    pub fn refresh_readings(&self) {
        if !self.view.borrow().saved.settings.network {
            return;
        }
        let core = self.core.clone();
        let now = unix_now();
        let me = self.rc();
        spawn(
            move || {
                let before = core.is_nowcast(now);
                let sw = core.refresh_space_weather(now);
                let _ = core.refresh_sounding(now);
                (before, sw)
            },
            move |(before, sw)| {
                let changed = {
                    let mut v = me.view.borrow_mut();
                    let old = v
                        .saved
                        .space_weather
                        .as_ref()
                        .map(|s| (s.effective_ssn, s.kp_max_24h));
                    v.saved = me.core.state();
                    let new = v
                        .saved
                        .space_weather
                        .as_ref()
                        .map(|s| (s.effective_ssn, s.kp_max_24h));
                    sw.is_ok() && (old != new || !before)
                };
                me.emit(Change::Readings);
                if changed {
                    me.reload();
                }
            },
        );
    }

    /// Called every minute: rolls "now", and follows the clock if the
    /// reader was on the current hour.
    pub fn tick(&self) {
        let now = unix_now();
        let (rolled, follow) = {
            let mut v = self.view.borrow_mut();
            let old_hour = v.now_hour();
            let following = v.hour == old_hour;
            let old_day = v.now.div_euclid(86_400);
            v.now = now;
            (
                v.now_hour() != old_hour || now.div_euclid(86_400) != old_day,
                following,
            )
        };
        if rolled {
            if follow {
                let h = self.view.borrow().now_hour();
                self.view.borrow_mut().hour = h;
            }
            self.emit(Change::Clock);
            self.reload();
        }
    }
}
