//! Legal local time at a place, worked out on the device: the time zone
//! from the coordinates (tzf), its offset and summer time from the tz
//! database (jiff, bundled). Out at sea, where no zone applies, the
//! nautical zone (an hour per 15 degrees of longitude) stands.

use std::sync::OnceLock;

use jiff::Timestamp;
use jiff::tz::TimeZone;

fn finder() -> &'static tzf_rs::DefaultFinder {
    static FINDER: OnceLock<tzf_rs::DefaultFinder> = OnceLock::new();
    FINDER.get_or_init(tzf_rs::DefaultFinder::new)
}

/// The IANA zone at a place ("Europe/Madrid"), or `None` at sea.
pub fn zone_name(lat: f64, lon: f64) -> Option<String> {
    let name = finder().get_tz_name(lon, lat);
    (!name.is_empty() && !name.starts_with("Etc/")).then(|| name.to_string())
}

/// Seconds to add to UTC for the local time at a place and moment.
pub fn utc_offset_secs(lat: f64, lon: f64, unix: i64) -> i32 {
    let nautical = ((lon / 15.0).round() as i32) * 3600;
    let Some(name) = zone_name(lat, lon) else {
        return nautical;
    };
    let (Ok(tz), Ok(t)) = (jiff::tz::db().get(&name), Timestamp::from_second(unix)) else {
        return nautical;
    };
    offset_of(&tz, t)
}

fn offset_of(tz: &TimeZone, t: Timestamp) -> i32 {
    tz.to_offset(t).seconds()
}

/// The local clock at a place and moment, "HH:MM".
pub fn clock(lat: f64, lon: f64, unix: i64) -> String {
    let s = (unix + i64::from(utc_offset_secs(lat, lon, unix))).rem_euclid(86_400);
    format!("{:02}:{:02}", s / 3600, (s % 3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-01-15 12:00 and 2026-07-15 12:00 UTC.
    const WINTER: i64 = 1_768_478_400;
    const SUMMER: i64 = 1_784_116_800;

    #[test]
    fn zones_and_summer_time() {
        assert_eq!(zone_name(39.47, -0.38).as_deref(), Some("Europe/Madrid"));
        assert_eq!(utc_offset_secs(39.47, -0.38, WINTER), 3600);
        assert_eq!(utc_offset_secs(39.47, -0.38, SUMMER), 7200);
        // Valencia is on Madrid's clock, an hour ahead of its sun.
        assert_eq!(clock(39.47, -0.38, WINTER), "13:00");
        // India keeps half hours; Tokyo has no summer time.
        assert_eq!(clock(28.61, 77.21, WINTER), "17:30");
        assert_eq!(utc_offset_secs(35.69, 139.69, SUMMER), 9 * 3600);
        // Southern summer: Sydney is ahead by eleven hours in January.
        assert_eq!(utc_offset_secs(-33.87, 151.21, WINTER), 11 * 3600);
    }

    #[test]
    fn open_sea_keeps_nautical_time() {
        // Mid-Atlantic, 30 degrees west: two hours behind.
        assert_eq!(utc_offset_secs(0.0, -30.0, WINTER), -2 * 3600);
    }
}
