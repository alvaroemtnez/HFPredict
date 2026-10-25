//! The app's text, from the same tables the Android app is generated from
//! (`i18n/*.json`). English is the fallback for anything missing.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Languages with a table, in the order Preferences lists them, and their
/// own names.
pub const LANGUAGES: [(&str, &str); 6] = [
    ("en", "English"),
    ("es", "Español"),
    ("ca", "Valencià"),
    ("de", "Deutsch"),
    ("ja", "日本語"),
    ("ar", "العربية"),
];

fn table(lang: &str) -> &'static str {
    match lang {
        "es" => include_str!("../../i18n/es.json"),
        "ca" => include_str!("../../i18n/ca.json"),
        "de" => include_str!("../../i18n/de.json"),
        "ja" => include_str!("../../i18n/ja.json"),
        "ar" => include_str!("../../i18n/ar.json"),
        _ => include_str!("../../i18n/en.json"),
    }
}

struct Strings {
    lang: String,
    chosen: HashMap<String, String>,
    english: HashMap<String, String>,
}

static STRINGS: OnceLock<Strings> = OnceLock::new();

fn parse(text: &str) -> HashMap<String, String> {
    serde_json::from_str(text).unwrap_or_default()
}

/// The language to show: the saved choice, else the first of the system's
/// preferred languages that has a table, else English.
pub fn resolve(choice: Option<&str>) -> String {
    if let Some(c) = choice.filter(|c| LANGUAGES.iter().any(|(code, _)| code == c)) {
        return c.to_string();
    }
    for name in gtk::glib::language_names() {
        let code = name.split(['_', '.', '@', '-']).next().unwrap_or("");
        // Valencian is filed under Catalan in locales (ca_ES@valencia).
        if let Some((code, _)) = LANGUAGES.iter().find(|(c, _)| *c == code) {
            return code.to_string();
        }
    }
    "en".into()
}

/// Loads the tables once, at start-up. A language change shows after a
/// restart, as GNOME apps do.
pub fn init(lang: &str) {
    let _ = STRINGS.set(Strings {
        lang: lang.to_string(),
        chosen: parse(table(lang)),
        english: parse(table("en")),
    });
}

pub fn lang() -> &'static str {
    STRINGS.get().map_or("en", |s| s.lang.as_str())
}

/// Right-to-left script (Arabic).
pub fn is_rtl() -> bool {
    lang() == "ar"
}

/// The text for a key, or the key itself when no table has it (so a
/// missing string is visible rather than blank).
pub fn tr(key: &str) -> String {
    let Some(s) = STRINGS.get() else {
        return key.to_string();
    };
    s.chosen
        .get(key)
        .or_else(|| s.english.get(key))
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

/// `tr` with `{name}` placeholders filled in.
pub fn trf(key: &str, args: &[(&str, &str)]) -> String {
    let mut text = tr(key);
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// The language code a place search should ask Open-Meteo for.
pub fn search_language() -> &'static str {
    lang()
}
