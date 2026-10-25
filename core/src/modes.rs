//! The operator's mode, and the signal-to-noise ratio it needs.
//!
//! The engine's SNR is in a 1 Hz bandwidth (VOACAP's -145 dBW residential
//! noise is per hertz), so a mode's requirement converts as
//!
//! ```text
//! required = in-channel SNR + 10 log10(reference bandwidth)
//! ```
//!
//! CW and SSB reproduce VOACAP's long-standing 24 dB and 38 dB. The
//! weak-signal digital modes are quoted in the 2500 Hz reference their
//! decoders report in, so the app agrees with what the operator reads on
//! screen. Values from HFcast (`shared/modes.ts`).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum Mode {
    Fm,
    Am,
    Ssb,
    Rtty,
    Cw,
    Psk31,
    Ft8,
    Js8,
    Wspr,
}

/// What a mode occupies and needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModeSpec {
    /// What the signal occupies, Hz. Shown to the reader only.
    pub occupied_hz: f64,
    /// The bandwidth `snr_db` is quoted in, Hz.
    pub reference_hz: f64,
    /// Signal-to-noise the mode needs in `reference_hz`, dB.
    pub snr_db: f64,
}

impl Mode {
    /// Hardest first, as the picker lists them.
    pub const ORDER: [Mode; 9] = [
        Mode::Fm,
        Mode::Am,
        Mode::Ssb,
        Mode::Rtty,
        Mode::Cw,
        Mode::Psk31,
        Mode::Ft8,
        Mode::Js8,
        Mode::Wspr,
    ];

    pub const DEFAULT: Mode = Mode::Cw;

    pub fn spec(self) -> ModeSpec {
        let (occupied_hz, reference_hz, snr_db) = match self {
            Mode::Fm => (16_000.0, 16_000.0, 12.0),
            Mode::Am => (6_000.0, 6_000.0, 8.2),
            Mode::Ssb => (2_400.0, 2_400.0, 4.2),
            Mode::Rtty => (250.0, 250.0, 8.0),
            Mode::Cw => (500.0, 500.0, -3.0),
            Mode::Psk31 => (31.0, 31.25, 6.0),
            Mode::Ft8 => (50.0, 2_500.0, -21.0),
            Mode::Js8 => (50.0, 2_500.0, -24.0),
            Mode::Wspr => (6.0, 2_500.0, -29.0),
        };
        ModeSpec {
            occupied_hz,
            reference_hz,
            snr_db,
        }
    }

    /// The required SNR the engine takes, in 1 Hz, rounded to whole dB (the
    /// engine prints SNR to the nearest dB).
    pub fn required_snr_db(self) -> i32 {
        let spec = self.spec();
        (spec.snr_db + 10.0 * spec.reference_hz.log10()).round() as i32
    }

    /// Short name, the same in every language.
    pub fn key(self) -> &'static str {
        match self {
            Mode::Fm => "fm",
            Mode::Am => "am",
            Mode::Ssb => "ssb",
            Mode::Rtty => "rtty",
            Mode::Cw => "cw",
            Mode::Psk31 => "psk31",
            Mode::Ft8 => "ft8",
            Mode::Js8 => "js8",
            Mode::Wspr => "wspr",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voacap_classics_hold() {
        assert_eq!(Mode::Cw.required_snr_db(), 24);
        assert_eq!(Mode::Ssb.required_snr_db(), 38);
    }

    #[test]
    fn digital_modes_use_the_decoder_reference() {
        // -21 + 10 log10(2500) = 12.98
        assert_eq!(Mode::Ft8.required_snr_db(), 13);
        assert_eq!(Mode::Js8.required_snr_db(), 10);
        assert_eq!(Mode::Wspr.required_snr_db(), 5);
    }
}
