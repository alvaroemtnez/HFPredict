//! Antennas: the families offered, their rules, and the VOACAP definition
//! file the engine reads.
//!
//! VOACAP names an antenna by a file path, so a station the operator
//! describes by height, gain and bearing is written out as a `.voa` file into
//! a cache directory the engine is pointed at, with the coefficients still
//! coming from inside the library. The card layout follows HFcast
//! (`shared/antenna.ts`, `mobile/src/data/antennaFile.ts`).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum AntennaKind {
    Isotropic,
    Dipole,
    InvertedV,
    Vertical,
    InvertedL,
    Yagi,
}

impl AntennaKind {
    /// The order the picker lists them in.
    pub const ORDER: [AntennaKind; 6] = [
        AntennaKind::Isotropic,
        AntennaKind::Dipole,
        AntennaKind::InvertedV,
        AntennaKind::Vertical,
        AntennaKind::InvertedL,
        AntennaKind::Yagi,
    ];

    /// Only a beam has a gain figure to state.
    pub fn uses_gain(self) -> bool {
        self == AntennaKind::Yagi
    }

    /// Everything but the isotrope sits at a height that changes the answer.
    pub fn uses_height(self) -> bool {
        self != AntennaKind::Isotropic
    }

    /// Families with a direction at all. Swept on a 14 MHz path a dipole and
    /// an inverted L move 12 dB round the compass; a vertical moves nothing.
    pub fn uses_beam(self) -> bool {
        matches!(
            self,
            AntennaKind::Dipole | AntennaKind::InvertedV | AntennaKind::InvertedL | AntennaKind::Yagi
        )
    }

    /// Wire antennas are asked "which way does the wire run" rather than
    /// "where is it pointed".
    pub fn asked_as_wire(self) -> bool {
        matches!(self, AntennaKind::Dipole | AntennaKind::InvertedV)
    }

    /// Two equal lobes, opposite each other.
    pub fn is_bidirectional(self) -> bool {
        matches!(self, AntennaKind::Dipole | AntennaKind::InvertedV | AntennaKind::InvertedL)
    }

    fn voa_type(self) -> u32 {
        match self {
            AntennaKind::Isotropic => 0,
            AntennaKind::Vertical => 22,
            // The inverted V has no pattern of its own in VOACAP; it is the
            // dipole's, at a lower effective height.
            AntennaKind::Dipole | AntennaKind::InvertedV => 23,
            AntennaKind::Yagi => 24,
            AntennaKind::InvertedL => 28,
        }
    }

    fn title(self) -> &'static str {
        match self {
            AntennaKind::Isotropic => "isotrope",
            AntennaKind::Dipole => "dipole",
            AntennaKind::InvertedV => "inverted V",
            AntennaKind::Vertical => "vertical",
            AntennaKind::InvertedL => "inverted L",
            AntennaKind::Yagi => "yagi",
        }
    }

    /// One letter per family for file names. The inverted V gets its own so a
    /// 10 m V and an 8 m dipole (the same card) keep separate files.
    fn short(self) -> char {
        match self {
            AntennaKind::Isotropic => 'i',
            AntennaKind::Dipole => 'd',
            AntennaKind::InvertedV => 'w',
            AntennaKind::Vertical => 'v',
            AntennaKind::InvertedL => 'l',
            AntennaKind::Yagi => 'y',
        }
    }
}

/// An antenna as the operator describes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Antenna {
    pub kind: AntennaKind,
    /// Height above ground in metres, whatever unit the reader sees: the feed
    /// of a dipole or yagi, the element of a vertical, the horizontal run of
    /// an inverted L and the apex of an inverted V.
    pub height_m: f64,
    /// Gain over a half-wave dipole, dB. Only the yagi uses it.
    pub gain_dbd: f64,
    /// Main lobe bearing, degrees true. For a wire this is at right angles to
    /// the wire.
    pub beam_deg: f64,
}

impl Default for Antenna {
    fn default() -> Self {
        Antenna { kind: AntennaKind::Isotropic, height_m: 10.0, gain_dbd: 6.0, beam_deg: 0.0 }
    }
}

pub const MIN_HEIGHT_M: f64 = 1.0;
pub const MAX_HEIGHT_M: f64 = 100.0;
pub const MIN_GAIN_DBD: f64 = 0.0;
pub const MAX_GAIN_DBD: f64 = 20.0;
/// Below 0.1 W VOACAP stops tracking power; 1500 W is the legal ceiling in
/// the countries HFcast was built for.
pub const MIN_WATTS: f64 = 0.1;
pub const MAX_WATTS: f64 = 1500.0;

/// An inverted V behaves like a horizontal dipole about four fifths of the
/// way up to its apex, for the shallow droop amateurs build. A stated
/// approximation: VOACAP has no inverted V.
pub const INVERTED_V_HEIGHT_FRACTION: f64 = 0.8;

impl Antenna {
    /// The height the engine is given. Only the inverted V differs.
    pub fn effective_height_m(&self) -> f64 {
        if self.kind == AntennaKind::InvertedV {
            self.height_m * INVERTED_V_HEIGHT_FRACTION
        } else {
            self.height_m
        }
    }

    /// Clamped into the ranges the model and the controls accept.
    pub fn clamped(&self) -> Antenna {
        Antenna {
            kind: self.kind,
            height_m: self.height_m.clamp(MIN_HEIGHT_M, MAX_HEIGHT_M),
            gain_dbd: self.gain_dbd.clamp(MIN_GAIN_DBD, MAX_GAIN_DBD),
            beam_deg: wrap_deg(self.beam_deg),
        }
    }

    /// The bearing the engine is given: only directional families carry one,
    /// so a vertical does not refetch answers that cannot differ.
    pub fn engine_beam_deg(&self) -> f64 {
        if self.kind.uses_beam() { self.beam_deg } else { 0.0 }
    }

    /// The file for this antenna, or `None` for an isotropic station, which
    /// names no file at all (the engine's default).
    pub fn voa_file(&self) -> Option<VoaFile> {
        if self.kind == AntennaKind::Isotropic {
            return None;
        }
        let height = self.height_m.round() as i64;
        let gain = if self.kind == AntennaKind::Yagi { self.gain_dbd.round() as i64 } else { 0 };
        // The card holds 21 columns for the path; `antennas/` is stripped by
        // the engine, so `hfpredict/` plus this name must fit.
        let name = format!("hfp/{}{height}g{gain}.voa", self.kind.short());
        Some(VoaFile { path: format!("antennas/{name}"), name, text: self.definition() })
    }

    fn definition(&self) -> String {
        let height = format!("{:>6.2}", self.effective_height_m());
        let mut params = vec![
            param("  0.00", 1, "Max Gain dBi..:"),
            param(&format!("{:>6}", self.kind.voa_type()), 2, "Antenna Type..:"),
            param(&format!("{:>6}", DIELECTRIC), 3, "Dielectric....:"),
            param(&format!("{:>6.5}", CONDUCTIVITY), 4, "Conductivity..:"),
            param(&format!("{:>6.3}", DESIGN_MHZ), 5, "Operating Freq:"),
        ];
        // For the monopole parameter 6 is its height; the dipole has length
        // then height. Mixing them up gives a vertical as tall as its gain.
        match self.kind {
            AntennaKind::Dipole | AntennaKind::InvertedV => {
                params.push(param("  -.50", 6, "Antenna Length:"));
                params.push(param(&height, 7, "Antenna Height:"));
                params.push(param("   0.0", 8, "Gain ab dipole:"));
            }
            AntennaKind::Yagi => {
                params.push(param("  -.50", 6, "Antenna Length:"));
                params.push(param(&height, 7, "Antenna Height:"));
                params.push(param(&format!("{:>6.1}", self.gain_dbd), 8, "Gain ab dipole:"));
            }
            AntennaKind::Vertical => {
                params.push(param(&height, 6, "Antenna Height:"));
                params.push(param("   0.0", 7, "Gain ab dipole:"));
            }
            // An amateur inverted L is about a quarter wave of wire in total,
            // so the horizontal run is taken as equal to the height.
            AntennaKind::InvertedL => {
                params.push(param(&height, 6, "Antenna Length:"));
                params.push(param(&height, 7, "Antenna Height:"));
            }
            AntennaKind::Isotropic => {}
        }
        // The count on the second line is what the reader trusts.
        let mut lines = vec![
            format!("HFPredict {} {} m", self.kind.title(), self.height_m),
            format!("{:>2}    {} parameters", params.len(), params.len()),
        ];
        lines.extend(params);
        lines.push(String::new());
        lines.join("\n")
    }
}

/// Average ground, as every distributed sample uses.
const DIELECTRIC: u32 = 4;
const CONDUCTIVITY: f64 = 0.001;
/// Only parameters in wavelengths read it; heights here are metres.
const DESIGN_MHZ: f64 = 10.0;

fn param(value: &str, index: u32, name: &str) -> String {
    format!("{value}  [{index:>2}] {name}")
}

/// An antenna definition, ready to write under the engine's overlay root.
#[derive(Clone, Debug, PartialEq)]
pub struct VoaFile {
    /// The name on the card, which the engine joins after `antennas/`.
    pub name: String,
    /// Where to write it, relative to the overlay directory.
    pub path: String,
    pub text: String,
}

pub fn wrap_deg(deg: f64) -> f64 {
    deg.rem_euclid(360.0)
}

/// For a wire, the beam is at right angles to the run.
pub fn beam_from_wire(wire_deg: f64) -> f64 {
    wrap_deg(wire_deg + 90.0)
}

pub fn wire_from_beam(beam_deg: f64) -> f64 {
    wrap_deg(beam_deg - 90.0)
}

/// The directions this antenna favours, ascending.
pub fn lobes(beam_deg: f64, kind: AntennaKind) -> Vec<f64> {
    let main = wrap_deg(beam_deg);
    if kind.is_bidirectional() {
        let mut both = vec![main, wrap_deg(main + 180.0)];
        both.sort_by(f64::total_cmp);
        both
    } else {
        vec![main]
    }
}

/// Angle between two bearings, 0 to 180.
pub fn separation_deg(a: f64, b: f64) -> f64 {
    let raw = (wrap_deg(a) - wrap_deg(b)).abs();
    if raw > 180.0 { 360.0 - raw } else { raw }
}

/// The lobe closest to a path's bearing.
pub fn nearest_lobe(beam_deg: f64, kind: AntennaKind, path_deg: f64) -> f64 {
    lobes(beam_deg, kind)
        .into_iter()
        .min_by(|a, b| separation_deg(*a, path_deg).total_cmp(&separation_deg(*b, path_deg)))
        .unwrap_or(beam_deg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn antenna(kind: AntennaKind, height_m: f64) -> Antenna {
        Antenna { kind, height_m, gain_dbd: 6.0, beam_deg: 45.0 }
    }

    #[test]
    fn isotropic_names_no_file() {
        assert!(antenna(AntennaKind::Isotropic, 10.0).voa_file().is_none());
    }

    #[test]
    fn dipole_card_matches_hfcast_layout() {
        let file = antenna(AntennaKind::Dipole, 10.0).voa_file().unwrap();
        assert_eq!(file.name, "hfp/d10g0.voa");
        assert_eq!(file.path, "antennas/hfp/d10g0.voa");
        let expected = "HFPredict dipole 10 m\n 8    8 parameters\n  0.00  [ 1] Max Gain dBi..:\n    23  [ 2] Antenna Type..:\n     4  [ 3] Dielectric....:\n0.00100  [ 4] Conductivity..:\n10.000  [ 5] Operating Freq:\n  -.50  [ 6] Antenna Length:\n 10.00  [ 7] Antenna Height:\n   0.0  [ 8] Gain ab dipole:\n";
        assert_eq!(file.text, expected);
        // The engine's card budget: `antennas/` stripped, 21 columns.
        assert!(file.name.len() <= 21);
    }

    #[test]
    fn inverted_v_runs_lower_but_keeps_its_name() {
        let file = antenna(AntennaKind::InvertedV, 10.0).voa_file().unwrap();
        assert_eq!(file.name, "hfp/w10g0.voa");
        assert!(file.text.contains("  8.00  [ 7] Antenna Height:"));
        assert!(file.text.starts_with("HFPredict inverted V 10 m"));
    }

    #[test]
    fn vertical_puts_height_first() {
        let file = antenna(AntennaKind::Vertical, 6.0).voa_file().unwrap();
        assert!(file.text.contains("  6.00  [ 6] Antenna Height:"));
        assert!(file.text.contains(" 7    7 parameters"));
    }

    #[test]
    fn yagi_carries_gain_in_name_and_card() {
        let file = antenna(AntennaKind::Yagi, 12.0).voa_file().unwrap();
        assert_eq!(file.name, "hfp/y12g6.voa");
        assert!(file.text.contains("   6.0  [ 8] Gain ab dipole:"));
    }

    #[test]
    fn lobes_and_wires() {
        assert_eq!(lobes(350.0, AntennaKind::Dipole), vec![170.0, 350.0]);
        assert_eq!(beam_from_wire(260.0), 350.0);
        assert_eq!(nearest_lobe(350.0, AntennaKind::Dipole, 200.0), 170.0);
        assert_eq!(separation_deg(10.0, 350.0), 20.0);
    }
}
