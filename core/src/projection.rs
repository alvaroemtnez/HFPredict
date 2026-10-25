//! The map's projection: azimuthal equidistant centred on the station.
//!
//! Every straight line out from the centre is a great circle at its true
//! bearing and distance, which is exactly what a propagation map is about:
//! where a beam pointed this way lands, and how far. The whole earth fits in
//! a disc whose rim is the antipode.
//!
//! Coordinates here are "unit disc" coordinates: the centre is (0, 0), the
//! antipode is at radius 1, x grows east and y grows down (screen order).

use std::f64::consts::PI;

use crate::geo::{EARTH_RADIUS_KM, LatLon, normalise_lon};

const DEG: f64 = PI / 180.0;

/// Points closer than this to the antipode are not drawn: the projection
/// stretches the antipode into the whole rim, so lines there smear.
pub const CLIP_DEG: f64 = 179.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projection {
    pub centre: LatLon,
    sin_lat0: f64,
    cos_lat0: f64,
}

impl Projection {
    pub fn new(centre: LatLon) -> Projection {
        let lat0 = centre.lat * DEG;
        Projection { centre, sin_lat0: lat0.sin(), cos_lat0: lat0.cos() }
    }

    /// Unit-disc position of a point, `None` near the antipode.
    pub fn project(&self, p: LatLon) -> Option<(f64, f64)> {
        let phi = p.lat * DEG;
        let dlon = (p.lon - self.centre.lon) * DEG;
        let (sin_phi, cos_phi) = phi.sin_cos();
        let cos_dlon = dlon.cos();
        let cos_c = (self.sin_lat0 * sin_phi + self.cos_lat0 * cos_phi * cos_dlon).clamp(-1.0, 1.0);
        let c = cos_c.acos();
        if c > CLIP_DEG * DEG {
            return None;
        }
        let k = if c == 0.0 { 1.0 } else { c / c.sin() };
        let x = k * cos_phi * dlon.sin();
        let y = k * (self.cos_lat0 * sin_phi - self.sin_lat0 * cos_phi * cos_dlon);
        Some((x / PI, -y / PI))
    }

    /// The point at a unit-disc position, `None` outside the disc.
    pub fn invert(&self, x: f64, y: f64) -> Option<LatLon> {
        let (dx, dy) = (x * PI, -y * PI);
        let c = dx.hypot(dy);
        if c > CLIP_DEG * DEG {
            return None;
        }
        if c == 0.0 {
            return Some(self.centre);
        }
        let (sin_c, cos_c) = c.sin_cos();
        let phi = (cos_c * self.sin_lat0 + dy * sin_c * self.cos_lat0 / c).clamp(-1.0, 1.0).asin();
        let lambda = self.centre.lon * DEG + (dx * sin_c).atan2(c * self.cos_lat0 * cos_c - dy * self.sin_lat0 * sin_c);
        Some(LatLon { lat: phi / DEG, lon: normalise_lon(lambda / DEG) })
    }

    /// Unit-disc radius of a distance from the centre.
    pub fn radius_of_km(km: f64) -> f64 {
        km / (PI * EARTH_RADIUS_KM)
    }

    /// Kilometres from the centre at a unit-disc radius.
    pub fn km_of_radius(r: f64) -> f64 {
        r * PI * EARTH_RADIUS_KM
    }
}

/// What part of the disc is on screen: magnification (1 shows the whole
/// earth) and the centre of the window as a fraction of the disc's bounding
/// square (0.5, 0.5 is the station).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct MapView {
    pub scale: f64,
    pub cx: f64,
    pub cy: f64,
}

pub const MIN_SCALE: f64 = 1.0;
pub const MAX_SCALE: f64 = 24.0;

impl Default for MapView {
    fn default() -> Self {
        MapView { scale: 1.0, cx: 0.5, cy: 0.5 }
    }
}

impl MapView {
    /// Kept inside the disc's square, so panning never shows only empty
    /// space.
    pub fn contained(self) -> MapView {
        let scale = self.scale.clamp(MIN_SCALE, MAX_SCALE);
        let half = 1.0 / (2.0 * scale);
        MapView { scale, cx: self.cx.clamp(half, 1.0 - half), cy: self.cy.clamp(half, 1.0 - half) }
    }

    /// Unit-disc position of a pixel in a square of `size` pixels.
    pub fn to_disc(&self, px: f64, py: f64, size: f64) -> (f64, f64) {
        let window = 1.0 / self.scale;
        let fx = self.cx - window / 2.0 + px / size * window;
        let fy = self.cy - window / 2.0 + py / size * window;
        (fx * 2.0 - 1.0, fy * 2.0 - 1.0)
    }

    /// Pixel position of a unit-disc point.
    pub fn to_pixel(&self, x: f64, y: f64, size: f64) -> (f64, f64) {
        let window = 1.0 / self.scale;
        let fx = (x + 1.0) / 2.0;
        let fy = (y + 1.0) / 2.0;
        ((fx - (self.cx - window / 2.0)) / window * size, (fy - (self.cy - window / 2.0)) / window * size)
    }

    /// Zooms by `factor` keeping the pixel at `(px, py)` still, as a pinch
    /// does.
    pub fn zoomed_at(&self, factor: f64, px: f64, py: f64, size: f64) -> MapView {
        let scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        let (fx, fy) = (px / size, py / size);
        let before = (self.cx - 0.5 / self.scale + fx / self.scale, self.cy - 0.5 / self.scale + fy / self.scale);
        MapView { scale, cx: before.0 + 0.5 / scale - fx / scale, cy: before.1 + 0.5 / scale - fy / scale }.contained()
    }

    /// Moves the content by a pixel drag.
    pub fn panned(&self, dx: f64, dy: f64, size: f64) -> MapView {
        MapView { cx: self.cx - dx / size / self.scale, cy: self.cy - dy / size / self.scale, ..*self }.contained()
    }

    /// Centre of the view and half its height in degrees of latitude: what a
    /// zoomed-in patch needs to know. `None` at the whole-earth view.
    pub fn region(&self, projection: &Projection) -> Option<(LatLon, f64)> {
        if self.scale <= MIN_SCALE {
            return None;
        }
        let centre = projection.invert(self.cx * 2.0 - 1.0, self.cy * 2.0 - 1.0)?;
        let half_km = Projection::km_of_radius(1.0 / self.scale);
        Some((centre, half_km / 111.19))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let p = Projection::new(LatLon { lat: 39.47, lon: -0.38 });
        for (lat, lon) in [(51.5, -0.1), (-33.9, 151.2), (35.7, 139.7), (39.47, -0.38), (-80.0, 100.0)] {
            let (x, y) = p.project(LatLon { lat, lon }).unwrap();
            let back = p.invert(x, y).unwrap();
            assert!((back.lat - lat).abs() < 1e-9 && (back.lon - lon).abs() < 1e-9, "{lat},{lon} -> {back:?}");
        }
        // North is up, east is right.
        let (x, y) = p.project(LatLon { lat: 50.0, lon: -0.38 }).unwrap();
        assert!(x.abs() < 1e-12 && y < 0.0);
        let (x, _) = p.project(LatLon { lat: 39.47, lon: 10.0 }).unwrap();
        assert!(x > 0.0);
        // Radius is distance.
        let london = LatLon { lat: 51.5, lon: -0.1 };
        let (x, y) = p.project(london).unwrap();
        let km = Projection::km_of_radius(x.hypot(y));
        assert!((km - crate::geo::distance_km(p.centre, london)).abs() < 1e-6);
    }

    #[test]
    fn view_maths() {
        let v = MapView::default();
        assert_eq!(v.to_disc(50.0, 50.0, 100.0), (0.0, 0.0));
        let z = v.zoomed_at(2.0, 75.0, 50.0, 100.0);
        // The pixel under the fingers stays put.
        let before = v.to_disc(75.0, 50.0, 100.0);
        let after = z.to_disc(75.0, 50.0, 100.0);
        assert!((before.0 - after.0).abs() < 1e-12 && (before.1 - after.1).abs() < 1e-12);
        let (px, py) = z.to_pixel(after.0, after.1, 100.0);
        assert!((px - 75.0).abs() < 1e-9 && (py - 50.0).abs() < 1e-9);
        assert_eq!(MapView { scale: 1.0, cx: 0.9, cy: 0.1 }.contained(), MapView::default());
    }
}
