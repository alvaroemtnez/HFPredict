//! The amateur bands HFPredict forecasts, and their frequencies.

use serde::{Deserialize, Serialize};

/// One amateur band. Declared in ascending frequency, which is the order the
/// engine needs for a multi-band run (each band's antenna table is cut
/// halfway to its neighbours, so a list that is not increasing is refused).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum Band {
    #[serde(rename = "160m")]
    M160,
    #[serde(rename = "80m")]
    M80,
    #[serde(rename = "60m")]
    M60,
    #[serde(rename = "40m")]
    M40,
    #[serde(rename = "30m")]
    M30,
    #[serde(rename = "20m")]
    M20,
    #[serde(rename = "17m")]
    M17,
    #[serde(rename = "15m")]
    M15,
    #[serde(rename = "12m")]
    M12,
    #[serde(rename = "10m")]
    M10,
}

impl Band {
    /// Ascending frequency: the order the engine is asked in.
    pub const BY_FREQ: [Band; 10] = [
        Band::M160,
        Band::M80,
        Band::M60,
        Band::M40,
        Band::M30,
        Band::M20,
        Band::M17,
        Band::M15,
        Band::M12,
        Band::M10,
    ];

    /// Display order: highest frequency first, as the selector lists them.
    pub const DISPLAY: [Band; 10] = [
        Band::M10,
        Band::M12,
        Band::M15,
        Band::M17,
        Band::M20,
        Band::M30,
        Band::M40,
        Band::M60,
        Band::M80,
        Band::M160,
    ];

    /// Nominal centre frequency in MHz.
    ///
    /// 60m is not one contiguous allocation everywhere; 5.36 MHz sits inside
    /// the WRC-15 band and within 1% of the middle of the channelised plans.
    pub fn mhz(self) -> f64 {
        match self {
            Band::M160 => 1.84,
            Band::M80 => 3.75,
            Band::M60 => 5.36,
            Band::M40 => 7.1,
            Band::M30 => 10.12,
            Band::M20 => 14.2,
            Band::M17 => 18.1,
            Band::M15 => 21.2,
            Band::M12 => 24.94,
            Band::M10 => 28.4,
        }
    }

    /// "20m", as shown on screen and in every language.
    pub fn label(self) -> &'static str {
        match self {
            Band::M160 => "160m",
            Band::M80 => "80m",
            Band::M60 => "60m",
            Band::M40 => "40m",
            Band::M30 => "30m",
            Band::M20 => "20m",
            Band::M17 => "17m",
            Band::M15 => "15m",
            Band::M12 => "12m",
            Band::M10 => "10m",
        }
    }

    /// Position in [`Band::BY_FREQ`], which is also the index into the
    /// engine's multi-band answers.
    pub fn freq_index(self) -> usize {
        self as usize
    }

    /// The band whose centre frequency this is, within the engine's echo
    /// precision.
    pub fn from_mhz(mhz: f64) -> Option<Band> {
        Band::BY_FREQ.into_iter().find(|b| (b.mhz() - mhz).abs() < 1e-4)
    }

    pub fn all_mhz() -> Vec<f64> {
        Band::BY_FREQ.iter().map(|b| b.mhz()).collect()
    }
}

/// The lowest frequency an antenna card claims to serve, in whole MHz.
///
/// The engine gives a frequency in no card's range no antenna at all, and the
/// card's default floor is 2 MHz, which would leave 160m isotropic.
pub const MIN_CARD_FREQ_MHZ: i32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_freq_ascends_and_indexes() {
        for (i, pair) in Band::BY_FREQ.windows(2).enumerate() {
            assert!(pair[0].mhz() < pair[1].mhz());
            assert_eq!(pair[0].freq_index(), i);
        }
        assert_eq!(Band::from_mhz(14.2), Some(Band::M20));
        assert_eq!(Band::from_mhz(14.3), None);
    }

    #[test]
    fn serde_uses_labels() {
        assert_eq!(serde_json::to_string(&Band::M60).unwrap(), "\"60m\"");
        let b: Band = serde_json::from_str("\"160m\"").unwrap();
        assert_eq!(b, Band::M160);
    }
}
