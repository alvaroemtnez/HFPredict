//! Preferences, Help and About.

use std::rc::Rc;

use adw::prelude::*;
use hfpredict_core::engine::Model;
use hfpredict_core::store::{Theme, Units};

use crate::format;
use crate::i18n::{self, tr, trf};
use crate::window::Window;

pub const APP_ID: &str = "io.alvaroedo.HFPredict";
pub const WEBSITE: &str = "https://code.alvaroedo.io/alvaroemtnez/HFPredict";
pub const ISSUES: &str = "https://code.alvaroedo.io/alvaroemtnez/HFPredict/issues";

pub fn preferences(w: &Rc<Window>) {
    let app = w.app.clone();
    let settings = app.view.borrow().saved.settings.clone();
    let dialog = adw::PreferencesDialog::builder()
        .title(tr("preferences.title"))
        .build();
    let page = adw::PreferencesPage::new();

    let g = adw::PreferencesGroup::builder()
        .title(tr("preferences.appearance"))
        .build();
    let themes = [Theme::System, Theme::Light, Theme::Dark, Theme::LowLight];
    let theme_names: Vec<String> = ["system", "light", "dark", "lowLight"]
        .iter()
        .map(|k| tr(&format!("preferences.theme.{k}")))
        .collect();
    let theme = adw::ComboRow::builder()
        .title(tr("preferences.theme"))
        .model(&gtk::StringList::new(
            &theme_names.iter().map(String::as_str).collect::<Vec<_>>(),
        ))
        .selected(
            themes
                .iter()
                .position(|t| *t == settings.theme)
                .unwrap_or(0) as u32,
        )
        .build();
    // The hint explains low light, so it shows only with low light picked.
    let hint = |row: &adw::ComboRow| {
        row.set_subtitle(&if row.selected() == 3 {
            tr("preferences.theme.lowLightHint")
        } else {
            String::new()
        });
    };
    hint(&theme);
    theme.connect_selected_notify(hint);
    g.add(&theme);

    let mut langs: Vec<String> = vec![tr("preferences.language.system")];
    langs.extend(i18n::LANGUAGES.iter().map(|(_, name)| name.to_string()));
    let lang_index = settings
        .language
        .as_deref()
        .and_then(|l| i18n::LANGUAGES.iter().position(|(c, _)| *c == l))
        .map_or(0, |i| i + 1);
    let language = adw::ComboRow::builder()
        .title(tr("preferences.language"))
        .model(&gtk::StringList::new(
            &langs.iter().map(String::as_str).collect::<Vec<_>>(),
        ))
        .selected(lang_index as u32)
        .build();
    g.add(&language);

    let unit_values = [Units::Auto, Units::Metric, Units::Imperial];
    let unit_names: Vec<String> = ["auto", "metric", "imperial"]
        .iter()
        .map(|k| tr(&format!("preferences.units.{k}")))
        .collect();
    let units = adw::ComboRow::builder()
        .title(tr("preferences.units"))
        .model(&gtk::StringList::new(
            &unit_names.iter().map(String::as_str).collect::<Vec<_>>(),
        ))
        .selected(
            unit_values
                .iter()
                .position(|u| *u == settings.units)
                .unwrap_or(0) as u32,
        )
        .build();
    g.add(&units);
    page.add(&g);

    let g = adw::PreferencesGroup::builder()
        .title(tr("preferences.model"))
        .build();
    let truecast = gtk::CheckButton::new();
    let voacap = gtk::CheckButton::new();
    voacap.set_group(Some(&truecast));
    truecast.set_active(settings.model == Model::Truecast);
    voacap.set_active(settings.model == Model::Voacap);
    for (button, title, hint) in [
        (
            &truecast,
            "preferences.model.truecast",
            "preferences.model.truecastHint",
        ),
        (
            &voacap,
            "preferences.model.voacap",
            "preferences.model.voacapHint",
        ),
    ] {
        let row = adw::ActionRow::builder()
            .title(tr(title))
            .subtitle(tr(hint))
            .activatable_widget(button)
            .build();
        row.add_prefix(button);
        g.add(&row);
    }
    page.add(&g);

    let g = adw::PreferencesGroup::builder()
        .title(tr("preferences.privacy"))
        .build();
    let network = adw::SwitchRow::builder()
        .title(tr("preferences.network"))
        .subtitle(tr("preferences.networkHint"))
        .active(settings.network)
        .build();
    g.add(&network);
    page.add(&g);

    let g = adw::PreferencesGroup::builder()
        .title(tr("preferences.storage"))
        .build();
    let keep = adw::SwitchRow::builder()
        .title(tr("preferences.keepMaps"))
        .subtitle(tr("preferences.keepMapsHint"))
        .active(settings.keep_maps)
        .build();
    g.add(&keep);
    let stored = adw::ActionRow::builder()
        .title(trf(
            "preferences.stored",
            &[("size", &format::megabytes(app.core.stored_maps_bytes()))],
        ))
        .build();
    let forget = gtk::Button::builder()
        .label(tr("preferences.forget"))
        .valign(gtk::Align::Center)
        .build();
    forget.add_css_class("destructive-action");
    stored.add_suffix(&forget);
    g.add(&stored);
    page.add(&g);
    dialog.add(&page);

    let apply = {
        let app = app.clone();
        let (theme, language, units, truecast, network, keep) = (
            theme.clone(),
            language.clone(),
            units.clone(),
            truecast.clone(),
            network.clone(),
            keep.clone(),
        );
        let w = Rc::downgrade(w);
        move || {
            let before = app.view.borrow().saved.settings.clone();
            let mut s = before.clone();
            s.theme = themes[theme.selected() as usize % themes.len()];
            s.units = unit_values[units.selected() as usize % unit_values.len()];
            s.language = match language.selected() as usize {
                0 => None,
                i => i18n::LANGUAGES.get(i - 1).map(|(c, _)| c.to_string()),
            };
            s.model = if truecast.is_active() {
                Model::Truecast
            } else {
                Model::Voacap
            };
            s.network = network.is_active();
            s.keep_maps = keep.is_active();
            if s == before {
                return;
            }
            let language_changed = s.language != before.language;
            let network_on = s.network && !before.network;
            apply_theme(s.theme);
            app.set_settings(s);
            if network_on {
                app.refresh_readings();
            }
            if language_changed && let Some(w) = w.upgrade() {
                w.toast(&tr("preferences.languageRestart"));
            }
        }
    };
    for row in [&theme, &language, &units] {
        let apply = apply.clone();
        row.connect_selected_notify(move |_| apply());
    }
    {
        let apply = apply.clone();
        truecast.connect_toggled(move |_| apply());
    }
    for row in [&network, &keep] {
        let apply = apply.clone();
        row.connect_active_notify(move |_| apply());
    }
    {
        let app = app.clone();
        let stored = stored.clone();
        let dialog = dialog.clone();
        forget.connect_clicked(move |_| {
            app.core.clear_stored_maps();
            stored.set_title(&trf(
                "preferences.stored",
                &[("size", &format::megabytes(app.core.stored_maps_bytes()))],
            ));
            dialog.add_toast(adw::Toast::new(&tr("preferences.forgotten")));
        });
    }
    dialog.present(Some(&w.window));
}

/// Light, dark, or the desktop's choice. Low light is dark plus the red
/// stylesheet (see main.rs).
pub fn apply_theme(theme: Theme) {
    let sm = adw::StyleManager::default();
    sm.set_color_scheme(match theme {
        Theme::System => adw::ColorScheme::Default,
        Theme::Light => adw::ColorScheme::ForceLight,
        Theme::Dark | Theme::LowLight => adw::ColorScheme::ForceDark,
    });
    crate::set_low_light(theme == Theme::LowLight);
}

pub fn help(w: &Rc<Window>) {
    let (first, last) = hfpredict_core::ssn::range();
    let dialog = adw::Dialog::builder()
        .title(tr("help.title"))
        .content_width(640)
        .content_height(760)
        .build();
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.set_margin_top(12);
    body.set_margin_bottom(24);
    body.set_margin_start(18);
    body.set_margin_end(18);
    let pct = |v: f64| format::int(v * 100.0);
    let sections: Vec<(String, String)> = vec![
        ("help.engine".into(), tr("help.engine.body")),
        ("help.truecast".into(), tr("help.truecast.body")),
        (
            "help.ssn".into(),
            trf(
                "help.ssn.body",
                &[("first", &first), ("last", &last), ("hours", "24")],
            ),
        ),
        (
            "help.colours".into(),
            trf(
                "help.colours.body",
                &[
                    ("reliable", &pct(hfpredict_core::quality::RELIABLE_AT)),
                    ("patchy", &pct(hfpredict_core::quality::REACHABLE)),
                    ("weak", &pct(hfpredict_core::quality::WEAK_AT)),
                ],
            ),
        ),
        (
            "help.map".into(),
            trf("help.map.body", &[("points", &format::int(34_560.0))]),
        ),
        (
            "help.nvis".into(),
            trf(
                "help.nvis.body",
                &[(
                    "degrees",
                    &format::int(hfpredict_core::quality::NVIS_MIN_ANGLE_DEG),
                )],
            ),
        ),
        (
            "help.invertedV".into(),
            trf(
                "help.invertedV.body",
                &[(
                    "percent",
                    &pct(hfpredict_core::antenna::INVERTED_V_HEIGHT_FRACTION),
                )],
            ),
        ),
        ("help.privacy".into(), tr("help.privacy.body")),
    ];
    for (key, text) in sections {
        let h = gtk::Label::builder()
            .label(tr(&format!("{key}.heading")))
            .xalign(0.0)
            .wrap(true)
            .margin_top(14)
            .build();
        h.add_css_class("title-4");
        body.append(&h);
        let b = gtk::Label::builder()
            .label(text)
            .xalign(0.0)
            .wrap(true)
            .selectable(true)
            .build();
        b.add_css_class("body");
        body.append(&b);
    }
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&adw::Clamp::builder().maximum_size(640).child(&body).build())
        .vexpand(true)
        .build();
    view.set_content(Some(&scroll));
    dialog.set_child(Some(&view));
    dialog.present(Some(&w.window));
}

pub fn about(w: &Rc<Window>) {
    let about = adw::AboutDialog::builder()
        .application_name(tr("app.name"))
        .application_icon(APP_ID)
        .developer_name("Álvaro Edo Martínez")
        .version(env!("CARGO_PKG_VERSION"))
        .website(WEBSITE)
        .issue_url(ISSUES)
        .comments(tr("about.description"))
        .license_type(gtk::License::Gpl30)
        .copyright("© 2026 Álvaro Edo Martínez")
        .build();
    about.add_credit_section(Some(&tr("about.builtOn")), &[
        "Jonathan McSweet, HFcast and the HFcast engine https://github.com/jonathanmcsweet/hfcast-engine",
        "NTIA/ITS, VOACAP (Greg Hand, maintainer) https://its.ntia.gov/",
        "J.A. Watson, voacapl https://github.com/jawatson/voacapl",
    ]);
    about.add_credit_section(Some(&tr("about.dataAndServices")), &[
        "ITU-R Study Group 3, CCIR coefficients https://github.com/ITU-R-Study-Group-3/ITU-R-HF",
        "Natural Earth https://www.naturalearthdata.com/",
        "NOAA Space Weather Prediction Center https://www.swpc.noaa.gov/",
        "GIRO, UMass Lowell https://giro.uml.edu/",
        "Open-Meteo (CC BY 4.0) https://open-meteo.com/",
    ]);
    about.add_legal_section(
        "HFcast engine",
        Some("© 2026 Jonathan McSweet"),
        gtk::License::Apache20,
        None,
    );
    about.add_legal_section(
        "HFcast",
        Some("© 2026 Jonathan McSweet"),
        gtk::License::Apache20,
        Some("Parts of HFPredict are translated from HFcast. See NOTICE."),
    );
    about.add_legal_section(
        "VOACAP",
        None,
        gtk::License::Custom,
        Some(&format!(
            "{}\n\n{}",
            "Not subject to copyright protection in the United States (NTIA/ITS).",
            tr("about.endorsement")
        )),
    );
    about.add_legal_section(
        "voacapl",
        Some("J.A. Watson"),
        gtk::License::Custom,
        Some("CC0 1.0 Universal"),
    );
    about.add_legal_section(
        "Open-Meteo",
        None,
        gtk::License::Custom,
        Some("Place search data under CC BY 4.0. https://open-meteo.com/"),
    );
    about.add_legal_section("GIRO", None, gtk::License::Custom, Some("Ionosonde data from the Lowell GIRO Data Center under CC BY-NC-SA 4.0; each sounding names its station."));
    about.present(Some(&w.window));
}
