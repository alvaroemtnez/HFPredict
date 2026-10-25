//! The coverage map, drawn to a bitmap.
//!
//! Both apps show this image and draw nothing on the map themselves, so the
//! map is the same picture on a phone and a desktop. The colours come from
//! the platform's theme ([`Palette`]); gestures stay native and only change
//! the [`MapView`].
//!
//! Cells are found per pixel through the inverse projection, so a cell is
//! drawn with its true, warped outline at any zoom and the pixel cost does
//! not depend on how many cells there are. Lines (coastlines, range rings,
//! the path) are antialiased vectors on top.

use std::sync::OnceLock;

use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Stroke,
    StrokeDash, Transform,
};

use crate::coverage::Grid;
use crate::geo::{self, LatLon};
use crate::projection::{MapView, Projection};
use crate::quality::{NVIS_MIN_ANGLE_DEG, Quality};

/// Colours as 0xAARRGGBB, straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Palette {
    /// The earth's disc, under everything.
    pub disc: u32,
    pub reliable: u32,
    pub patchy: u32,
    pub weak: u32,
    /// Laid over the night side.
    pub night: u32,
    pub coastline: u32,
    /// Distance rings and the disc's rim.
    pub grid_lines: u32,
    /// The great circle to the destination, its end and the selected cell.
    pub path: u32,
    /// The outline that keeps the path and the selected cell readable on
    /// any shade underneath.
    pub path_halo: u32,
    /// The station's own dot.
    pub station: u32,
    /// The ring round the dots, so they stand off any colour.
    pub halo: u32,
    /// The near-vertical-incidence dots.
    pub nvis: u32,
}

/// What the map shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene<'a> {
    pub centre: LatLon,
    pub destination: Option<LatLon>,
    /// When the night wash is for.
    pub unix_secs: i64,
    /// Drawn in order; a later grid covers an earlier one where it has
    /// cells (coarse, then fine, then a zoom patch).
    pub layers: Vec<&'a Grid>,
    pub show_night: bool,
    pub show_nvis: bool,
    /// The point the reader tapped: its cell is outlined.
    pub highlight: Option<LatLon>,
}

/// The highlight colours ([`Palette::path`] and [`Palette::path_halo`]).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Highlight {
    pub line: u32,
    pub halo: u32,
}

/// The path and the selected cell are yellow, outlined in black: a colour
/// no platform accent's reach shades share. Under a yellow or orange
/// accent the reach itself is yellow, so they turn magenta; in low light
/// they stay in the dim reds.
pub fn highlight(accent: u32, dark: bool, low_light: bool) -> Highlight {
    if low_light {
        return Highlight {
            line: 0xFFFF6A50,
            halo: 0xFF000000,
        };
    }
    let [r, g, b, _] = argb(accent).map(|c| f32::from(c) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let hue = if max - min < 1e-6 {
        None
    } else if max == r {
        Some(60.0 * ((g - b) / (max - min)).rem_euclid(6.0))
    } else if max == g {
        Some(60.0 * ((b - r) / (max - min) + 2.0))
    } else {
        Some(60.0 * ((r - g) / (max - min) + 4.0))
    };
    let saturated = max - min > 0.15;
    let yellowish = saturated && hue.is_some_and(|h| (22.0..=80.0).contains(&h));
    let line = match (yellowish, dark) {
        (false, true) => 0xFFF6D32D,
        (false, false) => 0xFFF5C211,
        (true, true) => 0xFFF279F2,
        (true, false) => 0xFFC01CC0,
    };
    Highlight {
        line,
        halo: if dark { 0xE0000000 } else { 0xC0202020 },
    }
}

/// Distance rings every 5,000 km.
pub const RING_KM: f64 = 5000.0;

fn argb(c: u32) -> [u8; 4] {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8]
}

fn premultiply([r, g, b, a]: [u8; 4]) -> [f32; 4] {
    let af = f32::from(a) / 255.0;
    [
        f32::from(r) * af,
        f32::from(g) * af,
        f32::from(b) * af,
        f32::from(a),
    ]
}

/// `top` over `under`, both premultiplied, 0..255.
fn over(top: [f32; 4], under: [f32; 4]) -> [f32; 4] {
    let k = 1.0 - top[3] / 255.0;
    [
        top[0] + under[0] * k,
        top[1] + under[1] * k,
        top[2] + under[2] * k,
        top[3] + under[3] * k,
    ]
}

fn paint(c: u32) -> Paint<'static> {
    let [r, g, b, a] = argb(c);
    let mut p = Paint::default();
    p.set_color(Color::from_rgba8(r, g, b, a));
    p.anti_alias = true;
    p
}

/// Draws the map into a `size` by `size` square of premultiplied RGBA, the
/// byte order Android's `ARGB_8888` and GDK's `R8g8b8a8Premultiplied` take.
/// `density` scales line widths (device pixels per dp / logical pixel).
pub fn render(
    scene: &Scene,
    view: MapView,
    size: u32,
    density: f32,
    palette: &Palette,
    threads: usize,
) -> Vec<u8> {
    let size = size.max(16);
    let Some(mut pixmap) = Pixmap::new(size, size) else {
        return vec![];
    };
    let projection = Projection::new(scene.centre);
    let s = f64::from(size);
    let (ox, oy) = view.to_pixel(0.0, 0.0, s);
    let radius = (view.to_pixel(1.0, 0.0, s).0 - ox) as f32;

    // The disc, antialiased at its rim.
    if let Some(circle) = PathBuilder::from_circle(ox as f32, oy as f32, radius) {
        pixmap.fill_path(
            &circle,
            &paint(palette.disc),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    raster_cells(
        &mut pixmap,
        scene,
        &projection,
        view,
        palette,
        threads,
        radius,
    );

    let line = density.max(0.5);
    let stroke = |width: f32| Stroke {
        width: width * line,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };

    // Distance rings, dashed, and the rim.
    let mut ring_km = RING_KM;
    while Projection::radius_of_km(ring_km) < 0.999 {
        let r = radius * Projection::radius_of_km(ring_km) as f32;
        if let Some(c) = PathBuilder::from_circle(ox as f32, oy as f32, r) {
            let mut st = stroke(1.0);
            st.dash = StrokeDash::new(vec![4.0 * line, 4.0 * line], 0.0);
            pixmap.stroke_path(
                &c,
                &paint(palette.grid_lines),
                &st,
                Transform::identity(),
                None,
            );
        }
        ring_km += RING_KM;
    }
    if let Some(c) = PathBuilder::from_circle(ox as f32, oy as f32, radius) {
        pixmap.stroke_path(
            &c,
            &paint(palette.grid_lines),
            &stroke(1.0),
            Transform::identity(),
            None,
        );
    }

    // Coastlines.
    let mut coast = PathBuilder::new();
    for ring in land() {
        add_polyline(
            &mut coast,
            &projection,
            view,
            s,
            ring.iter().map(|&(lon, lat)| LatLon {
                lat: f64::from(lat),
                lon: f64::from(lon),
            }),
        );
    }
    if let Some(path) = coast.finish() {
        pixmap.stroke_path(
            &path,
            &paint(palette.coastline),
            &stroke(0.9),
            Transform::identity(),
            None,
        );
    }

    // The selected cell, outlined: the finest grid's cell under the point,
    // or a ring where only the smoothed preview is drawn.
    if let Some(h) = scene.highlight {
        let mut pb = PathBuilder::new();
        let grid = scene
            .layers
            .iter()
            .rev()
            .find_map(|g| g.index_of(h.lat, h.lon).map(|i| (g, i)));
        match grid {
            Some((g, i)) if !g.is_coarse() => {
                let b = g.cell_bounds(i);
                let n = 16;
                let along = |a: f64, z: f64, k: usize| a + (z - a) * k as f64 / n as f64;
                let outline = (0..n)
                    .map(|k| (b.lat_min, along(b.lon_min, b.lon_max, k)))
                    .chain((0..n).map(|k| (along(b.lat_min, b.lat_max, k), b.lon_max)))
                    .chain((0..n).map(|k| (b.lat_max, along(b.lon_max, b.lon_min, k))))
                    .chain((0..=n).map(|k| (along(b.lat_max, b.lat_min, k), b.lon_min)))
                    .map(|(lat, lon)| LatLon { lat, lon });
                add_polyline(&mut pb, &projection, view, s, outline);
                // A cell a few pixels wide at this zoom gets a ring round
                // it too, so the eye finds it.
                let corners: Vec<(f64, f64)> = [
                    (b.lat_min, b.lon_min),
                    (b.lat_max, b.lon_max),
                    (b.lat_min, b.lon_max),
                    (b.lat_max, b.lon_min),
                ]
                .iter()
                .filter_map(|&(lat, lon)| projection.project(LatLon { lat, lon }))
                .map(|(x, y)| view.to_pixel(x, y, s))
                .collect();
                let extent = corners
                    .iter()
                    .flat_map(|a| corners.iter().map(move |b| (a.0 - b.0).hypot(a.1 - b.1)))
                    .fold(0.0, f64::max) as f32;
                if extent < 16.0 * line
                    && let Some((x, y)) = projection.project(h)
                {
                    let (px, py) = view.to_pixel(x, y, s);
                    pb.push_circle(px as f32, py as f32, 11.0 * line);
                }
            }
            _ => {
                if let Some((x, y)) = projection.project(h) {
                    let (px, py) = view.to_pixel(x, y, s);
                    pb.push_circle(px as f32, py as f32, 7.0 * line);
                }
            }
        }
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(
                &path,
                &paint(palette.path_halo),
                &stroke(4.5),
                Transform::identity(),
                None,
            );
            pixmap.stroke_path(
                &path,
                &paint(palette.path),
                &stroke(2.5),
                Transform::identity(),
                None,
            );
        }
    }

    // The path to the destination and its end.
    let dot = |pixmap: &mut Pixmap, p: LatLon, fill: u32, halo: u32, r: f32| {
        if let Some((x, y)) = projection.project(p) {
            let (px, py) = view.to_pixel(x, y, s);
            if let Some(c) = PathBuilder::from_circle(px as f32, py as f32, (r + 1.5) * line) {
                pixmap.fill_path(
                    &c,
                    &paint(halo),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
            if let Some(c) = PathBuilder::from_circle(px as f32, py as f32, r * line) {
                pixmap.fill_path(
                    &c,
                    &paint(fill),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
    };
    if let Some(to) = scene.destination {
        let mut pb = PathBuilder::new();
        add_polyline(
            &mut pb,
            &projection,
            view,
            s,
            geo::great_circle(scene.centre, to, 128).into_iter(),
        );
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(
                &path,
                &paint(palette.path_halo),
                &stroke(4.5),
                Transform::identity(),
                None,
            );
            pixmap.stroke_path(
                &path,
                &paint(palette.path),
                &stroke(2.5),
                Transform::identity(),
                None,
            );
        }
        dot(&mut pixmap, to, palette.path, palette.path_halo, 4.5);
    }
    dot(
        &mut pixmap,
        scene.centre,
        palette.station,
        palette.halo,
        5.5,
    );

    pixmap.take()
}

/// Coverage cells and the night wash, pixel by pixel inside the disc.
fn raster_cells(
    pixmap: &mut Pixmap,
    scene: &Scene,
    projection: &Projection,
    view: MapView,
    palette: &Palette,
    threads: usize,
    radius: f32,
) {
    let size = pixmap.width() as usize;
    let s = size as f64;
    let colours = [
        [0.0; 4],
        premultiply(argb(palette.weak)),
        premultiply(argb(palette.patchy)),
        premultiply(argb(palette.reliable)),
    ];
    let night = premultiply(argb(palette.night));
    let nvis = premultiply(argb(palette.nvis));
    let sun = geo::subsolar_point(scene.unix_secs);
    let sun_v = unit_vector(sun);
    let (ox, oy) = view.to_pixel(0.0, 0.0, s);
    // A pixel's NVIS dot pattern: one dot every `pitch` pixels.
    let pitch = (6.0 * (radius / 300.0).clamp(0.6, 1.6)).round().max(4.0) as usize;

    let rows: Vec<usize> = (0..size).collect();
    let chunks: Vec<&[usize]> = rows
        .chunks(size.div_ceil(threads.max(1) * 4).max(1))
        .collect();
    let bands: Vec<Vec<(usize, Vec<[f32; 4]>)>> = crate::parallel::map(&chunks, threads, |chunk| {
        chunk
            .iter()
            .map(|&py| {
                let mut row = vec![[0.0f32; 4]; size];
                let dy = py as f64 + 0.5 - oy;
                for (px, out) in row.iter_mut().enumerate() {
                    let dx = px as f64 + 0.5 - ox;
                    // Only strictly inside the rim; the rim pixels keep the
                    // antialiased disc edge.
                    if dx.hypot(dy) > f64::from(radius) - 1.0 {
                        continue;
                    }
                    let (x, y) = view.to_disc(px as f64 + 0.5, py as f64 + 0.5, s);
                    let Some(p) = projection.invert(x, y) else {
                        continue;
                    };
                    let mut colour = [0.0f32; 4];
                    let mut steep = false;
                    for grid in &scene.layers {
                        if let Some(i) = grid.index_of(p.lat, p.lon) {
                            let r = f64::from(grid.reliability_at(p.lat, p.lon).unwrap_or(0.0));
                            let q = Quality::of(r);
                            colour = colours[q as usize];
                            let angle = grid.takeoff_deg[i];
                            steep = q != Quality::Closed
                                && !angle.is_nan()
                                && f64::from(angle) >= NVIS_MIN_ANGLE_DEG;
                        }
                    }
                    if scene.show_nvis
                        && steep
                        && px % pitch == pitch / 2
                        && py % pitch == pitch / 2
                    {
                        colour = over(nvis, colour);
                    }
                    if scene.show_night {
                        // A soft terminator over about six degrees of twilight.
                        let d = dot(unit_vector(p), sun_v);
                        let t = ((0.05 - d) / 0.1).clamp(0.0, 1.0) as f32;
                        if t > 0.0 {
                            let wash = [night[0] * t, night[1] * t, night[2] * t, night[3] * t];
                            colour = over(wash, colour);
                        }
                    }
                    *out = colour;
                }
                (py, row)
            })
            .collect()
    });

    let width = size;
    let pixels = pixmap.pixels_mut();
    for (py, row) in bands.into_iter().flatten() {
        for (px, c) in row.into_iter().enumerate() {
            if c[3] <= 0.0 {
                continue;
            }
            let under = pixels[py * width + px];
            let u = [
                f32::from(under.red()),
                f32::from(under.green()),
                f32::from(under.blue()),
                f32::from(under.alpha()),
            ];
            let [r, g, b, a] = over(c, u);
            let a8 = a.round().clamp(0.0, 255.0) as u8;
            let ch = |v: f32| (v.round().clamp(0.0, 255.0) as u8).min(a8);
            if let Some(p) = PremultipliedColorU8::from_rgba(ch(r), ch(g), ch(b), a8) {
                pixels[py * width + px] = p;
            }
        }
    }
}

fn unit_vector(p: LatLon) -> [f64; 3] {
    let (lat, lon) = (p.lat.to_radians(), p.lon.to_radians());
    [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Adds a projected polyline, broken wherever it leaves the drawable disc
/// or jumps across it (a line wrapping round the antipode).
fn add_polyline(
    pb: &mut PathBuilder,
    projection: &Projection,
    view: MapView,
    size: f64,
    points: impl Iterator<Item = LatLon>,
) {
    let mut last: Option<(f64, f64)> = None;
    for p in points {
        match projection.project(p) {
            None => last = None,
            Some((x, y)) => {
                let jump = last.is_some_and(|(lx, ly)| (x - lx).hypot(y - ly) > 0.08);
                let (px, py) = view.to_pixel(x, y, size);
                if last.is_none() || jump {
                    pb.move_to(px as f32, py as f32);
                } else {
                    pb.line_to(px as f32, py as f32);
                }
                last = Some((x, y));
            }
        }
    }
}

/// Natural Earth's 1:110m land outlines, as rings of (lon, lat).
pub fn land() -> &'static [Vec<(f32, f32)>] {
    static LAND: OnceLock<Vec<Vec<(f32, f32)>>> = OnceLock::new();
    LAND.get_or_init(|| {
        serde_json::from_str(include_str!("../data/land.json")).expect("data/land.json is valid")
    })
}

/// The view that frames everywhere a grid reaches (at least `Patchy`),
/// with a margin; the whole earth when nothing does.
pub fn fit_reach(centre: LatLon, grid: &Grid) -> MapView {
    let projection = Projection::new(centre);
    let mut extent: f64 = 0.0;
    for row in 0..grid.ny {
        for column in 0..grid.nx {
            if Quality::of(f64::from(grid.reliability[row * grid.nx + column])) >= Quality::Patchy {
                let p = LatLon {
                    lat: grid.lat_at(row),
                    lon: grid.lon_at(column),
                };
                // The far corner of the cell, so the whole cell is in frame.
                let half = (grid.lat_step.max(grid.lon_step * p.lat.to_radians().cos())) / 2.0;
                let r = match projection.project(p) {
                    Some((x, y)) => x.hypot(y) + half / 180.0,
                    None => 1.0,
                };
                extent = extent.max(r);
            }
        }
    }
    if extent <= 0.0 || extent >= 0.9 {
        return MapView::default();
    }
    MapView {
        scale: 1.0 / (extent * 1.12).max(0.05),
        cx: 0.5,
        cy: 0.5,
    }
    .contained()
}

/// The furthest point still working by near-vertical incidence, km.
pub fn nvis_reach_km(centre: LatLon, grid: &Grid) -> Option<f64> {
    let mut furthest: Option<f64> = None;
    for row in 0..grid.ny {
        for column in 0..grid.nx {
            let i = row * grid.nx + column;
            let angle = grid.takeoff_deg[i];
            let angle = (!angle.is_nan()).then_some(f64::from(angle));
            if crate::quality::is_nvis(angle, f64::from(grid.reliability[i])) {
                let km = geo::distance_km(
                    centre,
                    LatLon {
                        lat: grid.lat_at(row),
                        lon: grid.lon_at(column),
                    },
                );
                furthest = Some(furthest.map_or(km, |f: f64| f.max(km)));
            }
        }
    }
    furthest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        Palette {
            disc: 0xFF101418,
            reliable: 0xFFD0BCFF,
            patchy: 0xFF9A82DB,
            weak: 0xFF4F378B,
            night: 0x66000000,
            coastline: 0xCCFFFFFF,
            grid_lines: 0x55FFFFFF,
            path: 0xFF00DDFF,
            path_halo: 0xFF000000,
            station: 0xFFFFC107,
            halo: 0xFF000000,
            nvis: 0xFFFFFFFF,
        }
    }

    fn world(reliability: f32) -> Grid {
        Grid {
            lat_min: -82.5,
            lon_min: -168.75,
            lat_step: 15.0,
            lon_step: 22.5,
            nx: 16,
            ny: 12,
            reliability: vec![reliability; 192],
            takeoff_deg: vec![f32::NAN; 192],
        }
    }

    #[test]
    fn draws_a_disc_with_cells() {
        let grid = world(0.9);
        let scene = Scene {
            centre: LatLon {
                lat: 39.47,
                lon: -0.38,
            },
            destination: Some(LatLon {
                lat: 35.7,
                lon: 139.7,
            }),
            unix_secs: 1_793_000_000,
            layers: vec![&grid],
            show_night: false,
            show_nvis: false,
            highlight: None,
        };
        let size = 200;
        let px = render(&scene, MapView::default(), size, 1.0, &palette(), 4);
        assert_eq!(px.len(), (size * size * 4) as usize);
        // A corner is outside the disc: transparent.
        assert_eq!(&px[0..4], &[0, 0, 0, 0]);
        // Somewhere between the centre and the rim, off the lines: the
        // reliable colour.
        let at = |x: usize, y: usize| &px[(y * size as usize + x) * 4..][..4];
        assert_eq!(at(150, 60), &[0xD0, 0xBC, 0xFF, 0xFF]);
    }

    #[test]
    fn highlight_avoids_a_yellow_accent() {
        let blue = highlight(0xFF3584E4, true, false);
        let yellow = highlight(0xFFC88800, true, false);
        let orange = highlight(0xFFED5B00, false, false);
        assert_eq!(blue.line, 0xFFF6D32D);
        assert_ne!(yellow.line, blue.line);
        assert_ne!(orange.line, highlight(0xFF3584E4, false, false).line);
        // A grey accent has no hue to clash with.
        assert_eq!(highlight(0xFF6F8396, true, false).line, 0xFFF6D32D);
        assert_eq!(highlight(0xFF3584E4, true, true).line, 0xFFFF6A50);
    }

    #[test]
    fn outlines_the_selected_cell() {
        let mut grid = world(0.0);
        grid.lat_step = 7.5;
        grid.lon_step = 11.25;
        grid.lat_min = -86.25;
        grid.lon_min = -174.375;
        grid.nx = 32;
        grid.ny = 24;
        grid.reliability = vec![0.0; 32 * 24];
        grid.takeoff_deg = vec![f32::NAN; 32 * 24];
        let centre = LatLon { lat: 0.0, lon: 0.0 };
        let mut scene = Scene {
            centre,
            destination: None,
            unix_secs: 1_793_000_000,
            layers: vec![&grid],
            show_night: false,
            show_nvis: false,
            highlight: None,
        };
        let plain = render(&scene, MapView::default(), 600, 1.0, &palette(), 2);
        scene.highlight = Some(LatLon {
            lat: 30.0,
            lon: 30.0,
        });
        let marked = render(&scene, MapView::default(), 600, 1.0, &palette(), 2);
        let cyan = marked
            .chunks(4)
            .zip(plain.chunks(4))
            .filter(|(m, p)| m != p && m[1] > 0xC0 && m[2] > 0xC0 && m[0] < 0x40)
            .count();
        assert!(cyan > 40, "{cyan} highlight pixels");
    }

    #[test]
    fn fits_reach() {
        let mut grid = world(0.0);
        assert_eq!(
            fit_reach(LatLon { lat: 0.0, lon: 0.0 }, &grid),
            MapView::default()
        );
        // Only the cell under the station reaches: zoom right in.
        let i = grid.index_of(0.0, 0.0).unwrap();
        grid.reliability[i] = 0.9;
        let v = fit_reach(
            LatLon {
                lat: 7.5,
                lon: 11.25,
            },
            &grid,
        );
        assert!(v.scale > 2.0, "{v:?}");
    }
}
