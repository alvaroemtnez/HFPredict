//! Coverage maps: where a band reaches, at one hour, in every direction.
//!
//! Three grids, as HFcast draws them: a coarse whole-world grid for every
//! band in one run (read at once, and the source of every band's reach
//! percentage), a fine whole-world grid for the band on screen, and a patch
//! around the view when zoomed in. Each is corrected against a lattice of
//! daily-median SNR (see [`crate::correct`]).

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::Error;
use crate::bands::Band;
use crate::correct::{self, CentreField};
use crate::coverage::{self, Bounds, Grid, PatchGrid};
use crate::engine::{Engine, Model, RunContext};
use crate::geo::LatLon;

/// A grid as the engine answered it, before correction. Signal columns are
/// NaN where the engine printed nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct RawGrid {
    pub grid: Grid,
    pub snr: Vec<f32>,
    pub snr_low: Vec<f32>,
    pub snr_up: Vec<f32>,
}

impl RawGrid {
    /// Corrects every cell against the middle of its own day. Without a
    /// lattice the engine's reliability stands, which is what a map shows
    /// until the lattice arrives.
    pub fn corrected(
        &self,
        centre: Option<&CentreField>,
        required_snr_db: f64,
        kp_max_24h: Option<f64>,
    ) -> Grid {
        let Some(field) = centre else {
            return self.grid.clone();
        };
        let factors = correct::factors_for(kp_max_24h);
        let g = &self.grid;
        let opt = |v: f32| (!v.is_nan()).then_some(f64::from(v));
        let mut reliability = Vec::with_capacity(g.reliability.len());
        for row in 0..g.ny {
            let lat = g.lat_at(row);
            for column in 0..g.nx {
                let i = row * g.nx + column;
                let mid = field.at(lat, g.lon_at(column));
                reliability.push(correct::correct_point(
                    f64::from(g.reliability[i]),
                    opt(self.snr[i]),
                    opt(self.snr_low[i]),
                    opt(self.snr_up[i]),
                    mid,
                    required_snr_db,
                    factors,
                ) as f32);
            }
        }
        Grid {
            reliability,
            ..g.clone()
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(Option<f64>),
    Many(Vec<Option<f64>>),
}

impl OneOrMany {
    fn at(&self, index: usize) -> Option<f64> {
        match self {
            OneOrMany::One(v) => *v,
            OneOrMany::Many(v) => v.get(index).copied().flatten(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WirePoint {
    lat: f64,
    lon: f64,
    reliability: Option<OneOrMany>,
    takeoff_angle_deg: Option<OneOrMany>,
    snr: Option<OneOrMany>,
    snr_low_decile: Option<OneOrMany>,
    snr_up_decile: Option<OneOrMany>,
    median_snr: Option<OneOrMany>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireArea {
    lat_step: f64,
    lon_step: f64,
    freqs_mhz: Option<Vec<f64>>,
    #[serde(default)]
    points: Vec<WirePoint>,
}

/// The lattice a set of points sits on: rows and columns from the asked
/// steps, anchored at the world's half-step cell centres.
fn lattice(
    points: &[WirePoint],
    lat_step: f64,
    lon_step: f64,
    whole_world: bool,
) -> Option<(f64, f64, usize, usize)> {
    if whole_world {
        let ny = (180.0 / lat_step).round() as usize;
        let nx = (360.0 / lon_step).round() as usize;
        return Some((-90.0 + lat_step / 2.0, -180.0 + lon_step / 2.0, nx, ny));
    }
    let lat_min = points.iter().map(|p| p.lat).fold(f64::INFINITY, f64::min);
    let lat_max = points
        .iter()
        .map(|p| p.lat)
        .fold(f64::NEG_INFINITY, f64::max);
    let lon_min = points.iter().map(|p| p.lon).fold(f64::INFINITY, f64::min);
    let lon_max = points
        .iter()
        .map(|p| p.lon)
        .fold(f64::NEG_INFINITY, f64::max);
    if !lat_min.is_finite() {
        return None;
    }
    let ny = ((lat_max - lat_min) / lat_step).round() as usize + 1;
    let nx = ((lon_max - lon_min) / lon_step).round() as usize + 1;
    Some((lat_min, lon_min, nx, ny))
}

/// Places every point by its own coordinates (never trusting emission
/// order), one band's columns out of a possibly multi-band answer.
fn raw_grid(
    points: &[WirePoint],
    lat_step: f64,
    lon_step: f64,
    whole_world: bool,
    band_index: usize,
) -> Result<RawGrid, Error> {
    let (lat_min, lon_min, nx, ny) = lattice(points, lat_step, lon_step, whole_world)
        .ok_or_else(|| Error::Engine("the engine produced no coverage points".into()))?;
    let n = nx * ny;
    let mut grid = Grid {
        lat_min,
        lon_min,
        lat_step,
        lon_step,
        nx,
        ny,
        reliability: vec![0.0; n],
        takeoff_deg: vec![f32::NAN; n],
    };
    let (mut snr, mut low, mut up) = (vec![f32::NAN; n], vec![f32::NAN; n], vec![f32::NAN; n]);
    let get = |v: &Option<OneOrMany>| v.as_ref().and_then(|v| v.at(band_index));
    for p in points {
        let row = ((p.lat - lat_min) / lat_step).round() as i64;
        let mut column = ((p.lon - lon_min) / lon_step).round() as i64;
        if whole_world {
            column = column.rem_euclid(nx as i64);
        }
        if row < 0 || row >= ny as i64 || column < 0 || column >= nx as i64 {
            continue;
        }
        let i = row as usize * nx + column as usize;
        let nan = |v: Option<f64>| v.map_or(f32::NAN, |v| v as f32);
        grid.reliability[i] = get(&p.reliability).unwrap_or(0.0).clamp(0.0, 1.0) as f32;
        grid.takeoff_deg[i] = nan(get(&p.takeoff_angle_deg));
        snr[i] = nan(get(&p.snr));
        low[i] = nan(get(&p.snr_low_decile));
        up[i] = nan(get(&p.snr_up_decile));
    }
    Ok(RawGrid {
        grid,
        snr,
        snr_low: low,
        snr_up: up,
    })
}

fn check_echo(answer: &WireArea, bands: &[Band]) -> Result<(), Error> {
    if bands.len() < 2 {
        return Ok(());
    }
    let echoed = answer.freqs_mhz.as_deref().unwrap_or(&[]);
    let same = echoed.len() == bands.len()
        && echoed
            .iter()
            .zip(bands)
            .all(|(f, b)| (f - b.mhz()).abs() < 1e-4);
    if same {
        Ok(())
    } else {
        Err(Error::Engine(
            "the engine did not answer the bands asked for".into(),
        ))
    }
}

fn area_request(
    engine: &Engine,
    ctx: &RunContext,
    from: LatLon,
    hour: Option<u32>,
    bands: &[Band],
) -> Result<Map<String, Value>, Error> {
    let mut req = engine.base_request(ctx)?;
    req.insert("mode".into(), json!("area"));
    req.insert("fromLat".into(), json!(from.lat));
    req.insert("fromLon".into(), json!(from.lon));
    match hour {
        Some(h) => {
            req.insert("hour".into(), json!(h));
        }
        None => {
            req.insert("dailyMedian".into(), json!(true));
        }
    }
    if let [band] = bands {
        req.insert("freqMhz".into(), json!(band.mhz()));
    } else {
        req.insert(
            "freqsMhz".into(),
            json!(bands.iter().map(|b| b.mhz()).collect::<Vec<_>>()),
        );
    }
    Ok(req)
}

fn with_bounds(req: &mut Map<String, Value>, b: Bounds) {
    req.insert("latMin".into(), json!(b.lat_min));
    req.insert("latMax".into(), json!(b.lat_max));
    req.insert("lonMin".into(), json!(b.lon_min));
    req.insert("lonMax".into(), json!(b.lon_max));
}

/// Every band on the coarse whole-world grid, one hour, uncorrected.
pub fn coarse(
    engine: &Engine,
    ctx: &RunContext,
    from: LatLon,
    hour: u32,
) -> Result<Vec<(Band, RawGrid)>, Error> {
    let bands = Band::BY_FREQ;
    let mut req = area_request(engine, ctx, from, Some(hour), &bands)?;
    req.insert("latStep".into(), json!(coverage::LAT_STEP));
    req.insert("lonStep".into(), json!(coverage::LON_STEP));
    let answer: WireArea = engine.run(&req)?;
    check_echo(&answer, &bands)?;
    bands
        .iter()
        .enumerate()
        .map(|(i, &band)| {
            Ok((
                band,
                raw_grid(&answer.points, answer.lat_step, answer.lon_step, true, i)?,
            ))
        })
        .collect()
}

/// The lattice of daily middles for one band, or every band when `None`.
pub fn centres(
    engine: &Engine,
    ctx: &RunContext,
    from: LatLon,
    lat_step: f64,
    lon_step: f64,
    band: Option<Band>,
    threads: usize,
) -> Result<Vec<(Band, Option<CentreField>)>, Error> {
    let bands: Vec<Band> = band.map_or(Band::BY_FREQ.to_vec(), |b| vec![b]);
    let mut req = area_request(engine, ctx, from, None, &bands)?;
    req.insert("latStep".into(), json!(lat_step));
    req.insert("lonStep".into(), json!(lon_step));
    // The whole-day shape threads inside the engine.
    req.insert("threads".into(), json!(threads));
    let answer: WireArea = engine.run(&req)?;
    check_echo(&answer, &bands)?;
    Ok(bands
        .iter()
        .enumerate()
        .map(|(i, &b)| {
            let points: Vec<(f64, f64, f64)> = answer
                .points
                .iter()
                .map(|p| {
                    (
                        p.lat,
                        p.lon,
                        p.median_snr
                            .as_ref()
                            .and_then(|m| m.at(i))
                            .unwrap_or(correct::DEAD_SIGNAL_FLOOR_DB),
                    )
                })
                .collect();
            (
                b,
                CentreField::new(&points, answer.lat_step, answer.lon_step),
            )
        })
        .collect())
}

/// One band on the fine whole-world grid, uncorrected. Truecast threads
/// inside the engine; VOACAP's area driver is serial (it carries state from
/// point to point, as the Fortran does), so it runs in latitude strips.
pub fn fine(
    engine: &Engine,
    ctx: &RunContext,
    from: LatLon,
    hour: u32,
    band: Band,
    threads: usize,
) -> Result<RawGrid, Error> {
    let mut req = area_request(engine, ctx, from, Some(hour), &[band])?;
    let (lat_step, lon_step) = (coverage::FINE_LAT_STEP, coverage::FINE_LON_STEP);
    req.insert("latStep".into(), json!(lat_step));
    req.insert("lonStep".into(), json!(lon_step));
    let strips = match ctx.model {
        Model::Truecast => None,
        Model::Voacap => coverage::lat_strips(lat_step, lon_step, threads * 2),
    };
    let points: Vec<WirePoint> = match strips {
        None => {
            req.insert("threads".into(), json!(threads));
            engine.run::<WireArea>(&req)?.points
        }
        Some(strips) => {
            let answers = crate::parallel::map(&strips, threads, |b| {
                let mut part = req.clone();
                with_bounds(&mut part, *b);
                engine.run::<WireArea>(&part)
            });
            let mut all = vec![];
            for answer in answers {
                all.extend(answer?.points);
            }
            all
        }
    };
    raw_grid(&points, lat_step, lon_step, true, 0)
}

/// Every band on a patch around the view, uncorrected.
pub fn patch(
    engine: &Engine,
    ctx: &RunContext,
    from: LatLon,
    hour: u32,
    grid: &PatchGrid,
) -> Result<Vec<(Band, RawGrid)>, Error> {
    let bands = Band::BY_FREQ;
    let mut req = area_request(engine, ctx, from, Some(hour), &bands)?;
    req.insert("latStep".into(), json!(grid.lat_step));
    req.insert("lonStep".into(), json!(grid.lon_step));
    with_bounds(&mut req, grid.request_bounds());
    let answer: WireArea = engine.run(&req)?;
    check_echo(&answer, &bands)?;
    bands
        .iter()
        .enumerate()
        .map(|(i, &band)| {
            Ok((
                band,
                raw_grid(&answer.points, grid.lat_step, grid.lon_step, false, i)?,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::ctx;

    const VALENCIA: LatLon = LatLon {
        lat: 39.47,
        lon: -0.38,
    };

    #[test]
    fn coarse_grid_is_whole_and_corrects() {
        let engine = Engine::new(std::env::temp_dir().join("hfp-maps-coarse"));
        let c = ctx(Model::Voacap);
        let grids = coarse(&engine, &c, VALENCIA, 12).unwrap();
        assert_eq!(grids.len(), 10);
        let (_, g20) = grids.iter().find(|(b, _)| *b == Band::M20).unwrap();
        assert_eq!((g20.grid.nx, g20.grid.ny), (16, 12));
        let fields = centres(
            &engine,
            &c,
            VALENCIA,
            coverage::CENTRE_LAT_STEP,
            coverage::CENTRE_LON_STEP,
            None,
            4,
        )
        .unwrap();
        let field = fields
            .iter()
            .find(|(b, _)| *b == Band::M20)
            .unwrap()
            .1
            .as_ref()
            .unwrap();
        let fixed = g20.corrected(Some(field), 24.0, None);
        assert_eq!(fixed.reliability.len(), 192);
        assert!(fixed.reach() > 0.0 && fixed.reach() < 1.0);
    }

    #[test]
    fn voacap_strips_match_one_run() {
        // The strip cut must not change a single cell.
        let engine = Engine::new(std::env::temp_dir().join("hfp-maps-strips"));
        let c = ctx(Model::Voacap);
        let whole = fine(&engine, &c, VALENCIA, 12, Band::M20, 1).unwrap();
        let cut = fine(&engine, &c, VALENCIA, 12, Band::M20, 8).unwrap();
        assert_eq!(whole.grid.reliability.len(), 34_560);
        assert_eq!(whole, cut);
    }

    #[test]
    fn patch_lands_on_its_grid() {
        let engine = Engine::new(std::env::temp_dir().join("hfp-maps-patch"));
        let pg = coverage::patch_grid(VALENCIA.lat, VALENCIA.lon, 10.0).unwrap();
        let grids = patch(&engine, &ctx(Model::Truecast), VALENCIA, 12, &pg).unwrap();
        let g = &grids[0].1.grid;
        assert!((g.lat_min - pg.bounds.lat_min).abs() < 1e-6);
        assert!(g.nx * g.ny <= coverage::MAX_PATCH_POINTS);
    }
}
