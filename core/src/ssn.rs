//! The built-in monthly sunspot table (`data/ssn.json`, refreshed from NOAA
//! by `tools/update-ssn.py`). It is what makes a VOACAP forecast possible
//! with no network.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Where the sunspot number driving a run came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum Basis {
    /// Observed smoothed SSN for the month.
    Climatology,
    /// SSN from current solar and geomagnetic readings.
    Nowcast,
    /// Predicted smoothed SSN for a month not yet observed.
    Forecast,
}

#[derive(Deserialize)]
struct Table {
    fetched: String,
    months: BTreeMap<String, (f64, String)>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str(include_str!("../data/ssn.json")).expect("data/ssn.json is valid")
    })
}

/// The month's sunspot number and whether it was observed or predicted.
/// Outside the table the nearest end stands in, as a forecast.
pub fn for_month(year: i32, month: u32) -> (f64, Basis) {
    let t = table();
    let tag = format!("{year:04}-{month:02}");
    if let Some((ssn, kind)) = t.months.get(&tag) {
        let basis = if kind == "c" { Basis::Climatology } else { Basis::Forecast };
        return (*ssn, basis);
    }
    let nearest = if Some(&tag) < t.months.keys().next() {
        t.months.values().next()
    } else {
        t.months.values().next_back()
    };
    (nearest.map_or(0.0, |(ssn, _)| *ssn), Basis::Forecast)
}

/// First and last month in the table, `YYYY-MM`.
pub fn range() -> (String, String) {
    let t = table();
    let first = t.months.keys().next().cloned().unwrap_or_default();
    let last = t.months.keys().next_back().cloned().unwrap_or_default();
    (first, last)
}

/// When the table was fetched from NOAA, `YYYY-MM-DD`.
pub fn fetched() -> String {
    table().fetched.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups() {
        let (ssn, basis) = for_month(2020, 1);
        assert!(ssn >= 0.0);
        assert_eq!(basis, Basis::Climatology);
        assert_eq!(for_month(1999, 1).1, Basis::Forecast);
        assert_eq!(for_month(2099, 1).1, Basis::Forecast);
        let (first, last) = range();
        assert!(first < last);
    }
}
