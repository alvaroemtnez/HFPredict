//! Coverage grids: the lattices maps run on, what "reach" means on them, and
//! the cell arithmetic that keeps a regional patch on the same lattice as the
//! whole world. Ported from HFcast `shared/coverageGrid.ts`,
//! `shared/coveragePatch.ts` and `shared/shard.ts`.

use crate::quality::REACHABLE;

/// The coarse whole-world grid: 12 rows of 16, 192 points. Wider in
/// longitude because meridians converge.
pub const LAT_STEP: f64 = 15.0;
pub const LON_STEP: f64 = 22.5;

/// The fine grid: 144 rows of 240, 34,560 points. Both divide the world and
/// a coarse cell evenly, so fine cells nest inside coarse ones.
pub const FINE_LAT_STEP: f64 = 1.25;
pub const FINE_LON_STEP: f64 = 1.5;

/// The lattice of daily middles the coarse grid is corrected against (all
/// bands in one whole-day run).
pub const CENTRE_LAT_STEP: f64 = 15.0;
pub const CENTRE_LON_STEP: f64 = 22.5;
/// The finer lattice the whole-world fine grid waits for (one band).
pub const FINE_CENTRE_LAT_STEP: f64 = 5.0;
pub const FINE_CENTRE_LON_STEP: f64 = 7.5;

/// The cell sizes a patch may use, coarsest first. Every rung divides the
/// world and a coarse cell evenly.
pub const PATCH_STEPS: [(f64, f64); 6] = [
    (15.0, 22.5),
    (7.5, 11.25),
    (5.0, 7.5),
    (2.5, 3.75),
    (1.25, 1.5),
    (0.625, 0.75),
];
/// The whole cost control of a patch: hold the count and the run costs the
/// same wherever the reader looks. HFcast used 700 for JavaScript on slow
/// phones; natively 2,000 (about 0.15 s on a desktop) makes a patch finer
/// than the whole-world fine grid from a moderate zoom on.
pub const MAX_PATCH_POINTS: usize = 2000;
/// About 1,100 km either side of the centre.
pub const PATCH_HALF_LAT_DEG: f64 = 10.0;
/// Meridians converge; past about 70 degrees the patch is held narrower.
pub const PATCH_MAX_HALF_LON_DEG: f64 = 30.0;
pub const PATCH_MIN_HALF_LON_DEG: f64 = 2.0 * FINE_LON_STEP;

/// A rectangle in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub lat_min: f64,
    pub lat_max: f64,
    pub lon_min: f64,
    pub lon_max: f64,
}

/// A coverage map on a regular lat/lon lattice, row-major from the
/// south-west. `lat_min`/`lon_min` are cell centres.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub lat_min: f64,
    pub lon_min: f64,
    pub lat_step: f64,
    pub lon_step: f64,
    pub nx: usize,
    pub ny: usize,
    /// 0..1, one per cell.
    pub reliability: Vec<f32>,
    /// Transmit take-off angle in degrees, NaN where the engine had none.
    pub takeoff_deg: Vec<f32>,
}

impl Grid {
    pub fn lat_at(&self, row: usize) -> f64 {
        self.lat_min + row as f64 * self.lat_step
    }

    pub fn lon_at(&self, column: usize) -> f64 {
        self.lon_min + column as f64 * self.lon_step
    }

    pub fn is_whole_world(&self) -> bool {
        self.nx as f64 * self.lon_step >= 359.9
    }

    /// The cell containing a coordinate, if the grid covers it.
    pub fn index_of(&self, lat: f64, lon: f64) -> Option<usize> {
        let row = ((lat - self.lat_min) / self.lat_step + 0.5).floor();
        if row < 0.0 || row >= self.ny as f64 {
            return None;
        }
        let x = (lon - self.lon_min) / self.lon_step + 0.5;
        let column = if self.is_whole_world() {
            x.floor().rem_euclid(self.nx as f64)
        } else {
            let c = x.floor();
            if c < 0.0 || c >= self.nx as f64 {
                return None;
            }
            c
        };
        Some(row as usize * self.nx + column as usize)
    }

    /// The coarse whole-world preview, drawn smoothed rather than as cells.
    pub fn is_coarse(&self) -> bool {
        self.lat_step >= LAT_STEP - 1e-9 && self.is_whole_world()
    }

    /// The reliability shown at a point: a bilinear blend of the four
    /// nearest cell centres (wrapping in longitude on a whole-world grid,
    /// held at the edges of a regional one), so the map draws smooth
    /// contours rather than cells, and a point's value is the one painted
    /// there. `None` outside the grid.
    pub fn reliability_at(&self, lat: f64, lon: f64) -> Option<f32> {
        self.index_of(lat, lon)?;
        if self.nx < 2 || self.ny < 2 {
            return self.index_of(lat, lon).map(|i| self.reliability[i]);
        }
        let fy = ((lat - self.lat_min) / self.lat_step).clamp(0.0, (self.ny - 1) as f64);
        let r0 = (fy.floor() as usize).min(self.ny - 2);
        let ty = (fy - r0 as f64) as f32;
        let (c0, c1, tx) = if self.is_whole_world() {
            let fx = ((lon - self.lon_min) / self.lon_step).rem_euclid(self.nx as f64);
            let c0 = (fx.floor() as usize) % self.nx;
            (c0, (c0 + 1) % self.nx, (fx - fx.floor()) as f32)
        } else {
            let fx = ((lon - self.lon_min) / self.lon_step).clamp(0.0, (self.nx - 1) as f64);
            let c0 = (fx.floor() as usize).min(self.nx - 2);
            (c0, c0 + 1, (fx - c0 as f64) as f32)
        };
        let v = |r: usize, c: usize| self.reliability[r * self.nx + c];
        let row = |r: usize| v(r, c0) * (1.0 - tx) + v(r, c1) * tx;
        Some(row(r0) * (1.0 - ty) + row(r0 + 1) * ty)
    }

    /// The rectangle a cell covers.
    pub fn cell_bounds(&self, i: usize) -> Bounds {
        let (lat, lon) = (self.lat_at(i / self.nx), self.lon_at(i % self.nx));
        Bounds {
            lat_min: (lat - self.lat_step / 2.0).max(-90.0),
            lat_max: (lat + self.lat_step / 2.0).min(90.0),
            lon_min: lon - self.lon_step / 2.0,
            lon_max: lon + self.lon_step / 2.0,
        }
    }

    /// The share of the sphere this grid reaches, weighted by area (cosine of
    /// latitude), so polar slivers do not count as much as the tropics.
    pub fn reach(&self) -> f64 {
        let (mut hit, mut total) = (0.0, 0.0);
        for row in 0..self.ny {
            let weight = self.lat_at(row).to_radians().cos();
            for column in 0..self.nx {
                total += weight;
                if f64::from(self.reliability[row * self.nx + column]) >= REACHABLE {
                    hit += weight;
                }
            }
        }
        if total > 0.0 { hit / total } else { 0.0 }
    }

    pub fn bounds(&self) -> Bounds {
        Bounds {
            lat_min: self.lat_min,
            lat_max: self.lat_at(self.ny.saturating_sub(1)),
            lon_min: self.lon_min,
            lon_max: self.lon_at(self.nx.saturating_sub(1)),
        }
    }
}

/// One axis of a run: `n` cell centres from `min` to `max`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axis {
    pub first: i64,
    pub last: i64,
    pub width: f64,
    pub bands: i64,
}

/// The engine's own arithmetic for which world cells a range covers, so a
/// request describes the grid that will run. `edge` and `span` are -90/180
/// for latitude and -180/360 for longitude.
pub fn axis_of(lo: f64, hi: f64, step: f64, edge: f64, span: f64) -> Option<Axis> {
    let bands = (span / step).round() as i64;
    if bands < 2 {
        return None;
    }
    let width = span / bands as f64;
    let index = |deg: f64| (deg - edge) / width - 0.5;
    let first = (index(lo).ceil() as i64).max(0);
    let last = (index(hi).floor() as i64).min(bands - 1);
    (last >= first).then_some(Axis {
        first,
        last,
        width,
        bands,
    })
}

fn centre(axis: &Axis, edge: f64, i: i64) -> f64 {
    edge + (i as f64 + 0.5) * axis.width
}

/// A quarter of a cell outside the first and last point, so no rounding in
/// the engine can drop or add a row.
fn edges(axis: &Axis, edge: f64, first: i64, last: i64) -> (f64, f64) {
    (
        centre(axis, edge, first) - axis.width / 4.0,
        centre(axis, edge, last) + axis.width / 4.0,
    )
}

/// The whole world cut into latitude strips that together run exactly the
/// whole-world lattice. `None` when the grid is too small to be worth it.
pub fn lat_strips(lat_step: f64, lon_step: f64, strips: usize) -> Option<Vec<Bounds>> {
    let lat = axis_of(-90.0, 90.0, lat_step, -90.0, 180.0)?;
    let lon = axis_of(-180.0, 180.0, lon_step, -180.0, 360.0)?;
    let rows = (lat.last - lat.first + 1) as usize;
    let strips = strips.min(rows / 2);
    if strips < 2 {
        return None;
    }
    let (lon_min, lon_max) = edges(&lon, -180.0, lon.first, lon.last);
    let mut next = lat.first;
    Some(
        (0..strips)
            .map(|i| {
                let size = (rows / strips + usize::from(i < rows % strips)) as i64;
                let (lat_min, lat_max) = edges(&lat, -90.0, next, next + size - 1);
                next += size;
                Bounds {
                    lat_min,
                    lat_max,
                    lon_min,
                    lon_max,
                }
            })
            .collect(),
    )
}

/// A rectangle and the cell size chosen to cover it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatchGrid {
    /// First and last cell centres on each axis.
    pub bounds: Bounds,
    pub lat_step: f64,
    pub lon_step: f64,
}

impl PatchGrid {
    /// The edges to send the engine: a quarter cell outside the points.
    pub fn request_bounds(&self) -> Bounds {
        Bounds {
            lat_min: self.bounds.lat_min - self.lat_step / 4.0,
            lat_max: self.bounds.lat_max + self.lat_step / 4.0,
            lon_min: self.bounds.lon_min - self.lon_step / 4.0,
            lon_max: self.bounds.lon_max + self.lon_step / 4.0,
        }
    }

    /// Stable across views that produce the same grid, so panning within a
    /// cell does not spend an engine run on an answer already held.
    pub fn key(&self) -> String {
        let b = self.bounds;
        format!(
            "{},{},{},{},{},{}",
            b.lat_min, b.lat_max, b.lon_min, b.lon_max, self.lat_step, self.lon_step
        )
    }
}

/// The rectangle around a point, narrowed symmetrically near the
/// antimeridian (the engine counts eastward from the western edge and cannot
/// express a rectangle across it). `None` when too little is left.
pub fn patch_bounds(lat: f64, lon: f64, half_lat_deg: f64) -> Option<Bounds> {
    let half_lat = half_lat_deg.min(PATCH_HALF_LAT_DEG);
    let wanted = PATCH_MAX_HALF_LON_DEG.min(half_lat / lat.to_radians().cos().max(1e-6));
    let half_lon = wanted.min(180.0 - lon.abs());
    if half_lon < wanted.min(PATCH_MIN_HALF_LON_DEG) {
        return None;
    }
    Some(Bounds {
        lat_min: (lat - half_lat).max(-90.0),
        lat_max: (lat + half_lat).min(90.0),
        lon_min: lon - half_lon,
        lon_max: lon + half_lon,
    })
}

/// The rectangle to run for a view centred on `lat`/`lon` showing
/// `half_lat_deg` either side, at the finest rung whose point count fits the
/// budget.
pub fn patch_grid(lat: f64, lon: f64, half_lat_deg: f64) -> Option<PatchGrid> {
    let b = patch_bounds(lat, lon, half_lat_deg)?;
    let count = |(ls, ws): (f64, f64)| {
        ((b.lat_max - b.lat_min) / ls).ceil() as usize
            * ((b.lon_max - b.lon_min) / ws).ceil() as usize
    };
    let (lat_step, lon_step) = PATCH_STEPS
        .iter()
        .copied()
        .rfind(|&s| count(s) <= MAX_PATCH_POINTS)
        .unwrap_or(PATCH_STEPS[0]);
    let down = axis_of(b.lat_min, b.lat_max, lat_step, -90.0, 180.0)?;
    let across = axis_of(b.lon_min, b.lon_max, lon_step, -180.0, 360.0)?;
    // The engine needs at least two points on each axis.
    if down.last - down.first < 1 || across.last - across.first < 1 {
        return None;
    }
    Some(PatchGrid {
        bounds: Bounds {
            lat_min: centre(&down, -90.0, down.first),
            lat_max: centre(&down, -90.0, down.last),
            lon_min: centre(&across, -180.0, across.first),
            lon_max: centre(&across, -180.0, across.last),
        },
        lat_step,
        lon_step,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coarse_world() -> Grid {
        Grid {
            lat_min: -82.5,
            lon_min: -168.75,
            lat_step: LAT_STEP,
            lon_step: LON_STEP,
            nx: 16,
            ny: 12,
            reliability: (0..192).map(|i| (i % 16) as f32 / 15.0).collect(),
            takeoff_deg: vec![f32::NAN; 192],
        }
    }

    #[test]
    fn coarse_preview_is_smooth_and_exact_at_centres() {
        let g = coarse_world();
        assert!(g.is_coarse());
        // At a centre: the centre's value.
        let i = g.index_of(7.5, 11.25).unwrap();
        assert_eq!(g.reliability_at(7.5, 11.25), Some(g.reliability[i]));
        // Half-way between two centres: their mean.
        let a = g.reliability_at(7.5, 0.0).unwrap();
        let (l, r) = (
            g.reliability[g.index_of(7.5, -11.25).unwrap()],
            g.reliability[g.index_of(7.5, 11.25).unwrap()],
        );
        assert!((a - (l + r) / 2.0).abs() < 1e-6);
        // Across the antimeridian it wraps rather than jumping.
        let east = g.reliability_at(7.5, 179.9).unwrap();
        let west = g.reliability_at(7.5, -179.9).unwrap();
        assert!((east - west).abs() < 0.01, "{east} {west}");
        // Past the last row it holds the edge.
        assert!(g.reliability_at(89.9, 0.0).is_some());
    }

    #[test]
    fn finer_grids_blend_too() {
        let mut g = coarse_world();
        g.lat_step = FINE_LAT_STEP;
        assert!(!g.is_coarse());
        // At a centre, its own value; between two, their mean.
        let i = g.index_of(-82.5, 11.25).unwrap();
        assert_eq!(g.reliability_at(-82.5, 11.25), Some(g.reliability[i]));
        let mid = g.reliability_at(-82.5, 0.0).unwrap();
        assert!((mid - (g.reliability[i] + g.reliability[i - 1]) / 2.0).abs() < 1e-6);
        // A regional grid holds its edge instead of wrapping, and has no
        // value outside itself.
        let mut r = g.clone();
        r.nx = 4;
        r.reliability = r.reliability[..r.nx * r.ny].to_vec();
        r.takeoff_deg = r.takeoff_deg[..r.nx * r.ny].to_vec();
        assert!(!r.is_whole_world());
        assert!(r.reliability_at(-82.5, -168.75).is_some());
        assert!(r.reliability_at(-82.5, 100.0).is_none());
        let b = g.cell_bounds(i);
        assert!((b.lat_max - b.lat_min - FINE_LAT_STEP).abs() < 1e-9);
    }

    #[test]
    fn strips_cover_every_row_once() {
        let strips = lat_strips(FINE_LAT_STEP, FINE_LON_STEP, 16).unwrap();
        assert_eq!(strips.len(), 16);
        let rows: usize = strips
            .iter()
            .map(|s| {
                let a = axis_of(s.lat_min, s.lat_max, FINE_LAT_STEP, -90.0, 180.0).unwrap();
                (a.last - a.first + 1) as usize
            })
            .sum();
        assert_eq!(rows, 144);
        assert!(strips.windows(2).all(|w| w[0].lat_max < w[1].lat_min));
    }

    #[test]
    fn patch_is_on_the_world_lattice_and_in_budget() {
        let p = patch_grid(39.47, -0.38, PATCH_HALF_LAT_DEG).unwrap();
        let rows = ((p.bounds.lat_max - p.bounds.lat_min) / p.lat_step).round() as usize + 1;
        let cols = ((p.bounds.lon_max - p.bounds.lon_min) / p.lon_step).round() as usize + 1;
        assert!(rows * cols <= MAX_PATCH_POINTS, "{rows}x{cols}");
        // Cell centres sit at half steps from the world edges.
        let frac = ((p.bounds.lat_min + 90.0) / p.lat_step).fract();
        assert!((frac - 0.5).abs() < 1e-9);
        // The default patch is finer than the whole-world fine grid.
        assert!(p.lat_step < FINE_LAT_STEP);
        // Zoomed out it is held to the same rectangle.
        assert_eq!(patch_grid(39.47, -0.38, 40.0), Some(p));
    }

    #[test]
    fn patch_narrows_at_the_dateline() {
        let b = patch_bounds(0.0, 178.0, 10.0);
        assert!(b.is_none());
        let b = patch_bounds(0.0, 170.0, 10.0).unwrap();
        assert!((b.lon_max - 180.0).abs() < 1e-9 && (b.lon_min - 160.0).abs() < 1e-9);
    }

    #[test]
    fn reach_is_area_weighted() {
        let grid = Grid {
            lat_min: -60.0,
            lon_min: 0.0,
            lat_step: 60.0,
            lon_step: 180.0,
            nx: 2,
            ny: 3,
            reliability: vec![1.0, 1.0, 0.0, 0.0, 0.0, 0.0],
            takeoff_deg: vec![f32::NAN; 6],
        };
        // cos(60) = 0.5 for the southern row, 1 at the equator, 0.5 north.
        assert!((grid.reach() - 0.25).abs() < 1e-9);
        assert_eq!(grid.index_of(-55.0, 10.0), Some(0));
        assert_eq!(grid.index_of(10.0, 200.0), Some(3));
    }
}
