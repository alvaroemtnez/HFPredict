//! The app's state and everything that changes it.
//!
//! One `App` per window. Core calls run on worker threads
//! (`gio::spawn_blocking`) and their answers come back to the main loop,
//! where a generation number throws away any answer a newer request has
//! overtaken (the reader moved the slider again before the map arrived).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::{gio, glib};
use hfpredict_core::Core;
use hfpredict_core::app::{BandReach, MapDetail, MapOptions, MapStatus, Probe};
use hfpredict_core::bands::Band;
use hfpredict_core::forecast::Forecast;
use hfpredict_core::projection::MapView;
use hfpredict_core::quality::Quality;
use hfpredict_core::store::{SavedState, Settings, Theme};
use hfpredict_core::tiles::{self, Screen, Tile};

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

/// A rendered tile: its texture, the fingerprint of what it shows (the
/// core's map signature with the app's colours and selection) and when it
/// was last drawn (for evicting the least used).
pub struct TileEntry {
    pub texture: gtk::gdk::Texture,
    pub signature: u64,
    pub used: Cell<u64>,
}

/// Hour changes closer than this count as sliding: coarser tiles first.
const SCRUB_MS: i64 = 350;

/// Tiles kept as textures, about 1 MB each.
const MAX_TILES: usize = 96;

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
    /// The map area in device pixels and their size in logical ones.
    screen: Cell<Option<(Screen, f32)>>,
    /// The map's tiles (see `hfpredict_core::tiles`), each the newest drawn.
    /// A stale tile (its signature not the current one) stays on screen
    /// until its replacement arrives.
    pub tiles: RefCell<HashMap<Tile, TileEntry>>,
    signature: Cell<u64>,
    /// The hour is sliding (changes closer than SCRUB_MS): coarser tiles.
    scrubbing: Cell<bool>,
    last_select: Cell<i64>,
    tile_busy: Cell<bool>,
    uses: Cell<u64>,
    /// Redraws the map widget.
    pub redraw: RefCell<Option<Box<dyn Fn()>>>,
    /// Settles a pan or zoom shortly after the last move (a scroll wheel
    /// has no gesture end).
    settle: RefCell<Option<glib::SourceId>>,
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
            screen: Cell::new(None),
            tiles: RefCell::new(HashMap::new()),
            signature: Cell::new(0),
            scrubbing: Cell::new(false),
            last_select: Cell::new(0),
            tile_busy: Cell::new(false),
            uses: Cell::new(0),
            redraw: RefCell::new(None),
            settle: RefCell::new(None),
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
        self.map_changed();
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
        }
        // A selected point stays; its chance is worked out again.
        self.refresh_probe();
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
        // Hours changing quickly (the slider, a drag along the grid): the
        // map shows coarser tiles until it stops.
        let t = glib::monotonic_time() / 1000;
        if t - self.last_select.get() < SCRUB_MS {
            self.scrubbing.set(true);
        }
        self.last_select.set(t);
        let me = Rc::downgrade(&self.rc());
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(SCRUB_MS as u64),
            move || {
                if let Some(me) = me.upgrade()
                    && me.scrubbing.get()
                    && glib::monotonic_time() / 1000 - me.last_select.get() >= SCRUB_MS
                {
                    me.scrubbing.set(false);
                    me.pump_tiles();
                }
            },
        );
        let band_changed = {
            let mut v = self.view.borrow_mut();
            let changed = v.band != band;
            v.band = band;
            v.hour = hour % 24;
            changed
        };
        if band_changed {
            self.view.borrow_mut().saved.band = band;
            let _ = self.core.set_band(band);
        }
        self.emit(Change::Selection);
        self.update_map();
        self.refresh_probe();
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
        self.map_changed();
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
                me.map_changed();
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
                me.map_changed();
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

    /// The map area changed: `screen` in device pixels, `scale` of them
    /// per logical pixel.
    pub fn set_map_screen(&self, screen: Screen, scale: f32) {
        let old = self.screen.replace(Some((screen, scale)));
        if old.map(|(_, k)| k) != Some(scale) {
            // Line widths follow the scale: every tile is redrawn.
            self.map_changed();
        } else {
            self.pump_tiles();
        }
        self.request_redraw();
    }

    pub fn screen(&self) -> Option<(Screen, f32)> {
        self.screen.get()
    }

    /// A pan or zoom: the held tiles move at once, missing ones follow, and
    /// a patch is worked out once the view settles.
    pub fn set_map_view(&self, view: MapView, settled: bool) {
        self.view.borrow_mut().map_view = view;
        if let Some(id) = self.settle.borrow_mut().take() {
            id.remove();
        }
        self.request_redraw();
        self.pump_tiles();
        if !settled {
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
            return;
        }
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
                // A patch for this view (new or held) changes what its tiles show.
                if me.map_gen.get() == generation && r.unwrap_or(false) {
                    me.map_changed();
                    me.emit(Change::Map);
                }
            },
        );
    }

    fn request_redraw(&self) {
        if let Some(f) = self.redraw.borrow().as_ref() {
            f();
        }
    }

    /// What the map shows may have changed (data, band, hour, colours, the
    /// selection): the tiles on screen are checked against the content's
    /// fingerprint and only those that differ are drawn again.
    pub fn map_changed(&self) {
        self.pump_tiles();
    }

    /// The fingerprint of the newest content; tiles with another are stale.
    pub fn signature(&self) -> u64 {
        self.signature.get()
    }

    /// Draws the next tile the screen lacks, one at a time: the one in the
    /// middle first; the whole-earth tile first when none is held at all,
    /// so zooming out never shows empty space. While the hour slides, a
    /// level coarser (a quarter of the pixels), sharpened once it stops.
    fn pump_tiles(&self) {
        if self.tile_busy.get() {
            return;
        }
        let Some((screen, scale)) = self.screen.get() else {
            return;
        };
        if screen.side < 16.0 {
            return;
        }
        let (band, hour, now, view, palette, options) = {
            let v = self.view.borrow();
            let q = v
                .probe
                .as_ref()
                .and_then(|p| p.chance.map(Quality::of).or(p.quality));
            let mut palette = v.colors.map;
            // The path in the traffic-light colour of the chance to its end.
            if let Some(cell) = v
                .forecast
                .as_ref()
                .filter(|f| f.to.is_some())
                .and_then(|f| f.cell(v.band, v.hour))
            {
                palette.path = hfpredict_core::render::semaphore(
                    Some(Quality::of(cell.reliability)),
                    v.colors.dark,
                    v.colors.low_light,
                );
            }
            (
                v.band,
                v.hour,
                v.now,
                v.map_view,
                palette,
                MapOptions {
                    show_night: true,
                    show_nvis: true,
                    show_path: true,
                    highlight: v.probe.clone(),
                    highlight_color: hfpredict_core::render::semaphore(
                        q,
                        v.colors.dark,
                        v.colors.low_light,
                    ),
                },
            )
        };
        // The app's part of a tile's look.
        let app_key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            format!("{palette:?}{options:?}{scale}").hash(&mut h);
            h.finish()
        };
        let full = tiles::level_for(view, &screen);
        let z = if self.scrubbing.get() {
            full.saturating_sub(1)
        } else {
            full
        };
        self.tile_busy.set(true);
        let core = self.core.clone();
        let me = self.rc();
        spawn(
            move || core.map_signature(band, hour, now, view),
            move |content| {
                let signature = content.wrapping_mul(31) ^ app_key;
                me.signature.set(signature);
                let next = {
                    let held = me.tiles.borrow();
                    let fresh = |t: &Tile| held.get(t).is_some_and(|e| e.signature == signature);
                    let mut wanted = vec![];
                    if !held.contains_key(&Tile::WORLD) {
                        wanted.push(Tile::WORLD);
                    }
                    wanted.extend(tiles::visible(view, &screen, z).into_iter().map(|p| p.tile));
                    wanted.push(Tile::WORLD);
                    wanted.into_iter().find(|t| !fresh(t))
                };
                let Some(tile) = next else {
                    me.tile_busy.set(false);
                    me.request_redraw();
                    return;
                };
                let core = me.core.clone();
                let me2 = me.clone();
                // Lines thicker by as much as the tile will be shrunk.
                let lines = scale * tiles::line_scale(view, &screen, tile.z) as f32;
                spawn(
                    move || core.render_tile(band, hour, now, view, tile, lines, palette, options),
                    move |image| {
                        let me = me2;
                        me.tile_busy.set(false);
                        let changed = me.view.borrow().map_detail != image.detail;
                        me.view.borrow_mut().map_detail = image.detail;
                        if changed {
                            me.emit(Change::Map);
                        }
                        let size = image.size as i32;
                        let texture = gtk::gdk::MemoryTexture::new(
                            size,
                            size,
                            gtk::gdk::MemoryFormat::R8g8b8a8Premultiplied,
                            &glib::Bytes::from_owned(image.pixels),
                            (size * 4) as usize,
                        );
                        me.keep_tile(tile, gtk::prelude::Cast::upcast(texture), signature);
                        me.request_redraw();
                        me.pump_tiles();
                    },
                );
            },
        );
    }

    fn keep_tile(&self, tile: Tile, texture: gtk::gdk::Texture, signature: u64) {
        let mut held = self.tiles.borrow_mut();
        held.insert(
            tile,
            TileEntry {
                texture,
                signature,
                used: Cell::new(self.next_use()),
            },
        );
        while held.len() > MAX_TILES {
            let oldest = held
                .iter()
                .filter(|(t, _)| **t != Tile::WORLD)
                .min_by_key(|(_, e)| e.used.get())
                .map(|(t, _)| *t);
            match oldest {
                Some(t) => held.remove(&t),
                None => break,
            };
        }
    }

    /// A tick of the use clock, stamped on every tile drawn.
    pub fn next_use(&self) -> u64 {
        self.uses.set(self.uses.get() + 1);
        self.uses.get()
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
        self.map_changed();
        self.refresh_probe();
    }

    /// Works out the selected point's own chance of contact (a path
    /// forecast to it, kept by the core), for the band and hour on screen.
    pub fn refresh_probe(&self) {
        let Some((lat, lon, grid)) = self
            .view
            .borrow()
            .probe
            .as_ref()
            .map(|p| (p.lat, p.lon, p.grid.clone()))
        else {
            return;
        };
        let (band, hour, now) = {
            let v = self.view.borrow();
            (v.band, v.hour, v.now)
        };
        let core = self.core.clone();
        let me = self.rc();
        spawn(
            move || core.probe_at(band, hour, now, lat, lon),
            move |full| {
                let current = {
                    let v = me.view.borrow();
                    v.probe.as_ref().is_some_and(|p| p.grid == grid)
                        && v.band == band
                        && v.hour == hour
                };
                if current {
                    me.view.borrow_mut().probe = Some(full);
                    me.emit(Change::Map);
                    me.map_changed();
                }
            },
        );
    }

    /// Makes the selected point the path's destination.
    pub fn probe_to_destination(&self) {
        let Some((lat, lon)) = self.view.borrow().probe.as_ref().map(|p| (p.lat, p.lon)) else {
            return;
        };
        let place = self.core.place_at(lat, lon);
        self.view.borrow_mut().probe = None;
        self.save(|c| c.set_to(Some(place)));
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
