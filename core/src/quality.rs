//! Reliability as a decision: four buckets an operator can act on.

use serde::{Deserialize, Serialize};

/// What "reachable" means: the share of a map at least `Patchy`. The reach
/// percentage, both surveys and the map colours all count against this one
/// number.
pub const REACHABLE: f64 = 0.4;
pub const RELIABLE_AT: f64 = 0.7;
pub const WEAK_AT: f64 = 0.15;

/// Above this take-off angle a path works by near-vertical incidence (NVIS):
/// no skip zone. The engine's own numbers sit well clear of it either way.
pub const NVIS_MIN_ANGLE_DEG: f64 = 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum Quality {
    Closed,
    Weak,
    Patchy,
    Reliable,
}

impl Quality {
    /// Best first, as legends list them.
    pub const ORDER: [Quality; 4] = [
        Quality::Reliable,
        Quality::Patchy,
        Quality::Weak,
        Quality::Closed,
    ];

    pub fn of(reliability: f64) -> Quality {
        if reliability >= RELIABLE_AT {
            Quality::Reliable
        } else if reliability >= REACHABLE {
            Quality::Patchy
        } else if reliability >= WEAK_AT {
            Quality::Weak
        } else {
            Quality::Closed
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Quality::Closed => "closed",
            Quality::Weak => "weak",
            Quality::Patchy => "patchy",
            Quality::Reliable => "reliable",
        }
    }
}

/// Whether a band and hour reach by near-vertical incidence. A closed band
/// is excluded: the angle describes a mode the engine found working.
pub fn is_nvis(takeoff_angle_deg: Option<f64>, reliability: f64) -> bool {
    takeoff_angle_deg.is_some_and(|a| a >= NVIS_MIN_ANGLE_DEG)
        && Quality::of(reliability) != Quality::Closed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets() {
        assert_eq!(Quality::of(0.7), Quality::Reliable);
        assert_eq!(Quality::of(0.69), Quality::Patchy);
        assert_eq!(Quality::of(0.4), Quality::Patchy);
        assert_eq!(Quality::of(0.15), Quality::Weak);
        assert_eq!(Quality::of(0.1), Quality::Closed);
        assert!(is_nvis(Some(80.0), 0.5));
        assert!(!is_nvis(Some(80.0), 0.05));
        assert!(!is_nvis(None, 0.9));
    }
}
