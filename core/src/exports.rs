//! Small pure helpers the Android app calls instead of re-implementing them
//! in Kotlin. (The GNOME app reaches the same functions directly.)

use crate::antenna::{self, AntennaKind};
use crate::bands::Band;
use crate::geo;
use crate::modes::Mode;
use crate::quality::{self, Quality};

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn band_label(band: Band) -> String {
    band.label().to_string()
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn band_mhz(band: Band) -> f64 {
    band.mhz()
}

/// Highest frequency first, as the selector lists them.
#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn bands_display_order() -> Vec<Band> {
    Band::DISPLAY.to_vec()
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn modes_in_order() -> Vec<Mode> {
    Mode::ORDER.to_vec()
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn mode_key(mode: Mode) -> String {
    mode.key().to_string()
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn required_snr_db(mode: Mode) -> i32 {
    mode.required_snr_db()
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn antennas_in_order() -> Vec<AntennaKind> {
    AntennaKind::ORDER.to_vec()
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn antenna_uses_height(kind: AntennaKind) -> bool {
    kind.uses_height()
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn antenna_uses_gain(kind: AntennaKind) -> bool {
    kind.uses_gain()
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn antenna_uses_beam(kind: AntennaKind) -> bool {
    kind.uses_beam()
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn antenna_asked_as_wire(kind: AntennaKind) -> bool {
    kind.asked_as_wire()
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn antenna_is_bidirectional(kind: AntennaKind) -> bool {
    kind.is_bidirectional()
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn antenna_lobes(beam_deg: f64, kind: AntennaKind) -> Vec<f64> {
    antenna::lobes(beam_deg, kind)
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn beam_from_wire(wire_deg: f64) -> f64 {
    antenna::beam_from_wire(wire_deg)
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn wire_from_beam(beam_deg: f64) -> f64 {
    antenna::wire_from_beam(beam_deg)
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn nearest_lobe(beam_deg: f64, kind: AntennaKind, path_deg: f64) -> f64 {
    antenna::nearest_lobe(beam_deg, kind, path_deg)
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn separation_deg(a: f64, b: f64) -> f64 {
    antenna::separation_deg(a, b)
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn effective_height_m(antenna: antenna::Antenna) -> f64 {
    antenna.effective_height_m()
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn quality_of(reliability: f64) -> Quality {
    Quality::of(reliability)
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn quality_thresholds() -> Vec<f64> {
    vec![quality::RELIABLE_AT, quality::REACHABLE, quality::WEAK_AT]
}

#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn bearing_between(from_lat: f64, from_lon: f64, to_lat: f64, to_lon: f64) -> f64 {
    geo::bearing_deg(
        geo::LatLon {
            lat: from_lat,
            lon: from_lon,
        },
        geo::LatLon {
            lat: to_lat,
            lon: to_lon,
        },
    )
}

/// First and last month of the built-in sunspot table, `YYYY-MM`.
#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn ssn_table_range() -> Vec<String> {
    let (a, b) = crate::ssn::range();
    vec![a, b]
}

/// The map view zoomed by `factor` about a pixel, as a pinch does.
#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn view_zoomed(
    view: crate::projection::MapView,
    factor: f64,
    x: f64,
    y: f64,
    size: f64,
) -> crate::projection::MapView {
    view.zoomed_at(factor, x, y, size)
}

/// The map view moved by a drag.
#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn view_panned(
    view: crate::projection::MapView,
    dx: f64,
    dy: f64,
    size: f64,
) -> crate::projection::MapView {
    view.panned(dx, dy, size)
}

/// Where a first run works from until a place is chosen.
#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn greenwich() -> crate::places::Place {
    crate::places::Place::greenwich()
}

/// The colours of the path and the selected cell for a platform accent
/// (0xAARRGGBB): yellow unless the accent is yellow or orange.
#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn highlight_colors(accent: u32, dark: bool, low_light: bool) -> crate::render::Highlight {
    crate::render::highlight(accent, dark, low_light)
}

/// The turn from a heading to a bearing, degrees: negative to the left,
/// positive to the right, within half a turn.
#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn turn_deg(heading: f64, target: f64) -> f64 {
    let d = (target - heading).rem_euclid(360.0);
    if d > 180.0 { d - 360.0 } else { d }
}

/// The long path's bearing: the short one turned round.
#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn long_path_deg(bearing: f64) -> f64 {
    (bearing + 180.0).rem_euclid(360.0)
}

/// The local clock ("HH:MM") at a place and moment: legal time with summer
/// time, nautical time at sea.
#[cfg_attr(feature = "ffi", uniffi::export)]
pub fn local_clock(lat: f64, lon: f64, unix: i64) -> String {
    crate::localtime::clock(lat, lon, unix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_take_the_short_way() {
        assert_eq!(turn_deg(350.0, 10.0), 20.0);
        assert_eq!(turn_deg(10.0, 350.0), -20.0);
        assert_eq!(turn_deg(0.0, 180.0), 180.0);
        assert_eq!(long_path_deg(300.0), 120.0);
    }
}
