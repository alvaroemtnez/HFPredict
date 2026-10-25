//! The three optional online sources, and the parsing that needs no network.
//!
//! - NOAA SWPC: solar flux and planetary K index, turned into an effective
//!   sunspot number for a now-cast.
//! - GIRO: the latest measured foF2 from a nearby ionosonde, shown beside
//!   the forecast (never fed into it).
//! - Open-Meteo: place search beyond the built-in list.
//!
//! Each request names nothing about the operator except what a place search
//! types. Timeouts are short: a hilltop with one bar of signal should fall
//! back to the offline forecast quickly, not hang.

use serde::{Deserialize, Serialize};

use crate::geo::{self, LatLon};
use crate::places::Place;

/// Current conditions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct SpaceWeather {
    /// 10.7 cm solar radio flux, solar flux units.
    pub f107: f64,
    /// Planetary K index, 0-9.
    pub kp: f64,
    /// Highest Kp over about the last 24 hours (storm effects outlast the
    /// disturbance).
    pub kp_max_24h: f64,
    /// SSN derived from the flux and Kp, which drives a now-cast.
    pub effective_ssn: f64,
    /// When the flux was measured, seconds since the epoch.
    pub observed_at: i64,
    /// When the app fetched it, seconds since the epoch.
    pub fetched_at: i64,
}

/// A reading older than this is not a now-cast: its Kp describes a window
/// that has gone.
pub const NOWCAST_GOOD_FOR_SECS: i64 = 24 * 3600;

impl SpaceWeather {
    pub fn is_fresh(&self, now: i64) -> bool {
        now - self.observed_at <= NOWCAST_GOOD_FOR_SECS
    }
}

/// A measured foF2 from a real ionosonde near the station.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Sounding {
    pub station: String,
    pub ursi: String,
    /// Distance from the station asked about, km.
    pub km: f64,
    /// F2 critical frequency, MHz.
    pub fof2: f64,
    pub measured_at: i64,
    /// Autoscaling confidence, 0-100.
    pub confidence: f64,
}

/// SSN from the 10.7 cm flux: the inverse of the standard quadratic
/// F10.7 = 63.7 + 0.728 SSN + 0.00089 SSN².
pub fn ssn_from_f107(f107: f64) -> f64 {
    let (a, b, c) = (0.00089, 0.728, 63.7 - f107);
    let d = b * b - 4.0 * a * c;
    if d <= 0.0 {
        0.0
    } else {
        ((-b + d.sqrt()) / (2.0 * a)).max(0.0)
    }
}

/// A disturbed ionosphere supports less than the flux suggests.
pub fn kp_derate(ssn: f64, kp: f64) -> f64 {
    if kp <= 4.0 {
        return ssn;
    }
    (ssn * (1.0 - ((kp - 4.0) * 0.08).min(0.5))).max(0.0)
}

#[derive(Deserialize)]
struct FluxRecord {
    time_tag: String,
    flux: f64,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct KpRecord {
    Kp: f64,
}

/// Parses NOAA's two feeds (`json/f107_cm_flux.json`, newest first, and
/// `products/noaa-planetary-k-index.json`, oldest first).
pub fn parse_space_weather(
    flux_json: &str,
    kp_json: &str,
    fetched_at: i64,
) -> Option<SpaceWeather> {
    let flux: Vec<FluxRecord> = serde_json::from_str(flux_json).ok()?;
    let kp: Vec<KpRecord> = serde_json::from_str(kp_json).ok()?;
    let latest = flux.first()?;
    let kp_now = kp.last()?.Kp;
    // Nine three-hour values are about a day.
    let kp_max_24h = kp.iter().rev().take(9).map(|r| r.Kp).fold(0.0, f64::max);
    Some(SpaceWeather {
        f107: latest.flux,
        kp: kp_now,
        kp_max_24h,
        effective_ssn: kp_derate(ssn_from_f107(latest.flux), kp_now).round(),
        observed_at: parse_time(&latest.time_tag)?,
        fetched_at,
    })
}

/// "2026-10-24T22:00:00" or "2026.10.24T22:00", with an optional fraction
/// and "Z", as UTC seconds.
pub fn parse_time(text: &str) -> Option<i64> {
    let t = text.trim().trim_end_matches('Z');
    let (date, time) = t.split_once('T')?;
    let mut d = date.split(['-', '.']).map(|s| s.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut h = time.split(':');
    let hour: i64 = h.next()?.parse().ok()?;
    let minute: i64 = h.next().unwrap_or("0").parse().ok()?;
    let second: f64 = h.next().unwrap_or("0").parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    Some(
        geo::days_from_civil(y as i32, m as u32, day as u32) * 86_400
            + hour * 3600
            + minute * 60
            + second as i64,
    )
}

/// The ionosondes that report live, nearly all in Europe.
pub const IONOSONDES: [(&str, &str, f64, f64); 6] = [
    ("AT138", "Athens", 38.01, 23.53),
    ("EB040", "Roquetes", 40.8, 0.5),
    ("JR055", "Juliusruh", 54.63, 13.41),
    ("RO041", "Rome", 41.9, 12.5),
    ("SO148", "Sopron", 47.63, 16.72),
    ("GR13L", "Grahamstown", -33.3, 26.5),
];

/// Further than this a sounding says little about the station's sky.
pub const MAX_IONOSONDE_KM: f64 = 1500.0;
/// Autoscaled readings below this confidence are skipped.
pub const MIN_CONFIDENCE: f64 = 50.0;
/// How far back a reading is looked for.
pub const SOUNDING_WINDOW_MINUTES: i64 = 90;

/// The nearest live ionosonde within reach: (ursi, name, km).
pub fn useful_ionosonde(p: LatLon) -> Option<(&'static str, &'static str, f64)> {
    IONOSONDES
        .iter()
        .map(|&(ursi, name, lat, lon)| (ursi, name, geo::distance_km(p, LatLon { lat, lon })))
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .filter(|s| s.2 <= MAX_IONOSONDE_KM)
}

/// The newest confident row of a FastChar answer: (unix secs, CS, foF2).
pub fn parse_fastchar(body: &str) -> Option<(i64, f64, f64)> {
    body.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let at = parse_time(parts.next()?)?;
            let cs: f64 = parts.next()?.parse().ok()?;
            let fof2: f64 = parts.next()?.parse().ok()?;
            (fof2 > 0.0 && (cs >= MIN_CONFIDENCE || cs == 999.0)).then_some((
                at,
                cs.min(100.0),
                fof2,
            ))
        })
        .max_by_key(|r| r.0)
}

fn fastchar_date(unix: i64) -> String {
    let d = geo::UtcDate::of(unix);
    let minute = unix.rem_euclid(3600) / 60;
    format!(
        "{:04}.{:02}.{:02}T{:02}:{:02}",
        d.year, d.month, d.day, d.hour, minute
    )
}

#[derive(Deserialize)]
struct GeoResults {
    #[serde(default)]
    results: Vec<GeoPlace>,
}

#[derive(Deserialize)]
struct GeoPlace {
    name: String,
    latitude: f64,
    longitude: f64,
    country: Option<String>,
    admin1: Option<String>,
}

pub fn parse_geocode(body: &str) -> Option<Vec<Place>> {
    let r: GeoResults = serde_json::from_str(body).ok()?;
    Some(
        r.results
            .into_iter()
            .map(|p| {
                let detail = [p.admin1, p.country]
                    .into_iter()
                    .flatten()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                Place::at(p.name, detail, p.latitude, p.longitude)
            })
            .collect(),
    )
}

#[cfg(feature = "net")]
pub use fetch::*;

/// Built without networking: every fetch refuses.
#[cfg(not(feature = "net"))]
mod offline {
    use super::*;
    use crate::Error;

    pub fn fetch_space_weather(_now: i64) -> Result<SpaceWeather, Error> {
        Err(Error::Offline)
    }

    pub fn fetch_sounding(_p: LatLon, _now: i64) -> Result<Option<Sounding>, Error> {
        Err(Error::Offline)
    }

    pub fn fetch_places(_query: &str, _language: &str) -> Result<Vec<Place>, Error> {
        Err(Error::Offline)
    }
}

#[cfg(not(feature = "net"))]
pub use offline::*;

#[cfg(feature = "net")]
mod fetch {
    use std::time::Duration;

    use super::*;
    use crate::Error;

    const TIMEOUT: Duration = Duration::from_secs(8);
    const SWPC: &str = "https://services.swpc.noaa.gov";
    const FASTCHAR: &str = "https://lgdc.uml.edu/fastchar/getbest";
    const GEOCODER: &str = "https://geocoding-api.open-meteo.com/v1/search";

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .user_agent(concat!(
                "HFPredict/",
                env!("CARGO_PKG_VERSION"),
                " (+https://code.alvaroedo.io/alvaroemtnez/HFPredict)"
            ))
            .build()
            .into()
    }

    fn get(url: &str, query: &[(&str, &str)]) -> Result<String, Error> {
        let mut req = agent().get(url);
        for (k, v) in query {
            req = req.query(k, v);
        }
        let mut response = req.call().map_err(|e| Error::Network(e.to_string()))?;
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| Error::Network(e.to_string()))
    }

    pub fn fetch_space_weather(now: i64) -> Result<SpaceWeather, Error> {
        let flux = get(&format!("{SWPC}/json/f107_cm_flux.json"), &[])?;
        let kp = get(&format!("{SWPC}/products/noaa-planetary-k-index.json"), &[])?;
        parse_space_weather(&flux, &kp, now)
            .ok_or_else(|| Error::Network("NOAA returned no current observations".into()))
    }

    /// `Ok(None)` when no sounder is near enough, or none reported lately.
    pub fn fetch_sounding(p: LatLon, now: i64) -> Result<Option<Sounding>, Error> {
        let Some((ursi, name, km)) = useful_ionosonde(p) else {
            return Ok(None);
        };
        let from = fastchar_date(now - SOUNDING_WINDOW_MINUTES * 60);
        let to = fastchar_date(now);
        let body = get(
            FASTCHAR,
            &[
                ("ursiCode", ursi),
                ("charName", "foF2"),
                ("fromDate", &from),
                ("toDate", &to),
            ],
        )?;
        Ok(
            parse_fastchar(&body).map(|(measured_at, confidence, fof2)| Sounding {
                station: name.into(),
                ursi: ursi.into(),
                km: km.round(),
                fof2,
                measured_at,
                confidence,
            }),
        )
    }

    pub fn fetch_places(query: &str, language: &str) -> Result<Vec<Place>, Error> {
        let body = get(
            GEOCODER,
            &[
                ("name", query),
                ("count", "8"),
                ("language", language),
                ("format", "json"),
            ],
        )?;
        parse_geocode(&body)
            .ok_or_else(|| Error::Network("the place search answered nonsense".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flux_to_ssn() {
        assert_eq!(ssn_from_f107(63.7), 0.0);
        // 150 sfu is about SSN 105.
        assert!((ssn_from_f107(150.0) - 105.05).abs() < 0.01);
        assert_eq!(kp_derate(100.0, 3.0), 100.0);
        assert!((kp_derate(100.0, 6.0) - 84.0).abs() < 1e-9);
    }

    #[test]
    fn parses_noaa() {
        let flux = r#"[{"time_tag":"2026-10-24T22:00:00","frequency":2800,"flux":91.0},{"time_tag":"2026-10-24T20:00:00","flux":93.0}]"#;
        let kp = r#"[{"time_tag":"2026-10-24T00:00:00","Kp":5.33},{"time_tag":"2026-10-25T06:00:00","Kp":0.33}]"#;
        let sw = parse_space_weather(flux, kp, 0).unwrap();
        assert_eq!(sw.f107, 91.0);
        assert_eq!(sw.kp, 0.33);
        assert_eq!(sw.kp_max_24h, 5.33);
        assert_eq!(
            sw.observed_at,
            geo::days_from_civil(2026, 10, 24) * 86_400 + 22 * 3600
        );
    }

    #[test]
    fn parses_giro() {
        let body = "# comment\n# Time CS foF2 QD\n2026-10-25T09:37:30.000Z  90  6.512 //\n2026-10-25T09:52:30.000Z  30  7.000 //\n2026-10-25T09:45:00.000Z 999 6.600 //\n";
        let (at, cs, fof2) = parse_fastchar(body).unwrap();
        assert_eq!((cs, fof2), (100.0, 6.6));
        assert_eq!(
            at,
            geo::days_from_civil(2026, 10, 25) * 86_400 + 9 * 3600 + 45 * 60
        );
        assert!(
            useful_ionosonde(LatLon {
                lat: 39.47,
                lon: -0.38
            })
            .is_some_and(|s| s.0 == "EB040")
        );
        assert!(
            useful_ionosonde(LatLon {
                lat: 40.0,
                lon: -100.0
            })
            .is_none()
        );
    }

    #[test]
    fn parses_geocode() {
        let body = r#"{"results":[{"name":"Alcira","latitude":39.15,"longitude":-0.43333,"country":"España","admin1":"Comunidad Valenciana"}]}"#;
        let p = &parse_geocode(body).unwrap()[0];
        assert_eq!(p.detail, "Comunidad Valenciana, España");
        assert_eq!(p.grid, "IM99sd");
        assert!(parse_geocode("{}").unwrap().is_empty());
    }
}
