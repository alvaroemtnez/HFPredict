//! Numbers, distances and times the way the reader's language writes them.

use hfpredict_core::quality::Quality;

use crate::i18n::{self, tr, trf};

fn separators() -> (char, char) {
    match i18n::lang() {
        "es" | "ca" | "de" => (',', '.'),
        _ => ('.', ','),
    }
}

/// An integer with thousands grouped: 10,520 or 10.520. Spanish and
/// Valencian leave four-digit numbers ungrouped.
pub fn int(v: f64) -> String {
    let n = v.round() as i64;
    let digits = n.unsigned_abs().to_string();
    let (_, group) = separators();
    let grouped = if digits.len() <= 4 && matches!(i18n::lang(), "es" | "ca") {
        digits
    } else {
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(group);
            }
            out.push(c);
        }
        out
    };
    if n < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

/// A decimal with `places` digits after the point, trailing zeros kept.
pub fn dec(v: f64, places: usize) -> String {
    let (point, _) = separators();
    let s = format!("{v:.places$}");
    if point == '.' {
        s
    } else {
        s.replace('.', &point.to_string())
    }
}

/// "81%" (or "81 %" where the language spaces it).
pub fn percent(share: f64) -> String {
    let n = int(share * 100.0);
    match i18n::lang() {
        "es" | "ca" | "de" => format!("{n} %"),
        _ => format!("{n}%"),
    }
}

pub fn distance(km: f64, imperial: bool) -> String {
    let (value, key) = if imperial {
        (km / 1.609_344, "units.miles")
    } else {
        (km, "units.kilometres")
    };
    // Round to something a reader can hold: tens below 1,000, else hundreds.
    let rounded = if value < 100.0 {
        (value / 5.0).round() * 5.0
    } else if value < 1000.0 {
        (value / 10.0).round() * 10.0
    } else {
        (value / 100.0).round() * 100.0
    };
    trf(key, &[("value", &int(rounded))])
}

pub fn height(m: f64, imperial: bool) -> String {
    if imperial {
        trf("units.feet", &[("value", &int(m / 0.3048))])
    } else {
        trf("units.metres", &[("value", &int(m))])
    }
}

pub fn watts(w: f64) -> String {
    let v = if w < 1.0 || (w < 10.0 && w.fract() != 0.0) {
        dec(w, 1)
    } else {
        int(w)
    };
    trf("units.watts", &[("value", &v)])
}

pub fn mhz(v: f64) -> String {
    trf("units.megahertz", &[("value", &dec(v, 1))])
}

pub fn hour(h: u32) -> String {
    format!("{:02}:00", h % 24)
}

pub fn hm(unix: i64) -> String {
    let s = unix.rem_euclid(86_400);
    format!("{:02}:{:02}", s / 3600, (s % 3600) / 60)
}

pub fn quality(q: Quality) -> String {
    tr(&format!("quality.{}", q.key()))
}

pub fn degrees(d: f64) -> String {
    int(d.rem_euclid(360.0))
}

/// Bytes as MB, for the stored-maps line.
pub fn megabytes(bytes: u64) -> String {
    format!("{} MB", dec(bytes as f64 / 1_048_576.0, 1))
}
