//! What the app remembers between launches, in one JSON file.
//!
//! The platform gives a private data directory; the file is written through
//! a temporary name so a crash mid-write leaves the previous state intact.
//! Unknown or missing fields fall back to defaults, so an older file always
//! loads.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::bands::Band;
use crate::engine::Model;
use crate::online::{Sounding, SpaceWeather};
use crate::places::Place;
use crate::station::{self, Station};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
    /// Dim red on black, for keeping night vision in the field.
    LowLight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum Units {
    /// From the locale: imperial in the US, Liberia and Myanmar.
    #[default]
    Auto,
    Metric,
    Imperial,
}

impl Units {
    /// Whether to show imperial units, for a locale tag like "en-US".
    pub fn imperial_for(self, locale: &str) -> bool {
        match self {
            Units::Metric => false,
            Units::Imperial => true,
            Units::Auto => locale
                .replace('_', "-")
                .split(['-', '.', '@'])
                .skip(1)
                .any(|part| matches!(part.to_ascii_uppercase().as_str(), "US" | "LR" | "MM")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Settings {
    pub theme: Theme,
    pub units: Units,
    /// A language code ("es", "ca"), or `None` to follow the system.
    pub language: Option<String>,
    pub model: Model,
    /// Whether the app may use the network at all.
    pub network: bool,
    /// Whether computed maps are kept on disk.
    pub keep_maps: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: Theme::System,
            units: Units::Auto,
            language: None,
            model: Model::Truecast,
            network: true,
            keep_maps: true,
        }
    }
}

/// Everything saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct SavedState {
    pub settings: Settings,
    pub stations: Vec<Station>,
    pub active_station: String,
    /// `None` until a first run has chosen (or skipped to Greenwich).
    pub from: Option<Place>,
    /// `None` forecasts every direction.
    pub to: Option<Place>,
    pub band: Band,
    /// Places picked lately, newest first, for one-tap reuse.
    pub recent: Vec<Place>,
    pub space_weather: Option<SpaceWeather>,
    pub sounding: Option<Sounding>,
}

impl Default for SavedState {
    fn default() -> Self {
        let first = Station::default();
        SavedState {
            settings: Settings::default(),
            active_station: first.id.clone(),
            stations: vec![first],
            from: None,
            to: None,
            band: Band::M20,
            recent: vec![],
            space_weather: None,
            sounding: None,
        }
    }
}

pub const MAX_RECENT: usize = 8;

impl SavedState {
    pub fn station(&self) -> Station {
        self.stations
            .iter()
            .find(|s| s.id == self.active_station)
            .or(self.stations.first())
            .cloned()
            .unwrap_or_default()
    }

    /// Repairs anything a hand-edited or old file could hold: no stations,
    /// an active id that is gone, values out of range.
    pub fn normalised(mut self) -> SavedState {
        if self.stations.is_empty() {
            self.stations.push(Station::default());
        }
        self.stations = self.stations.iter().map(Station::clamped).collect();
        if !self.stations.iter().any(|s| s.id == self.active_station) {
            self.active_station = self.stations[0].id.clone();
        }
        self.recent.truncate(MAX_RECENT);
        self
    }

    /// Saves a station (new or changed) and makes it active.
    pub fn put_station(&mut self, station: Station) {
        let station = station.clamped();
        self.active_station = station.id.clone();
        match self.stations.iter_mut().find(|s| s.id == station.id) {
            Some(existing) => *existing = station,
            None => self.stations.push(station),
        }
    }

    /// A copy of the active station under a new id, to change one thing.
    pub fn new_station(&self) -> Station {
        Station {
            id: station::next_id(&self.stations),
            name: String::new(),
            ..self.station()
        }
    }

    /// Removes a station; the last one cannot go.
    pub fn delete_station(&mut self, id: &str) {
        if self.stations.len() > 1 {
            self.stations.retain(|s| s.id != id);
            if self.active_station == id {
                self.active_station = self.stations[0].id.clone();
            }
        }
    }

    pub fn remember(&mut self, place: &Place) {
        self.recent
            .retain(|p| (p.lat - place.lat).abs() > 1e-6 || (p.lon - place.lon).abs() > 1e-6);
        self.recent.insert(0, place.clone());
        self.recent.truncate(MAX_RECENT);
    }
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(dir: &Path) -> Store {
        Store {
            path: dir.join("state.json"),
        }
    }

    /// The saved state, or defaults when there is none or it cannot be read.
    pub fn load(&self) -> SavedState {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str::<SavedState>(&text).ok())
            .unwrap_or_default()
            .normalised()
    }

    pub fn save(&self, state: &SavedState) -> Result<(), Error> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text =
            serde_json::to_string_pretty(state).map_err(|e| Error::Storage(e.to_string()))?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_repairs() {
        let dir = std::env::temp_dir().join("hfp-store-test");
        let _ = fs::remove_dir_all(&dir);
        let store = Store::new(&dir);
        let mut state = store.load();
        assert_eq!(state, SavedState::default());
        let mut extra = state.new_station();
        extra.name = "Portable".into();
        extra.watts = 5.0;
        state.put_station(extra.clone());
        state.from = Some(Place::greenwich());
        store.save(&state).unwrap();
        let back = store.load();
        assert_eq!(back.station(), extra);
        assert_eq!(back.stations.len(), 2);

        // A partial, older file still loads.
        fs::write(
            dir.join("state.json"),
            r#"{"settings":{"theme":"dark"},"activeStation":"gone"}"#,
        )
        .unwrap();
        let old = store.load();
        assert_eq!(old.settings.theme, Theme::Dark);
        assert!(old.settings.network);
        assert_eq!(old.station(), Station::default());
    }

    #[test]
    fn units_from_locale() {
        assert!(Units::Auto.imperial_for("en-US"));
        assert!(Units::Auto.imperial_for("en_US.UTF-8"));
        assert!(!Units::Auto.imperial_for("es-ES"));
        assert!(!Units::Auto.imperial_for("en"));
        assert!(Units::Imperial.imperial_for("es-ES"));
    }

    #[test]
    fn last_station_stays() {
        let mut state = SavedState::default();
        state.delete_station("s1");
        assert_eq!(state.stations.len(), 1);
    }
}
