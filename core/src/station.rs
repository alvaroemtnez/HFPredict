//! The operator's station: what they transmit with.

use serde::{Deserialize, Serialize};

use crate::antenna::{Antenna, MAX_WATTS, MIN_WATTS};
use crate::modes::Mode;

/// A saved station. Several can be kept ("Home", "Portable") and one is
/// active.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Station {
    pub id: String,
    pub name: String,
    pub mode: Mode,
    pub watts: f64,
    pub antenna: Antenna,
}

pub const MAX_NAME_LENGTH: usize = 24;

impl Default for Station {
    /// 100 W of CW into no particular antenna, as VOACAP assumes.
    fn default() -> Self {
        Station { id: "s1".into(), name: String::new(), mode: Mode::DEFAULT, watts: 100.0, antenna: Antenna::default() }
    }
}

impl Station {
    /// Clamped into what the model and the controls accept.
    pub fn clamped(&self) -> Station {
        Station {
            id: self.id.clone(),
            name: self.name.trim().chars().take(MAX_NAME_LENGTH).collect(),
            mode: self.mode,
            watts: if self.watts.is_finite() { self.watts.clamp(MIN_WATTS, MAX_WATTS) } else { 100.0 },
            antenna: self.antenna.clamped(),
        }
    }

    /// Everything about the station that changes an answer, for cache keys.
    pub fn signature(&self) -> String {
        let a = &self.antenna;
        format!(
            "{}:{}:{:?}:{:.1}:{:.1}:{:.1}",
            self.mode.key(),
            self.watts,
            a.kind,
            if a.kind.uses_height() { a.height_m } else { 0.0 },
            if a.kind.uses_gain() { a.gain_dbd } else { 0.0 },
            a.engine_beam_deg()
        )
    }
}

/// The next free id, `s<n>`.
pub fn next_id(stations: &[Station]) -> String {
    let max = stations.iter().filter_map(|s| s.id.strip_prefix('s')?.parse::<u32>().ok()).max().unwrap_or(0);
    format!("s{}", max + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::antenna::AntennaKind;

    #[test]
    fn clamps_and_signs() {
        let mut s = Station { watts: 5000.0, ..Station::default() };
        s.antenna.kind = AntennaKind::Vertical;
        s.antenna.beam_deg = 400.0;
        let c = s.clamped();
        assert_eq!(c.watts, MAX_WATTS);
        assert_eq!(c.antenna.beam_deg, 40.0);
        // A vertical's bearing does not reach the engine, so not the key either.
        assert!(c.signature().ends_with(":0.0"));
        assert_eq!(next_id(&[Station::default(), Station { id: "s7".into(), ..Station::default() }]), "s8");
    }
}
