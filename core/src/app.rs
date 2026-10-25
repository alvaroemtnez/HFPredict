//! The one object both apps talk to.
//!
//! Every method is blocking and thread-safe: the UI calls it from a worker
//! thread (a coroutine on Android, `gio::spawn_blocking` on GNOME) and never
//! from the thread that draws. Heavy work runs outside the locks, so a map
//! being computed never holds up a settings change.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::Error;
use crate::bands::Band;
use crate::cache::{DiskCache, Lru};
use crate::correct::CentreField;
use crate::coverage::{self, Grid};
use crate::engine::{Engine, RunContext};
use crate::forecast::{self, Forecast};
use crate::geo::{self, LatLon, UtcDate};
use crate::maps;
use crate::online::{self, Sounding, SpaceWeather};
use crate::places::{self, Parsed, Place};
use crate::projection::{MapView, Projection};
use crate::quality::Quality;
use crate::render::{self, Palette, Scene};
use crate::station::Station;
use crate::store::{SavedState, Settings, Store};
use crate::tiles::{TILE_PX, Tile};

/// Room allowed for maps on disk.
pub const MAP_BUDGET_BYTES: u64 = 128 * 1024 * 1024;

/// How finely the map on screen has been worked out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum MapDetail {
    /// Nothing computed yet for this band and hour.
    None,
    /// The 15 by 22.5 degree grid.
    Coarse,
    /// The 1.25 by 1.5 degree grid, whole world.
    Fine,
    /// The fine grid plus a finer patch around the zoomed view.
    Patch,
}

/// One band's share of the world at an hour.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct BandReach {
    pub band: Band,
    pub reach: f64,
}

/// What the map shows, in numbers for the lines under it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct MapStatus {
    pub detail: MapDetail,
    /// Area-weighted share of the world reached, from the finest whole-world
    /// grid held.
    pub reach: Option<f64>,
    /// How far the band reaches with no skip zone, km.
    pub nvis_km: Option<f64>,
}

/// A rendered map: `size` by `size` premultiplied RGBA.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct MapImage {
    pub size: u32,
    pub pixels: Vec<u8>,
    pub detail: MapDetail,
}

/// Toggles for the map layers.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct MapOptions {
    pub show_night: bool,
    pub show_nvis: bool,
    pub show_path: bool,
    /// The probed point, whose cell is outlined.
    pub highlight: Option<Probe>,
    /// The outline's colour, 0xAARRGGBB (`semaphore_color` of its chance).
    pub highlight_color: u32,
}

type BandGrids = Vec<(Band, Grid)>;
type Centres = Vec<(Band, Option<CentreField>)>;

struct Caches {
    coarse: Lru<BandGrids>,
    coarse_centres: Lru<Centres>,
    fine_centres: Lru<Option<CentreField>>,
    fine: Lru<Grid>,
    patch: Lru<BandGrids>,
    /// Whole-day path forecasts to probed points.
    points: Lru<Forecast>,
}

#[cfg_attr(feature = "ffi", derive(uniffi::Object))]
pub struct Core {
    engine: Engine,
    store: Store,
    state: Mutex<SavedState>,
    caches: Mutex<Caches>,
    disk: DiskCache,
    threads: usize,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Core {
    /// `data_dir` keeps the saved state; `cache_dir` is for files the system
    /// may delete (antenna definitions, stored maps).
    pub fn open(data_dir: PathBuf, cache_dir: PathBuf) -> Core {
        let store = Store::new(&data_dir);
        let state = store.load();
        Core {
            engine: Engine::new(cache_dir.join("engine")),
            store,
            state: Mutex::new(state),
            caches: Mutex::new(Caches {
                coarse: Lru::new(48),
                coarse_centres: Lru::new(4),
                fine_centres: Lru::new(12),
                fine: Lru::new(36),
                patch: Lru::new(8),
                points: Lru::new(8),
            }),
            disk: DiskCache::new(cache_dir.join("maps"), MAP_BUDGET_BYTES),
            threads: crate::parallel::cores(),
        }
    }

    fn update(&self, change: impl FnOnce(&mut SavedState)) -> Result<SavedState, Error> {
        let mut state = lock(&self.state);
        change(&mut state);
        let snapshot = state.clone();
        drop(state);
        self.store.save(&snapshot)?;
        Ok(snapshot)
    }

    /// The run context for today (UTC) and where the path starts.
    fn context(&self, now: i64) -> (RunContext, Place, Option<Place>) {
        let state = lock(&self.state);
        let date = UtcDate::of(now);
        let nowcast = state
            .space_weather
            .as_ref()
            .filter(|sw| sw.is_fresh(now))
            .map(|sw| crate::engine::Nowcast {
                effective_ssn: sw.effective_ssn,
                kp_max_24h: sw.kp_max_24h,
            });
        let ctx = RunContext {
            station: state.station(),
            model: state.settings.model,
            date,
            nowcast,
        };
        (
            ctx,
            state.from.clone().unwrap_or_else(Place::greenwich),
            state.to.clone(),
        )
    }

    fn key(kind: &str, ctx: &RunContext, from: &Place, rest: &str) -> String {
        format!(
            "{kind}|{:.4},{:.4}|{}|{rest}",
            from.lat,
            from.lon,
            ctx.signature()
        )
    }

    fn coarse_centres(&self, ctx: &RunContext, from: &Place) -> Result<Arc<Centres>, Error> {
        let key = Self::key("cc", ctx, from, "");
        if let Some(c) = lock(&self.caches).coarse_centres.get(&key) {
            return Ok(c);
        }
        let c = Arc::new(maps::centres(
            &self.engine,
            ctx,
            from.lat_lon(),
            coverage::CENTRE_LAT_STEP,
            coverage::CENTRE_LON_STEP,
            None,
            self.threads,
        )?);
        lock(&self.caches).coarse_centres.put(key, c.clone());
        Ok(c)
    }

    fn fine_centre(
        &self,
        ctx: &RunContext,
        from: &Place,
        band: Band,
        threads: usize,
    ) -> Result<Arc<Option<CentreField>>, Error> {
        let key = Self::key("fc", ctx, from, band.label());
        if let Some(c) = lock(&self.caches).fine_centres.get(&key) {
            return Ok(c);
        }
        let mut fields = maps::centres(
            &self.engine,
            ctx,
            from.lat_lon(),
            coverage::FINE_CENTRE_LAT_STEP,
            coverage::FINE_CENTRE_LON_STEP,
            Some(band),
            threads,
        )?;
        let c = Arc::new(fields.pop().and_then(|f| f.1));
        lock(&self.caches).fine_centres.put(key, c.clone());
        Ok(c)
    }

    fn coarse_grids(
        &self,
        ctx: &RunContext,
        from: &Place,
        hour: u32,
    ) -> Result<Arc<BandGrids>, Error> {
        let key = Self::key("coarse", ctx, from, &hour.to_string());
        if let Some(g) = lock(&self.caches).coarse.get(&key) {
            return Ok(g);
        }
        let raw = maps::coarse(&self.engine, ctx, from.lat_lon(), hour)?;
        let centres = self.coarse_centres(ctx, from)?;
        let required = f64::from(ctx.station.mode.required_snr_db());
        let kp = ctx.nowcast.map(|n| n.kp_max_24h);
        let grids: BandGrids = raw
            .iter()
            .map(|(band, r)| {
                let field = centres
                    .iter()
                    .find(|(b, _)| b == band)
                    .and_then(|(_, f)| f.as_ref());
                (*band, r.corrected(field, required, kp))
            })
            .collect();
        let grids = Arc::new(grids);
        lock(&self.caches).coarse.put(key, grids.clone());
        Ok(grids)
    }

    fn cached_fine(
        &self,
        ctx: &RunContext,
        from: &Place,
        band: Band,
        hour: u32,
    ) -> Option<Arc<Grid>> {
        let key = Self::key("fine", ctx, from, &format!("{}|{hour}", band.label()));
        if let Some(g) = lock(&self.caches).fine.get(&key) {
            return Some(g);
        }
        let keep = lock(&self.state).settings.keep_maps;
        let g = Arc::new(if keep {
            self.disk.get(&key)?
        } else {
            return None;
        });
        lock(&self.caches).fine.put(key, g.clone());
        Some(g)
    }

    fn fine_grid(
        &self,
        ctx: &RunContext,
        from: &Place,
        band: Band,
        hour: u32,
        threads: usize,
    ) -> Result<Arc<Grid>, Error> {
        if let Some(g) = self.cached_fine(ctx, from, band, hour) {
            return Ok(g);
        }
        let raw = maps::fine(&self.engine, ctx, from.lat_lon(), hour, band, threads)?;
        let centre = self.fine_centre(ctx, from, band, threads)?;
        let grid = Arc::new(raw.corrected(
            centre.as_ref().as_ref(),
            f64::from(ctx.station.mode.required_snr_db()),
            ctx.nowcast.map(|n| n.kp_max_24h),
        ));
        let key = Self::key("fine", ctx, from, &format!("{}|{hour}", band.label()));
        if lock(&self.state).settings.keep_maps {
            self.disk.put(&key, &grid);
        }
        lock(&self.caches).fine.put(key, grid.clone());
        Ok(grid)
    }

    fn patch_key(
        ctx: &RunContext,
        from: &Place,
        hour: u32,
        view: MapView,
    ) -> Option<(String, coverage::PatchGrid)> {
        let (centre, half_lat) = view.region(&Projection::new(from.lat_lon()))?;
        let grid = coverage::patch_grid(centre.lat, centre.lon, half_lat)?;
        // Only worth a run when finer than the fine grid already drawn.
        if grid.lat_step >= coverage::FINE_LAT_STEP {
            return None;
        }
        Some((
            Self::key("patch", ctx, from, &format!("{hour}|{}", grid.key())),
            grid,
        ))
    }

    fn layers(
        &self,
        ctx: &RunContext,
        from: &Place,
        band: Band,
        hour: u32,
        view: MapView,
    ) -> (Vec<Arc<Grid>>, MapDetail) {
        let mut layers = vec![];
        let mut detail = MapDetail::None;
        let coarse_key = Self::key("coarse", ctx, from, &hour.to_string());
        if let Some(c) = lock(&self.caches).coarse.get(&coarse_key)
            && let Some((_, g)) = c.iter().find(|(b, _)| *b == band)
        {
            layers.push(Arc::new(g.clone()));
            detail = MapDetail::Coarse;
        }
        if let Some(g) = self.cached_fine(ctx, from, band, hour) {
            layers.push(g);
            detail = MapDetail::Fine;
        }
        if let Some((key, _)) = Self::patch_key(ctx, from, hour, view)
            && let Some(p) = lock(&self.caches).patch.get(&key)
            && let Some((_, g)) = p.iter().find(|(b, _)| *b == band)
        {
            layers.push(Arc::new(g.clone()));
            detail = MapDetail::Patch;
        }
        (layers, detail)
    }
    /// The map for `frame` (a whole view or one tile), with the layers the
    /// screen's `view` calls for.
    #[allow(clippy::too_many_arguments)]
    fn render_view(
        &self,
        band: Band,
        hour: u32,
        now: i64,
        view: MapView,
        frame: MapView,
        size: u32,
        density: f32,
        palette: Palette,
        options: MapOptions,
    ) -> MapImage {
        let (ctx, from, to) = self.context(now);
        let (layers, detail) = self.layers(&ctx, &from, band, hour, view);
        let date = UtcDate::of(now);
        let at = geo::days_from_civil(date.year, date.month, date.day) * 86_400
            + i64::from(hour) * 3600
            + 1800;
        let scene = Scene {
            centre: from.lat_lon(),
            destination: if options.show_path {
                to.map(|t| t.lat_lon())
            } else {
                None
            },
            unix_secs: at,
            layers: layers.iter().map(|g| g.as_ref()).collect(),
            show_night: options.show_night,
            show_nvis: options.show_nvis,
            highlight: options.highlight.map(|p| LatLon {
                lat: p.lat,
                lon: p.lon,
            }),
            highlight_color: options.highlight_color,
        };
        MapImage {
            size,
            pixels: render::render(&scene, frame, size, density, &palette, self.threads),
            detail,
        }
    }
}

#[cfg_attr(feature = "ffi", uniffi::export)]
impl Core {
    #[cfg_attr(feature = "ffi", uniffi::constructor)]
    pub fn new(data_dir: String, cache_dir: String) -> Arc<Core> {
        Arc::new(Core::open(
            PathBuf::from(data_dir),
            PathBuf::from(cache_dir),
        ))
    }

    // ---- Saved state -------------------------------------------------

    pub fn state(&self) -> SavedState {
        lock(&self.state).clone()
    }

    pub fn set_settings(&self, settings: Settings) -> Result<SavedState, Error> {
        if !settings.keep_maps {
            self.disk.clear();
        }
        self.update(|s| s.settings = settings)
    }

    /// Saves a station (new or changed) and makes it the active one.
    pub fn put_station(&self, station: Station) -> Result<SavedState, Error> {
        self.update(|s| s.put_station(station))
    }

    /// A copy of the active station under a new id, not yet saved.
    pub fn new_station(&self) -> Station {
        lock(&self.state).new_station()
    }

    pub fn select_station(&self, id: String) -> Result<SavedState, Error> {
        self.update(|s| {
            if s.stations.iter().any(|st| st.id == id) {
                s.active_station = id;
            }
        })
    }

    pub fn delete_station(&self, id: String) -> Result<SavedState, Error> {
        self.update(|s| s.delete_station(&id))
    }

    pub fn set_from(&self, place: Place) -> Result<SavedState, Error> {
        self.update(|s| {
            s.remember(&place);
            s.from = Some(place);
        })
    }

    /// `None` forecasts every direction.
    pub fn set_to(&self, place: Option<Place>) -> Result<SavedState, Error> {
        self.update(|s| {
            if let Some(p) = &place {
                s.remember(p);
            }
            s.to = place;
        })
    }

    pub fn swap_ends(&self) -> Result<SavedState, Error> {
        self.update(|s| {
            if let (Some(from), Some(to)) = (s.from.take(), s.to.take()) {
                s.from = Some(to);
                s.to = Some(from);
            }
        })
    }

    pub fn set_band(&self, band: Band) -> Result<SavedState, Error> {
        self.update(|s| s.band = band)
    }

    // ---- Forecast ----------------------------------------------------

    /// Today's band table: to the destination, or a survey of every
    /// direction when there is none.
    pub fn forecast(&self, now: i64) -> Result<Forecast, Error> {
        let (ctx, from, to) = self.context(now);
        match to {
            Some(to) => forecast::path(&self.engine, &ctx, &from, &to),
            None => forecast::survey(&self.engine, &ctx, &from, self.threads),
        }
    }

    // ---- Maps --------------------------------------------------------

    /// Every band's reach at an hour, from the coarse grid. Fast: run it
    /// first, so the map has something to show at once.
    pub fn coarse_maps(&self, hour: u32, now: i64) -> Result<Vec<BandReach>, Error> {
        let (ctx, from, _) = self.context(now);
        let grids = self.coarse_grids(&ctx, &from, hour)?;
        Ok(grids
            .iter()
            .map(|(band, g)| BandReach {
                band: *band,
                reach: g.reach(),
            })
            .collect())
    }

    /// Works out the fine whole-world map for a band and hour (or loads it).
    pub fn fine_map(&self, band: Band, hour: u32, now: i64) -> Result<MapStatus, Error> {
        let (ctx, from, _) = self.context(now);
        self.coarse_grids(&ctx, &from, hour)?;
        let grid = self.fine_grid(&ctx, &from, band, hour, self.threads)?;
        Ok(MapStatus {
            detail: MapDetail::Fine,
            reach: Some(grid.reach()),
            nvis_km: render::nvis_reach_km(from.lat_lon(), &grid),
        })
    }

    /// Works out one map the reader is likely to look at next, so that
    /// sliding through the day finds them ready: first every hour's coarse
    /// grid (every band at once, quick), then `band`'s fine map at the hours
    /// nearest `hour`. On half the cores, so the reader's own requests keep
    /// most of the machine. Returns the hour worked on, or `None` when the
    /// whole day is ready. Apps call it again and again while the reader is
    /// idle and the app is on screen, and stop on any new request.
    pub fn prefetch(&self, band: Band, hour: u32, now: i64) -> Result<Option<u32>, Error> {
        let (ctx, from, _) = self.context(now);
        let threads = (self.threads / 2).max(1);
        let order: Vec<u32> = (0..24u32)
            .flat_map(|k| [(hour + k) % 24, (hour + 24 - k) % 24])
            .fold(vec![], |mut seen, h| {
                if !seen.contains(&h) {
                    seen.push(h);
                }
                seen
            });
        for &h in &order {
            let key = Self::key("coarse", &ctx, &from, &h.to_string());
            if lock(&self.caches).coarse.get(&key).is_none() {
                self.coarse_grids(&ctx, &from, h)?;
                return Ok(Some(h));
            }
        }
        for &h in &order {
            if self.cached_fine(&ctx, &from, band, h).is_none() {
                self.fine_grid(&ctx, &from, band, h, threads)?;
                return Ok(Some(h));
            }
        }
        Ok(None)
    }

    /// Whether a fine map is already held, without computing anything.
    pub fn has_fine_map(&self, band: Band, hour: u32, now: i64) -> bool {
        let (ctx, from, _) = self.context(now);
        self.cached_fine(&ctx, &from, band, hour).is_some()
    }

    /// Works out a finer patch for a zoomed-in view. False when the view is
    /// not zoomed far enough to need one (or sits on the antimeridian).
    pub fn patch_map(&self, hour: u32, now: i64, view: MapView) -> Result<bool, Error> {
        let (ctx, from, _) = self.context(now);
        let Some((key, grid)) = Self::patch_key(&ctx, &from, hour, view) else {
            return Ok(false);
        };
        if lock(&self.caches).patch.get(&key).is_some() {
            return Ok(true);
        }
        let raw = maps::patch(&self.engine, &ctx, from.lat_lon(), hour, &grid)?;
        let centres = self.coarse_centres(&ctx, &from)?;
        let required = f64::from(ctx.station.mode.required_snr_db());
        let kp = ctx.nowcast.map(|n| n.kp_max_24h);
        let grids: BandGrids = raw
            .iter()
            .map(|(band, r)| {
                let fine_field = lock(&self.caches).fine_centres.get(&Self::key(
                    "fc",
                    &ctx,
                    &from,
                    band.label(),
                ));
                let field = fine_field
                    .as_ref()
                    .and_then(|f| f.as_ref().as_ref())
                    .or_else(|| {
                        centres
                            .iter()
                            .find(|(b, _)| b == band)
                            .and_then(|(_, f)| f.as_ref())
                    });
                (*band, r.corrected(field, required, kp))
            })
            .collect();
        lock(&self.caches).patch.put(key, Arc::new(grids));
        Ok(true)
    }

    /// Draws the map from whatever has been computed, never computing.
    #[allow(clippy::too_many_arguments)]
    pub fn render_map(
        &self,
        band: Band,
        hour: u32,
        now: i64,
        view: MapView,
        size: u32,
        density: f32,
        palette: Palette,
        options: MapOptions,
    ) -> MapImage {
        self.render_view(band, hour, now, view, view, size, density, palette, options)
    }

    /// Draws one tile of the map (see `tiles`). `view` is the view on
    /// screen, which decides the zoom patch drawn; the tile is drawn at
    /// `TILE_PX` whatever the view.
    #[allow(clippy::too_many_arguments)]
    pub fn render_tile(
        &self,
        band: Band,
        hour: u32,
        now: i64,
        view: MapView,
        tile: Tile,
        density: f32,
        palette: Palette,
        options: MapOptions,
    ) -> MapImage {
        self.render_view(
            band,
            hour,
            now,
            view,
            tile.view(),
            TILE_PX,
            density,
            palette,
            options,
        )
    }

    /// Frames where the band reaches, from the finest grid held.
    pub fn fit_view(&self, band: Band, hour: u32, now: i64) -> MapView {
        let (ctx, from, _) = self.context(now);
        let (layers, _) = self.layers(&ctx, &from, band, hour, MapView::default());
        layers
            .last()
            .map_or_else(MapView::default, |g| render::fit_reach(from.lat_lon(), g))
    }

    /// The place under a point of the map, and the band's quality there.
    #[allow(clippy::too_many_arguments)]
    pub fn probe(
        &self,
        band: Band,
        hour: u32,
        now: i64,
        view: MapView,
        x: f64,
        y: f64,
        size: f64,
    ) -> Option<Probe> {
        let (ctx, from, _) = self.context(now);
        let projection = Projection::new(from.lat_lon());
        let (dx, dy) = view.to_disc(x, y, size);
        let point = projection.invert(dx, dy)?;
        let (layers, _) = self.layers(&ctx, &from, band, hour, view);
        let reliability = layers
            .iter()
            .rev()
            .find_map(|g| g.reliability_at(point.lat, point.lon).map(f64::from));
        Some(Probe {
            lat: point.lat,
            lon: point.lon,
            grid: geo::lat_lon_to_grid(point.lat, point.lon),
            distance_km: geo::distance_km(from.lat_lon(), point),
            bearing_deg: geo::bearing_deg(from.lat_lon(), point),
            quality: reliability.map(Quality::of),
            chance: None,
        })
    }

    /// A point on the map in full: the map's quality there and the chance
    /// of contact from a path forecast to it. The forecast covers every band
    /// and hour and is kept, so moving through the day or the bands with a
    /// point selected costs nothing more. Call it off the UI thread.
    pub fn probe_at(&self, band: Band, hour: u32, now: i64, lat: f64, lon: f64) -> Probe {
        let (ctx, from, _) = self.context(now);
        let point = LatLon { lat, lon };
        let (layers, _) = self.layers(&ctx, &from, band, hour, MapView::default());
        let reliability = layers
            .iter()
            .rev()
            .find_map(|g| g.reliability_at(lat, lon).map(f64::from));
        let key = Self::key("point", &ctx, &from, &format!("{lat:.4},{lon:.4}"));
        let cached = lock(&self.caches).points.get(&key);
        let forecast = cached.or_else(|| {
            let to = Place::at(geo::lat_lon_to_grid(lat, lon), "", lat, lon);
            let f = Arc::new(forecast::path(&self.engine, &ctx, &from, &to).ok()?);
            lock(&self.caches).points.put(key, f.clone());
            Some(f)
        });
        Probe {
            lat,
            lon,
            grid: geo::lat_lon_to_grid(lat, lon),
            distance_km: geo::distance_km(from.lat_lon(), point),
            bearing_deg: geo::bearing_deg(from.lat_lon(), point),
            quality: reliability.map(Quality::of),
            chance: forecast.and_then(|f| f.cell(band, hour).map(|c| c.reliability)),
        }
    }

    /// Room the stored maps take, bytes.
    pub fn stored_maps_bytes(&self) -> u64 {
        self.disk.size()
    }

    pub fn clear_stored_maps(&self) {
        self.disk.clear();
        let mut c = lock(&self.caches);
        c.fine.clear();
    }

    // ---- Online readings ---------------------------------------------

    /// Fetches NOAA's readings and keeps them. Without network permission
    /// in Preferences this refuses rather than reaching out.
    pub fn refresh_space_weather(&self, now: i64) -> Result<SpaceWeather, Error> {
        if !lock(&self.state).settings.network {
            return Err(Error::Offline);
        }
        let sw = online::fetch_space_weather(now)?;
        self.update(|s| s.space_weather = Some(sw.clone()))?;
        Ok(sw)
    }

    /// The nearest ionosonde's latest reading, `None` when none is close.
    pub fn refresh_sounding(&self, now: i64) -> Result<Option<Sounding>, Error> {
        let (network, from) = {
            let s = lock(&self.state);
            (
                s.settings.network,
                s.from.clone().unwrap_or_else(Place::greenwich),
            )
        };
        if !network {
            return Err(Error::Offline);
        }
        let sounding = online::fetch_sounding(from.lat_lon(), now)?;
        self.update(|s| s.sounding = sounding.clone())?;
        Ok(sounding)
    }

    /// Whether the forecast is a now-cast at this moment.
    pub fn is_nowcast(&self, now: i64) -> bool {
        self.context(now).0.nowcast.is_some()
    }

    // ---- Places --------------------------------------------------------

    /// A grid square or coordinates typed in, worked out on the device.
    pub fn parse_place(&self, query: String) -> Option<Place> {
        match places::parse_query(&query) {
            Parsed::Grid(p) | Parsed::Coordinates(p) => Some(p),
            Parsed::Text => None,
        }
    }

    /// The built-in list first; online only when that finds little, the
    /// network is allowed and `online` asks for it.
    pub fn search_places(
        &self,
        query: String,
        language: String,
        online: bool,
    ) -> Result<Vec<Place>, Error> {
        if let Some(p) = self.parse_place(query.clone()) {
            return Ok(vec![p]);
        }
        let mut found = places::search_cities(&query);
        if online
            && found.len() < 3
            && query.trim().chars().count() >= 3
            && lock(&self.state).settings.network
        {
            let remote = online::fetch_places(query.trim(), &language)?;
            for p in remote {
                if !found
                    .iter()
                    .any(|f| geo::distance_km(f.lat_lon(), p.lat_lon()) < 5.0)
                {
                    found.push(p);
                }
            }
        }
        Ok(found)
    }

    /// A GPS fix as a place: named after a built-in city within 25 km,
    /// otherwise by its coordinates.
    pub fn place_at(&self, lat: f64, lon: f64) -> Place {
        let p = LatLon { lat, lon };
        match places::nearest_city(p, 25.0) {
            Some(city) => Place {
                lat,
                lon,
                grid: geo::lat_lon_to_grid(lat, lon),
                ..city
            },
            None => Place::at(places::format_lat_lon(p), "", lat, lon),
        }
    }
}

/// What is under a finger on the map.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Probe {
    pub lat: f64,
    pub lon: f64,
    pub grid: String,
    pub distance_km: f64,
    pub bearing_deg: f64,
    /// The map's quality band under the point.
    pub quality: Option<Quality>,
    /// The chance of contact there from a path forecast to that very point
    /// (`Core::probe_at`), 0..1; `None` until it is worked out.
    pub chance: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(name: &str) -> Core {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        Core::open(dir.join("data"), dir.join("cache"))
    }

    const NOW: i64 = 20_751 * 86_400 + 12 * 3600; // 2026-10-25 12:00 UTC

    #[test]
    fn whole_flow() {
        let c = core("hfp-app-flow");
        c.set_from(Place::at("Valencia", "Spain", 39.47, -0.38))
            .unwrap();
        let survey = c.forecast(NOW).unwrap();
        assert!(survey.to.is_none());
        c.set_to(Some(Place::at("Tokyo", "Japan", 35.7, 139.7)))
            .unwrap();
        let path = c.forecast(NOW).unwrap();
        assert_eq!(path.hops, Some(4));

        let reach = c.coarse_maps(12, NOW).unwrap();
        assert_eq!(reach.len(), 10);
        assert!(!c.has_fine_map(Band::M20, 12, NOW));
        let status = c.fine_map(Band::M20, 12, NOW).unwrap();
        assert!(status.reach.unwrap() > 0.0);
        assert!(c.has_fine_map(Band::M20, 12, NOW));
        assert!(c.stored_maps_bytes() > 60_000);

        let palette = Palette {
            disc: 0xFF000000,
            reliable: 0xFFFFFFFF,
            patchy: 0xFFAAAAAA,
            weak: 0xFF555555,
            night: 0x55000000,
            coastline: 0xFFFFFFFF,
            grid_lines: 0x55FFFFFF,
            path: 0xFF00FFFF,
            path_halo: 0xFF000000,
            station: 0xFFFFFF00,
            halo: 0xFF000000,
            nvis: 0xFFFFFFFF,
        };
        let opts = MapOptions {
            show_night: true,
            show_nvis: true,
            show_path: true,
            highlight: None,
            highlight_color: 0xFFFFFF00,
        };
        let img = c.render_map(
            Band::M20,
            12,
            NOW,
            MapView::default(),
            64,
            1.0,
            palette,
            opts.clone(),
        );
        assert_eq!(img.detail, MapDetail::Fine);
        assert_eq!(img.pixels.len(), 64 * 64 * 4);

        // Zoomed in: a patch.
        let view = MapView {
            scale: 6.0,
            cx: 0.5,
            cy: 0.5,
        };
        assert!(c.patch_map(12, NOW, view).unwrap());
        assert_eq!(
            c.render_map(Band::M20, 12, NOW, view, 64, 1.0, palette, opts.clone())
                .detail,
            MapDetail::Patch
        );
        let probe = c.probe(Band::M20, 12, NOW, view, 32.0, 32.0, 64.0).unwrap();
        assert!(probe.distance_km < 50.0, "{probe:?}");
        // In full, a point gets its own chance of contact.
        let tokyo = c.probe_at(Band::M20, 12, NOW, 35.7, 139.7);
        assert!(
            tokyo.chance.is_some_and(|c| (0.0..=1.0).contains(&c)),
            "{tokyo:?}"
        );
        assert_eq!(tokyo.grid, "PM95uq");

        // A fresh core reads the map from disk.
        let again = Core::open(
            std::env::temp_dir().join("hfp-app-flow/data"),
            std::env::temp_dir().join("hfp-app-flow/cache"),
        );
        assert!(again.has_fine_map(Band::M20, 12, NOW));
    }

    #[test]
    fn prefetch_readies_the_day_nearest_first() {
        let c = core("hfp-app-prefetch");
        c.set_from(Place::at("Valencia", "Spain", 39.47, -0.38))
            .unwrap();
        let mut done = vec![];
        while let Some(h) = c.prefetch(Band::M20, 12, NOW).unwrap() {
            done.push(h);
            assert!(done.len() <= 48, "prefetch never finishes");
        }
        // Every hour's coarse grid, then the band's fine maps from 12 outwards.
        assert_eq!(done.len(), 48);
        assert_eq!(&done[..3], &[12, 13, 11]);
        assert_eq!(&done[24..27], &[12, 13, 11]);
        assert!((0..24).all(|h| c.has_fine_map(Band::M20, h, NOW)));
    }

    #[test]
    fn places() {
        let c = core("hfp-app-places");
        assert_eq!(
            c.search_places("IM99sl".into(), "en".into(), false)
                .unwrap()[0]
                .name,
            "IM99sl"
        );
        assert!(
            !c.search_places("Valencia".into(), "en".into(), false)
                .unwrap()
                .is_empty()
        );
        let fix = c.place_at(39.47, -0.38);
        assert_eq!(fix.lat, 39.47);
    }
}
