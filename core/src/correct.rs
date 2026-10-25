//! The empirical correction HFcast applies to VOACAP's numbers.
//!
//! Measured against WSPR reception reports across the solar cycle, VOACAP
//! puts a circuit's peaks and fades at the right hours but predicts a daily
//! swing about four times larger than reality, and claims a day-to-day spread
//! wider than measured. So each hour's SNR is shrunk toward the middle of its
//! own day,
//!
//! ```text
//! corrected = centre + swing * (predicted - centre)
//! ```
//!
//! and reliability is recomputed from the corrected median with the deciles
//! scaled by the validated spread factors (a decile is 1.2816 standard
//! deviations of a normal distribution, which reproduces the engine's own
//! reliability from its own numbers). After a geomagnetic storm the downward
//! spread widens. Fits and evidence: hfcast-engine `docs/accuracy.md`,
//! `docs/reliability.md`; ported from HFcast `shared/correct.ts` and
//! `shared/correctMap.ts`.

use crate::bands::Band;

/// How much of VOACAP's predicted daily swing is real.
pub const SWING_FACTOR: f64 = 0.25;
/// How much of VOACAP's claimed day-to-day spread is real, below and above
/// the median.
pub const SPREAD_FACTOR_LOW: f64 = 0.4;
pub const SPREAD_FACTOR_UP: f64 = 0.59;

pub const STORM_WIDENING_START_KP: f64 = 4.75;
pub const STORM_WIDENING_PER_KP: f64 = 0.5;
pub const STORM_WIDENING_CAP: f64 = 2.5;

/// A decile is this many standard deviations of a normal distribution.
const DECILE_TO_SIGMA: f64 = 1.2816;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Factors {
    pub swing: f64,
    pub spread_low: f64,
    pub spread_up: f64,
}

pub const VALIDATED: Factors = Factors {
    swing: SWING_FACTOR,
    spread_low: SPREAD_FACTOR_LOW,
    spread_up: SPREAD_FACTOR_UP,
};

/// How much wider the downward spread is after a storm, from the highest Kp
/// of the last 24 hours.
pub fn storm_widening(kp_max_24h: f64) -> f64 {
    (1.0 + STORM_WIDENING_PER_KP * (kp_max_24h - STORM_WIDENING_START_KP))
        .clamp(1.0, STORM_WIDENING_CAP)
}

/// The factors for current conditions; `None` (no live reading) is the
/// quiet-day calibration.
pub fn factors_for(kp_max_24h: Option<f64>) -> Factors {
    match kp_max_24h {
        None => VALIDATED,
        Some(kp) => Factors {
            spread_low: VALIDATED.spread_low * storm_widening(kp),
            ..VALIDATED
        },
    }
}

/// Standard normal CDF, Abramowitz-Stegun 7.1.26 (as HFcast, so the numbers
/// match to the last digit it prints).
pub fn phi(z: f64) -> f64 {
    let x = z.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1.0 - poly * (-x * x).exp();
    if z >= 0.0 {
        0.5 * (1.0 + erf)
    } else {
        0.5 * (1.0 - erf)
    }
}

/// The chance a day's SNR meets the requirement, from the corrected median
/// and the scaled deciles.
fn reliability_from(snr: f64, required: f64, low_decile: f64, up_decile: f64) -> f64 {
    let z = snr - required;
    let decile = if z >= 0.0 { low_decile } else { up_decile };
    if decile <= 0.0 {
        return if z >= 0.0 { 1.0 } else { 0.0 };
    }
    phi(z / (decile / DECILE_TO_SIGMA))
}

/// One band at one hour, as the engine reported it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawCell {
    pub band: Band,
    pub hour: u32,
    pub reliability: f64,
    pub snr: f64,
    pub snr_low_decile: Option<f64>,
    pub snr_up_decile: Option<f64>,
    pub takeoff_angle_deg: Option<f64>,
}

/// One band at one hour after correction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CorrectedCell {
    pub band: Band,
    pub hour: u32,
    pub reliability: f64,
    pub snr: f64,
    pub takeoff_angle_deg: Option<f64>,
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

/// Applies the swing correction per band and recomputes reliability. Cells
/// without both deciles keep the engine's reliability: moving the median
/// without the spread would be a guess.
pub fn correct_cells(
    cells: &[RawCell],
    required_snr_db: f64,
    factors: Factors,
) -> Vec<CorrectedCell> {
    let centre_of = |band: Band| {
        let mut snrs: Vec<f64> = cells
            .iter()
            .filter(|c| c.band == band)
            .map(|c| c.snr)
            .collect();
        median(&mut snrs)
    };
    let centres: Vec<(Band, f64)> = Band::BY_FREQ.iter().map(|&b| (b, centre_of(b))).collect();
    cells
        .iter()
        .map(|cell| {
            let centre = centres
                .iter()
                .find(|(b, _)| *b == cell.band)
                .map_or(cell.snr, |(_, c)| *c);
            let snr = centre + factors.swing * (cell.snr - centre);
            let reliability = match (cell.snr_low_decile, cell.snr_up_decile) {
                (Some(low), Some(up)) => reliability_from(
                    snr,
                    required_snr_db,
                    low * factors.spread_low,
                    up * factors.spread_up,
                ),
                _ => cell.reliability,
            };
            CorrectedCell {
                band: cell.band,
                hour: cell.hour,
                reliability: reliability.clamp(0.0, 1.0),
                snr,
                takeoff_angle_deg: cell.takeoff_angle_deg,
            }
        })
        .collect()
}

/// Below this a lattice value is read as the floor before averaging: where a
/// band is shut the engine reports numbers like -272 dB, and averaging one
/// against an open neighbour would describe neither place.
pub const DEAD_SIGNAL_FLOOR_DB: f64 = -60.0;

/// A lattice of daily-median SNR, readable at any coordinate. Built from a
/// `dailyMedian` area run: the map runs at full detail for one hour while a
/// much coarser lattice runs the whole day, and each map cell takes its
/// middle from the four lattice points around it.
#[derive(Clone, Debug, PartialEq)]
pub struct CentreField {
    pub lat_min: f64,
    pub lon_min: f64,
    pub lat_step: f64,
    pub lon_step: f64,
    pub rows: usize,
    pub columns: usize,
    /// Row-major, south to north then west to east, already floored.
    pub values: Vec<f32>,
}

impl CentreField {
    /// Arranges `(lat, lon, median)` points by their own coordinates. `None`
    /// where there are too few to interpolate between.
    pub fn new(points: &[(f64, f64, f64)], lat_step: f64, lon_step: f64) -> Option<CentreField> {
        if points.len() < 4 || lat_step <= 0.0 || lon_step <= 0.0 {
            return None;
        }
        let lat_min = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let lon_min = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let lat_max = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        let lon_max = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
        let rows = ((lat_max - lat_min) / lat_step).round() as usize + 1;
        let columns = ((lon_max - lon_min) / lon_step).round() as usize + 1;
        if rows < 2 || columns < 2 || rows * columns != points.len() {
            return None;
        }
        let mut values = vec![DEAD_SIGNAL_FLOOR_DB as f32; points.len()];
        for &(lat, lon, median) in points {
            let row = ((lat - lat_min) / lat_step).round() as usize;
            let column = ((lon - lon_min) / lon_step).round() as usize;
            values[row * columns + column] = median.max(DEAD_SIGNAL_FLOOR_DB) as f32;
        }
        Some(CentreField {
            lat_min,
            lon_min,
            lat_step,
            lon_step,
            rows,
            columns,
            values,
        })
    }

    /// Bilinear between the four lattice points around a coordinate.
    /// Longitude wraps when the lattice covers the whole world; latitude
    /// clamps at the outermost rows.
    pub fn at(&self, lat: f64, lon: f64) -> f64 {
        let (rows, columns) = (self.rows, self.columns);
        let y = ((lat - self.lat_min) / self.lat_step).clamp(0.0, (rows - 1) as f64);
        let row = (y.floor() as usize).min(rows - 2);
        let dy = y - row as f64;
        let wrapped = columns as f64 * self.lon_step >= 359.9;
        let x = (lon - self.lon_min) / self.lon_step;
        let (column, next, dx) = if wrapped {
            let c = x.floor().rem_euclid(columns as f64) as usize;
            (c, (c + 1) % columns, x - x.floor())
        } else {
            let xc = x.clamp(0.0, (columns - 1) as f64);
            let c = (xc.floor() as usize).min(columns - 2);
            (c, c + 1, xc - c as f64)
        };
        let at = |r: usize, c: usize| f64::from(self.values[r * columns + c]);
        let lower = at(row, column) * (1.0 - dx) + at(row, next) * dx;
        let upper = at(row + 1, column) * (1.0 - dx) + at(row + 1, next) * dx;
        lower * (1.0 - dy) + upper * dy
    }
}

/// Corrects one map point against the middle of its own day: `correct_cells`
/// arithmetic, value for value, for a shape that holds one hour.
pub fn correct_point(
    reliability: f64,
    snr: Option<f64>,
    low: Option<f64>,
    up: Option<f64>,
    centre: f64,
    required_snr_db: f64,
    factors: Factors,
) -> f64 {
    let (Some(level), Some(low), Some(up)) = (snr, low, up) else {
        return reliability;
    };
    let snr = centre + factors.swing * (level - centre);
    reliability_from(
        snr,
        required_snr_db,
        low * factors.spread_low,
        up * factors.spread_up,
    )
    .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phi_is_a_cdf() {
        assert!((phi(0.0) - 0.5).abs() < 1e-9);
        assert!((phi(1.2816) - 0.9).abs() < 1e-3);
        assert!((phi(-1.2816) - 0.1).abs() < 1e-3);
    }

    #[test]
    fn storm_widening_ramps_and_caps() {
        assert_eq!(storm_widening(3.0), 1.0);
        assert!((storm_widening(5.75) - 1.5).abs() < 1e-12);
        assert_eq!(storm_widening(9.0), 2.5);
        assert_eq!(factors_for(None), VALIDATED);
        assert!((factors_for(Some(6.75)).spread_low - 0.8).abs() < 1e-12);
    }

    fn raw(hour: u32, snr: f64) -> RawCell {
        RawCell {
            band: Band::M20,
            hour,
            reliability: 0.5,
            snr,
            snr_low_decile: Some(10.0),
            snr_up_decile: Some(10.0),
            takeoff_angle_deg: Some(10.0),
        }
    }

    #[test]
    fn swing_shrinks_toward_the_median() {
        let cells: Vec<RawCell> = (0..4)
            .map(|h| raw(h, [0.0, 10.0, 20.0, 40.0][h as usize]))
            .collect();
        let out = correct_cells(&cells, 24.0, VALIDATED);
        // median 15: 0 -> 11.25, 40 -> 21.25
        assert!((out[0].snr - 11.25).abs() < 1e-9);
        assert!((out[3].snr - 21.25).abs() < 1e-9);
        // 21.25 against 24 with the up decile 5.9 dB: z = -2.75 / 4.604
        assert!((out[3].reliability - phi(-2.75 / (5.9 / DECILE_TO_SIGMA))).abs() < 1e-12);
    }

    #[test]
    fn without_deciles_the_engine_value_stands() {
        let mut cell = raw(0, 30.0);
        cell.snr_low_decile = None;
        assert_eq!(correct_cells(&[cell], 24.0, VALIDATED)[0].reliability, 0.5);
    }

    #[test]
    fn centre_field_interpolates_and_wraps() {
        let mut points = vec![];
        for (r, lat) in [-15.0, 15.0].into_iter().enumerate() {
            for c in 0..16 {
                let lon = -180.0 + 11.25 + 22.5 * c as f64;
                points.push((lat, lon, (r * 100 + c) as f64));
            }
        }
        let field = CentreField::new(&points, 30.0, 22.5).unwrap();
        assert_eq!((field.rows, field.columns), (2, 16));
        // Halfway between row 0 col 0 (0) and row 1 col 0 (100)
        assert!((field.at(0.0, -168.75) - 50.0).abs() < 1e-6);
        // Across the antimeridian: between col 15 (15) and col 0 (0)
        assert!((field.at(-15.0, 180.0) - 7.5).abs() < 1e-6);
        // Dead values are floored
        let dead: Vec<_> = points.iter().map(|p| (p.0, p.1, -272.0)).collect();
        assert_eq!(
            CentreField::new(&dead, 30.0, 22.5).unwrap().at(0.0, 0.0),
            DEAD_SIGNAL_FLOOR_DB
        );
    }
}
