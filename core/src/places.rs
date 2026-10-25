//! Places: the built-in city list, coordinates and grid squares typed in,
//! and the record both ends of a path are kept as.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::geo::{self, LatLon};

/// One end of a path.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Place {
    /// Place name, or the locator / coordinates when none is known.
    pub name: String,
    /// Region and country, empty when unknown.
    pub detail: String,
    pub lat: f64,
    pub lon: f64,
    /// Six-character Maidenhead locator.
    pub grid: String,
}

impl Place {
    pub fn at(name: impl Into<String>, detail: impl Into<String>, lat: f64, lon: f64) -> Place {
        Place {
            name: name.into(),
            detail: detail.into(),
            lat,
            lon,
            grid: geo::lat_lon_to_grid(lat, lon),
        }
    }

    pub fn lat_lon(&self) -> LatLon {
        LatLon {
            lat: self.lat,
            lon: self.lon,
        }
    }

    /// Greenwich: where a first run works from until a place is chosen.
    pub fn greenwich() -> Place {
        Place::at("Greenwich", "England, United Kingdom", 51.4779, 0.0)
    }
}

#[derive(Deserialize)]
struct Cities {
    regions: Vec<String>,
    /// `[name, region index, lat, lon, alternate name?]`
    cities: Vec<serde_json::Value>,
}

struct Index {
    entries: Vec<City>,
    regions: Vec<String>,
    region_words: Vec<Vec<String>>,
}

struct City {
    name: String,
    alt: String,
    folded: String,
    folded_alt: String,
    region: usize,
    lat: f64,
    lon: f64,
}

fn index() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(|| {
        let raw: Cities = serde_json::from_str(include_str!("../data/cities.json"))
            .expect("data/cities.json is valid");
        let entries = raw
            .cities
            .iter()
            .filter_map(|row| {
                let row = row.as_array()?;
                let name = row.first()?.as_str()?.to_string();
                let alt = row
                    .get(4)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(City {
                    folded: fold(&name),
                    folded_alt: fold(&alt),
                    name,
                    alt,
                    region: row.get(1)?.as_u64()? as usize,
                    lat: row.get(2)?.as_f64()?,
                    lon: row.get(3)?.as_f64()?,
                })
            })
            .collect();
        let region_words = raw
            .regions
            .iter()
            .map(|r| {
                fold(r)
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| !w.is_empty())
                    .map(String::from)
                    .collect()
            })
            .collect();
        Index {
            entries,
            regions: raw.regions,
            region_words,
        }
    })
}

/// Lower case without diacritics, so "Valencia" finds "València".
pub fn fold(text: &str) -> String {
    text.nfd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .flat_map(char::to_lowercase)
        .collect()
}

/// How many places are built in.
pub fn city_count() -> usize {
    index().entries.len()
}

const LIMIT: usize = 40;

/// Searches the built-in list. "name" matches a prefix first, then anywhere
/// in the name; "name, region" also filters by a word of the region.
pub fn search_cities(query: &str) -> Vec<Place> {
    let idx = index();
    let (needle, where_) = match query.trim().split_once(',') {
        Some((n, w)) => (fold(n.trim()), fold(w.trim())),
        None => (fold(query.trim()), String::new()),
    };
    if needle.is_empty() {
        return vec![];
    }
    let in_region = |c: &City| {
        where_.is_empty()
            || idx.region_words[c.region]
                .iter()
                .any(|w| w.starts_with(&where_))
    };
    let mut prefix = vec![];
    let mut contains = vec![];
    for city in idx.entries.iter().filter(|c| in_region(c)) {
        if city.folded.starts_with(&needle)
            || (!city.folded_alt.is_empty() && city.folded_alt.starts_with(&needle))
        {
            prefix.push(city);
        } else if city.folded.contains(&needle)
            || (!city.folded_alt.is_empty() && city.folded_alt.contains(&needle))
        {
            contains.push(city);
        }
        if prefix.len() >= LIMIT {
            break;
        }
    }
    prefix
        .into_iter()
        .chain(contains)
        .take(LIMIT)
        .map(|c| {
            let region = &idx.regions[c.region];
            let name = if c.folded.starts_with(&needle) || c.alt.is_empty() {
                &c.name
            } else {
                &c.alt
            };
            Place::at(name.clone(), region.clone(), c.lat, c.lon)
        })
        .collect()
}

/// The closest built-in city within `max_km`, to name a GPS fix or a typed
/// coordinate.
pub fn nearest_city(point: LatLon, max_km: f64) -> Option<Place> {
    let idx = index();
    idx.entries
        .iter()
        .map(|c| {
            (
                c,
                geo::distance_km(
                    point,
                    LatLon {
                        lat: c.lat,
                        lon: c.lon,
                    },
                ),
            )
        })
        .filter(|(_, km)| *km <= max_km)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(c, _)| Place::at(c.name.clone(), idx.regions[c.region].clone(), c.lat, c.lon))
}

/// What a typed query is, before any search.
#[derive(Clone, Debug, PartialEq)]
pub enum Parsed {
    Grid(Place),
    Coordinates(Place),
    Text,
}

/// Recognises a Maidenhead locator or a coordinate pair, which are worked
/// out on the device and never reach the network.
pub fn parse_query(query: &str) -> Parsed {
    let text = query.trim();
    if let Some(p) = geo::grid_to_lat_lon(text) {
        let grid = text.to_string();
        // The locator is the name; the coordinates say where it is.
        return Parsed::Grid(Place {
            name: format_grid(&grid),
            detail: format_lat_lon(p),
            lat: p.lat,
            lon: p.lon,
            grid: geo::lat_lon_to_grid(p.lat, p.lon),
        });
    }
    if let Some(p) = parse_coordinates(text) {
        return Parsed::Coordinates(Place::at(format_lat_lon(p), "", p.lat, p.lon));
    }
    Parsed::Text
}

/// "im99SL" -> "IM99sl": fields and squares upper case, subsquares lower.
pub fn format_grid(grid: &str) -> String {
    grid.trim()
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if (i / 2) % 2 == 0 && i >= 4 {
                c.to_ascii_lowercase()
            } else {
                c.to_ascii_uppercase()
            }
        })
        .collect()
}

/// "39.4699, -0.3763"
pub fn format_lat_lon(p: LatLon) -> String {
    let place = |v: f64| {
        let s = format!("{v:.4}");
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s == "-0" {
            "0".to_string()
        } else {
            s.to_string()
        }
    };
    format!("{}, {}", place(p.lat), place(p.lon))
}

/// Decimal ("39.47, -0.38", "39.47 -0.38") or degrees-minutes-seconds with
/// hemisphere letters ("39°28'N 0°22'W"). Hemisphere letters are required
/// for DMS so the order is never guessed; a bare decimal pair is lat, lon.
pub fn parse_coordinates(input: &str) -> Option<LatLon> {
    let text: String = input
        .chars()
        .map(|c| match c {
            'º' => '°',
            '′' | '’' => '\'',
            '″' | '”' => '"',
            '‐'..='―' | '−' => '-',
            c => c,
        })
        .collect();
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    parse_dms(text).or_else(|| parse_decimal(text))
}

fn in_range(p: LatLon) -> Option<LatLon> {
    (p.lat.is_finite() && p.lon.is_finite() && p.lat.abs() <= 90.0 && p.lon.abs() <= 180.0)
        .then_some(p)
}

fn parse_decimal(text: &str) -> Option<LatLon> {
    let parts: Vec<&str> = if text.contains(',') {
        let (a, b) = text.split_once(',')?;
        vec![a.trim(), b.trim()]
    } else {
        text.split(|c: char| c.is_whitespace() || c == ';')
            .filter(|s| !s.is_empty())
            .collect()
    };
    if parts.len() != 2 {
        return None;
    }
    // Two bare integers without a comma ("40 3") read as a search, not a
    // coordinate.
    if !text.contains(',') && !text.contains(['.', '-', '+']) {
        return None;
    }
    let number = |s: &str| {
        let s = s.trim();
        let digits = s.trim_start_matches(['+', '-']);
        let ok = !digits.is_empty()
            && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
            && digits
                .split('.')
                .next()
                .is_some_and(|i| (1..=3).contains(&i.len()));
        if ok { s.parse::<f64>().ok() } else { None }
    };
    in_range(LatLon {
        lat: number(parts[0])?,
        lon: number(parts[1])?,
    })
}

fn parse_dms(text: &str) -> Option<LatLon> {
    let upper = text.to_ascii_uppercase();
    // Split into two halves at the hemisphere letters.
    let letters: Vec<(usize, char)> = upper
        .char_indices()
        .filter(|(_, c)| matches!(c, 'N' | 'S' | 'E' | 'W'))
        .collect();
    if letters.len() != 2 {
        return None;
    }
    // Each half is a number group with its letter before or after.
    let (first, second) = split_dms(&upper, &letters)?;
    let a = dms_value(&first)?;
    let b = dms_value(&second)?;
    let (lat, lon) = match (a.1, b.1) {
        ('N' | 'S', 'E' | 'W') => (a.0, b.0),
        ('E' | 'W', 'N' | 'S') => (b.0, a.0),
        _ => return None,
    };
    in_range(LatLon { lat, lon })
}

fn split_dms(text: &str, letters: &[(usize, char)]) -> Option<(String, String)> {
    let (i0, i1) = (letters[0].0, letters[1].0);
    // Letter after the numbers: "39 28 N, 0 22 W"; letter before: "N 39 28, W 0 22".
    let letter_first = text[..i0].trim().is_empty();
    if letter_first {
        Some((text[..i1].to_string(), text[i1..].to_string()))
    } else {
        Some((text[..=i0].to_string(), text[i0 + 1..].to_string()))
    }
}

fn dms_value(part: &str) -> Option<(f64, char)> {
    let hemisphere = part.chars().find(|c| matches!(c, 'N' | 'S' | 'E' | 'W'))?;
    let numbers: Vec<f64> = part
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|s| !s.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if numbers.is_empty() || numbers.len() > 3 {
        return None;
    }
    let minutes = numbers.get(1).copied().unwrap_or(0.0);
    let seconds = numbers.get(2).copied().unwrap_or(0.0);
    if minutes >= 60.0 || seconds >= 60.0 {
        return None;
    }
    let magnitude = numbers[0] + minutes / 60.0 + seconds / 3600.0;
    Some((
        if matches!(hemisphere, 'S' | 'W') {
            -magnitude
        } else {
            magnitude
        },
        hemisphere,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_diacritics() {
        assert_eq!(fold("València"), "valencia");
        assert_eq!(fold("ZÜRICH"), "zurich");
    }

    #[test]
    fn finds_cities() {
        assert_eq!(city_count(), 3997);
        let found = search_cities("london");
        assert!(
            found
                .iter()
                .any(|p| p.name.to_lowercase().starts_with("london")),
            "{found:?}"
        );
        let tokyo = search_cities("Tokyo");
        assert!(!tokyo.is_empty());
        assert!(search_cities("   ").is_empty());
    }

    #[test]
    fn parses_coordinates() {
        let p = parse_coordinates("39.47, -0.38").unwrap();
        assert_eq!((p.lat, p.lon), (39.47, -0.38));
        assert!(parse_coordinates("39.47 -0.38").is_some());
        assert!(parse_coordinates("40 3").is_none());
        let d = parse_coordinates("39°28'N 0°22'W").unwrap();
        assert!((d.lat - 39.4667).abs() < 1e-3 && (d.lon + 0.3667).abs() < 1e-3);
        let swapped = parse_coordinates("W 0 22, N 39 28").unwrap();
        assert!((swapped.lat - d.lat).abs() < 1e-9);
        assert!(parse_coordinates("95, 10").is_none());
        assert!(parse_coordinates("hello").is_none());
    }

    #[test]
    fn query_kinds() {
        assert!(matches!(parse_query("im99sl"), Parsed::Grid(p) if p.name == "IM99sl"));
        assert!(matches!(parse_query("51.5, -0.1"), Parsed::Coordinates(_)));
        assert_eq!(parse_query("Valencia"), Parsed::Text);
        assert_eq!(
            format_lat_lon(LatLon {
                lat: 39.5,
                lon: -0.0
            }),
            "39.5, 0"
        );
    }
}
