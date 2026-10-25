//! Maidenhead locators, great-circle maths and the sun. All pure.

use std::f64::consts::PI;

/// The IUGG mean earth radius, km.
pub const EARTH_RADIUS_KM: f64 = 6371.0088;

const DEG: f64 = PI / 180.0;

/// Latitude and longitude in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

/// Whether this text is a Maidenhead locator of 4, 6, 8 or 10 characters.
pub fn is_grid(text: &str) -> bool {
    grid_to_lat_lon(text).is_some()
}

/// A locator to the centre of the rectangle it names. Accepts 4 to 10
/// characters (field, square, subsquare, extended square, extended
/// subsquare), case-insensitive.
pub fn grid_to_lat_lon(text: &str) -> Option<LatLon> {
    let g: Vec<u8> = text.trim().to_ascii_uppercase().into_bytes();
    if !matches!(g.len(), 4 | 6 | 8 | 10) {
        return None;
    }
    let mut lon = -180.0;
    let mut lat = -90.0;
    let mut lon_step = 360.0;
    let mut lat_step = 180.0;
    for (pair, chunk) in g.chunks(2).enumerate() {
        // Pairs alternate between letters and digits; the first is A-R
        // (18 fields), the later letter pairs A-X (24 subdivisions).
        let (base, count) = match pair {
            0 => (b'A', 18.0),
            p if p % 2 == 1 => (b'0', 10.0),
            _ => (b'A', 24.0),
        };
        let a = f64::from(chunk[0].checked_sub(base)?);
        let b = f64::from(chunk[1].checked_sub(base)?);
        if a >= count || b >= count {
            return None;
        }
        lon_step /= count;
        lat_step /= count;
        lon += a * lon_step;
        lat += b * lat_step;
    }
    Some(LatLon { lat: lat + lat_step / 2.0, lon: lon + lon_step / 2.0 })
}

/// Latitude and longitude to a 6-character locator.
pub fn lat_lon_to_grid(lat: f64, lon: f64) -> String {
    let lon = lon.clamp(-180.0, 179.99999) + 180.0;
    let lat = lat.clamp(-90.0, 89.99999) + 90.0;
    let chars = [
        b'A' + (lon / 20.0) as u8,
        b'A' + (lat / 10.0) as u8,
        b'0' + ((lon % 20.0) / 2.0) as u8,
        b'0' + (lat % 10.0) as u8,
        b'a' + (((lon % 2.0) / 2.0) * 24.0) as u8,
        b'a' + ((lat % 1.0) * 24.0) as u8,
    ];
    // Subsquare in lower case, the usual way amateurs write it.
    String::from_utf8(chars.to_vec()).unwrap_or_default()
}

/// Great-circle distance, km.
pub fn distance_km(a: LatLon, b: LatLon) -> f64 {
    angular_distance_deg(a, b) * DEG * EARTH_RADIUS_KM
}

/// Central angle between two points, degrees.
pub fn angular_distance_deg(a: LatLon, b: LatLon) -> f64 {
    let (p1, p2) = (a.lat * DEG, b.lat * DEG);
    let dp = p2 - p1;
    let dl = (b.lon - a.lon) * DEG;
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * h.sqrt().min(1.0).asin() / DEG
}

/// Initial great-circle bearing, 0-360 clockwise from north.
pub fn bearing_deg(a: LatLon, b: LatLon) -> f64 {
    let (p1, p2) = (a.lat * DEG, b.lat * DEG);
    let dl = (b.lon - a.lon) * DEG;
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (y.atan2(x) / DEG).rem_euclid(360.0)
}

/// Where you are after `km` along `bearing` from `from`.
pub fn point_from(from: LatLon, bearing: f64, km: f64) -> LatLon {
    let d = km / EARTH_RADIUS_KM;
    let (lat1, lon1, theta) = (from.lat * DEG, from.lon * DEG, bearing * DEG);
    let sin_lat2 = (lat1.sin() * d.cos() + lat1.cos() * d.sin() * theta.cos()).clamp(-1.0, 1.0);
    let lat2 = sin_lat2.asin();
    let lon2 = lon1 + (theta.sin() * d.sin() * lat1.cos()).atan2(d.cos() - lat1.sin() * sin_lat2);
    LatLon { lat: lat2 / DEG, lon: normalise_lon(lon2 / DEG) }
}

/// Into -180..180.
pub fn normalise_lon(lon: f64) -> f64 {
    (lon + 540.0).rem_euclid(360.0) - 180.0
}

/// Points along the great circle from `a` to `b`, inclusive.
pub fn great_circle(a: LatLon, b: LatLon, steps: usize) -> Vec<LatLon> {
    let d = angular_distance_deg(a, b) * DEG;
    if d == 0.0 || steps == 0 {
        return vec![a];
    }
    let (p1, l1, p2, l2) = (a.lat * DEG, a.lon * DEG, b.lat * DEG, b.lon * DEG);
    (0..=steps)
        .map(|i| {
            let f = i as f64 / steps as f64;
            let s = ((1.0 - f) * d).sin() / d.sin();
            let t = (f * d).sin() / d.sin();
            let x = s * p1.cos() * l1.cos() + t * p2.cos() * l2.cos();
            let y = s * p1.cos() * l1.sin() + t * p2.cos() * l2.sin();
            let z = s * p1.sin() + t * p2.sin();
            LatLon { lat: z.atan2((x * x + y * y).sqrt()) / DEG, lon: y.atan2(x) / DEG }
        })
        .collect()
}

/// The point on earth with the sun overhead at `unix_secs`. Accurate to
/// about a degree, which is ample for a night wash.
pub fn subsolar_point(unix_secs: i64) -> LatLon {
    let days = unix_secs as f64 / 86_400.0;
    let (year_start, _) = year_start_days(unix_secs);
    let day_of_year = days - year_start as f64;
    let declination = -23.44 * (2.0 * PI / 365.0 * (day_of_year + 10.0)).cos();
    let hours = (unix_secs.rem_euclid(86_400)) as f64 / 3600.0;
    LatLon { lat: declination, lon: normalise_lon(-15.0 * (hours - 12.0)) }
}

/// Whether the sun is below the horizon at a point.
pub fn is_night(point: LatLon, unix_secs: i64) -> bool {
    angular_distance_deg(point, subsolar_point(unix_secs)) > 90.0
}

/// Days since the epoch of 1 January of the year containing `unix_secs`, and
/// that year.
fn year_start_days(unix_secs: i64) -> (i64, i32) {
    let (year, _, _) = civil_from_days(unix_secs.div_euclid(86_400));
    (days_from_civil(year, 1, 1), year)
}

/// A UTC calendar date, from days since 1970-01-01 (Howard Hinnant's
/// algorithm, valid for the proleptic Gregorian calendar).
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

/// Days since 1970-01-01 of a UTC calendar date.
pub fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The UTC calendar date and hour of a moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UtcDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
}

impl UtcDate {
    pub fn of(unix_secs: i64) -> UtcDate {
        let (year, month, day) = civil_from_days(unix_secs.div_euclid(86_400));
        UtcDate { year, month, day, hour: (unix_secs.rem_euclid(86_400) / 3600) as u32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn grid_round_trip() {
        let p = grid_to_lat_lon("IM99sl").unwrap();
        assert!(close(p.lat, 39.479, 0.01) && close(p.lon, -0.458, 0.01), "{p:?}");
        assert_eq!(lat_lon_to_grid(p.lat, p.lon), "IM99sl");
        let four = grid_to_lat_lon("JO01").unwrap();
        assert_eq!((four.lat, four.lon), (51.5, 1.0));
        assert!(grid_to_lat_lon("IM99sl12").is_some());
        assert!(grid_to_lat_lon("ZZ00").is_none());
        assert!(grid_to_lat_lon("IM9").is_none());
    }

    #[test]
    fn distance_and_bearing() {
        let valencia = LatLon { lat: 39.47, lon: -0.38 };
        let london = LatLon { lat: 51.5, lon: -0.1 };
        assert!(close(distance_km(valencia, london), 1338.0, 5.0));
        assert!(close(bearing_deg(valencia, london), 0.9, 0.5));
        let p = point_from(valencia, 90.0, 1000.0);
        assert!(close(distance_km(valencia, p), 1000.0, 0.01));
    }

    #[test]
    fn calendar_round_trip() {
        for days in [-1000, 0, 19_000, 20_752] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(civil_from_days(20_751), (2026, 10, 25));
    }

    #[test]
    fn noon_sun_is_overhead_near_greenwich() {
        // 2026-06-21 12:00 UTC
        let t = days_from_civil(2026, 6, 21) * 86_400 + 12 * 3600;
        let s = subsolar_point(t);
        assert!(close(s.lat, 23.4, 0.5) && close(s.lon, 0.0, 0.01), "{s:?}");
        assert!(!is_night(LatLon { lat: 40.0, lon: 0.0 }, t));
        assert!(is_night(LatLon { lat: 40.0, lon: 180.0 }, t));
    }
}
