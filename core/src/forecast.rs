//! The band table: every band at every hour, to one place or (with no
//! destination) as the share of directions reached.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::Error;
use crate::bands::Band;
use crate::correct::{self, CorrectedCell, RawCell};
use crate::engine::{Engine, Model, RunContext};
use crate::geo::{self, LatLon};
use crate::places::Place;
use crate::quality::REACHABLE;
use crate::ssn::Basis;

/// Roughly how far one F2 hop carries, km: the honest way to say why a long
/// path is harder (every bounce off the ground loses signal).
pub const HOP_KM: f64 = 3400.0;

/// One band at one hour.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Cell {
    pub band: Band,
    /// UTC hour, 0-23.
    pub hour: u32,
    /// Chance of contact (path) or share of directions reached (survey), 0..1.
    pub reliability: f64,
    /// Corrected median SNR, dB. Zero in a survey, where it has no meaning.
    pub snr: f64,
    pub takeoff_angle_deg: Option<f64>,
}

/// Where to set the dial, per UTC hour. A missing LUF means no frequency met
/// the requirement, not a very low one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct OperatingWindow {
    pub fot_by_hour: Vec<Option<f64>>,
    pub hpf_by_hour: Vec<Option<f64>>,
    pub luf_by_hour: Vec<Option<f64>>,
}

/// A day's forecast from one place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Forecast {
    pub from: Place,
    /// `None` for a survey: every direction rather than one path.
    pub to: Option<Place>,
    pub distance_km: Option<f64>,
    pub bearing_deg: Option<f64>,
    /// F2 hops on the path, at least one.
    pub hops: Option<u32>,
    /// The sunspot number shown for the run.
    pub ssn: f64,
    pub basis: Basis,
    pub model: Model,
    pub required_snr_db: i32,
    pub year: i32,
    pub month: u32,
    pub day: u32,
    /// Ten bands by 24 hours.
    pub cells: Vec<Cell>,
    /// Median MUF per UTC hour, MHz (0 where none).
    pub muf_by_hour: Vec<f64>,
    pub window: Option<OperatingWindow>,
}

impl Forecast {
    pub fn cell(&self, band: Band, hour: u32) -> Option<&Cell> {
        self.cells.iter().find(|c| c.band == band && c.hour == hour)
    }

    /// True when nothing opens at any hour: a real answer that looks like a
    /// failed one, so the screen says it in words.
    pub fn all_closed(&self) -> bool {
        self.cells.iter().all(|c| crate::quality::Quality::of(c.reliability) == crate::quality::Quality::Closed)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireCell {
    hour: u32,
    freq_mhz: f64,
    reliability: f64,
    snr: f64,
    snr_low_decile: Option<f64>,
    snr_up_decile: Option<f64>,
    takeoff_angle_deg: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WirePrediction {
    #[serde(default)]
    cells: Vec<WireCell>,
    #[serde(default)]
    muf_by_hour: Vec<Option<f64>>,
    fot_by_hour: Option<Vec<Option<f64>>>,
    hpf_by_hour: Option<Vec<Option<f64>>>,
    luf_by_hour: Option<Vec<Option<f64>>>,
}

fn hours(values: &[Option<f64>]) -> Vec<Option<f64>> {
    (0..24).map(|h| values.get(h).copied().flatten().filter(|v| v.is_finite())).collect()
}

/// One point-to-point run, all ten bands, corrected.
struct PathRun {
    cells: Vec<CorrectedCell>,
    muf_by_hour: Vec<f64>,
    window: Option<OperatingWindow>,
}

fn run_path(engine: &Engine, ctx: &RunContext, from: LatLon, to: LatLon) -> Result<PathRun, Error> {
    let mut req = engine.base_request(ctx)?;
    req.insert("fromLat".into(), json!(from.lat));
    req.insert("fromLon".into(), json!(from.lon));
    req.insert("toLat".into(), json!(to.lat));
    req.insert("toLon".into(), json!(to.lon));
    req.insert("bands".into(), json!(Band::all_mhz()));
    let answer: WirePrediction = engine.run(&req)?;
    let raw: Vec<RawCell> = answer
        .cells
        .iter()
        .filter_map(|c| {
            Some(RawCell {
                band: Band::from_mhz(c.freq_mhz)?,
                hour: c.hour,
                reliability: c.reliability.clamp(0.0, 1.0),
                snr: c.snr,
                snr_low_decile: c.snr_low_decile,
                snr_up_decile: c.snr_up_decile,
                takeoff_angle_deg: c.takeoff_angle_deg,
            })
        })
        .collect();
    if raw.is_empty() {
        return Err(Error::Engine("the engine produced no usable rows".into()));
    }
    let factors = correct::factors_for(ctx.nowcast.map(|n| n.kp_max_24h));
    let required = f64::from(ctx.station.mode.required_snr_db());
    let window = match (&answer.fot_by_hour, &answer.hpf_by_hour, &answer.luf_by_hour) {
        (None, None, None) => None,
        (fot, hpf, luf) => Some(OperatingWindow {
            fot_by_hour: hours(fot.as_deref().unwrap_or(&[])),
            hpf_by_hour: hours(hpf.as_deref().unwrap_or(&[])),
            luf_by_hour: hours(luf.as_deref().unwrap_or(&[])),
        }),
    };
    Ok(PathRun {
        cells: correct::correct_cells(&raw, required, factors),
        muf_by_hour: hours(&answer.muf_by_hour).into_iter().map(|m| m.unwrap_or(0.0)).collect(),
        window,
    })
}

fn to_cells(cells: &[CorrectedCell]) -> Vec<Cell> {
    let mut out: Vec<Cell> = cells
        .iter()
        .map(|c| Cell {
            band: c.band,
            hour: c.hour,
            reliability: c.reliability,
            snr: c.snr,
            takeoff_angle_deg: c.takeoff_angle_deg,
        })
        .collect();
    out.sort_by_key(|c| (c.band, c.hour));
    out
}

/// The forecast to one place.
pub fn path(engine: &Engine, ctx: &RunContext, from: &Place, to: &Place) -> Result<Forecast, Error> {
    let run = run_path(engine, ctx, from.lat_lon(), to.lat_lon())?;
    let km = geo::distance_km(from.lat_lon(), to.lat_lon());
    let (ssn, basis) = ctx.ssn();
    Ok(Forecast {
        from: from.clone(),
        to: Some(to.clone()),
        distance_km: Some(km),
        bearing_deg: Some(geo::bearing_deg(from.lat_lon(), to.lat_lon())),
        hops: Some(((km / HOP_KM).ceil() as u32).max(1)),
        ssn,
        basis,
        model: ctx.model,
        required_snr_db: ctx.station.mode.required_snr_db(),
        year: ctx.date.year,
        month: ctx.date.month,
        day: ctx.date.day,
        cells: to_cells(&run.cells),
        muf_by_hour: run.muf_by_hour,
        window: run.window,
    })
}

/// Sixteen compass directions at three ranges: regional, continental and
/// the far side of an ocean. Each sample is an ordinary path run, so 48 of
/// them cost about a tenth of an area run per cell (HFcast `shared/survey.ts`).
pub const SAMPLE_BEARINGS: usize = 16;
pub const SAMPLE_RANGES_KM: [f64; 3] = [1500.0, 4000.0, 8000.0];

pub fn sample_points(from: LatLon) -> Vec<LatLon> {
    (0..SAMPLE_BEARINGS)
        .flat_map(|i| {
            let bearing = i as f64 * 360.0 / SAMPLE_BEARINGS as f64;
            SAMPLE_RANGES_KM.iter().map(move |&km| geo::point_from(from, bearing, km))
        })
        .collect()
}

/// The forecast with no destination: each cell is the share of sampled
/// directions this band reaches at this hour, counted against the same
/// threshold as the map's reach figure.
pub fn survey(engine: &Engine, ctx: &RunContext, from: &Place, threads: usize) -> Result<Forecast, Error> {
    let points = sample_points(from.lat_lon());
    let runs = crate::parallel::map(&points, threads, |p| run_path(engine, ctx, from.lat_lon(), *p))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let mut reached = [[0u32; 24]; 10];
    for run in &runs {
        for c in &run.cells {
            if c.reliability >= REACHABLE && c.hour < 24 {
                reached[c.band.freq_index()][c.hour as usize] += 1;
            }
        }
    }
    let total = runs.len().max(1) as f64;
    let cells = Band::BY_FREQ
        .iter()
        .flat_map(|&band| {
            let reached = &reached;
            (0..24u32).map(move |hour| Cell {
                band,
                hour,
                reliability: f64::from(reached[band.freq_index()][hour as usize]) / total,
                snr: 0.0,
                takeoff_angle_deg: None,
            })
        })
        .collect();
    let (ssn, basis) = ctx.ssn();
    Ok(Forecast {
        from: from.clone(),
        to: None,
        distance_km: None,
        bearing_deg: None,
        hops: None,
        ssn,
        basis,
        model: ctx.model,
        required_snr_db: ctx.station.mode.required_snr_db(),
        year: ctx.date.year,
        month: ctx.date.month,
        day: ctx.date.day,
        cells,
        muf_by_hour: mid_range_muf(&runs),
        window: None,
    })
}

/// The median MUF per hour over the middle ring, whose paths most bands are
/// actually working.
fn mid_range_muf(runs: &[PathRun]) -> Vec<f64> {
    let middle = SAMPLE_RANGES_KM.len() / 2;
    (0..24)
        .map(|hour| {
            let mut values: Vec<f64> = runs
                .iter()
                .enumerate()
                .filter(|(i, _)| i % SAMPLE_RANGES_KM.len() == middle)
                .filter_map(|(_, r)| r.muf_by_hour.get(hour).copied())
                .filter(|v| *v > 0.0)
                .collect();
            values.sort_by(f64::total_cmp);
            values.get(values.len() / 2).copied().unwrap_or(0.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::ctx;

    fn valencia() -> Place {
        Place::at("Valencia", "Spain", 39.47, -0.38)
    }

    #[test]
    fn path_forecast_has_every_band_and_hour() {
        let engine = Engine::new(std::env::temp_dir().join("hfp-forecast-test"));
        for model in [Model::Voacap, Model::Truecast] {
            let f = path(&engine, &ctx(model), &valencia(), &Place::at("London", "UK", 51.5, -0.1)).unwrap();
            assert_eq!(f.cells.len(), 240, "{model:?}");
            assert_eq!(f.hops, Some(1));
            assert!(f.cells.iter().all(|c| (0.0..=1.0).contains(&c.reliability)));
            assert!(f.muf_by_hour.iter().any(|m| *m > 0.0));
            assert!(f.window.is_some());
            // A European path in October opens 20m at midday.
            assert!(f.cell(Band::M20, 12).unwrap().reliability > 0.4, "{model:?}");
        }
    }

    #[test]
    fn survey_counts_directions() {
        let engine = Engine::new(std::env::temp_dir().join("hfp-survey-test"));
        let f = survey(&engine, &ctx(Model::Truecast), &valencia(), 4).unwrap();
        assert_eq!(f.cells.len(), 240);
        assert!(f.to.is_none());
        // Shares come in 48ths.
        assert!(f.cells.iter().all(|c| ((c.reliability * 48.0).round() - c.reliability * 48.0).abs() < 1e-9));
        assert!(f.cells.iter().any(|c| c.reliability > 0.0));
    }
}
