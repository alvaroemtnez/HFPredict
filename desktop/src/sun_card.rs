//! The sun today (live readings, when allowed) and what the forecast
//! assumes.

use std::rc::Rc;

use adw::prelude::*;
use hfpredict_core::engine::Model;
use hfpredict_core::ssn::Basis;

use crate::app::{App, Change};
use crate::format;
use crate::i18n::{tr, trf};

pub struct SunCard {
    pub root: gtk::Box,
    expander: adw::ExpanderRow,
    flux: adw::ActionRow,
    kp: adw::ActionRow,
    essn: adw::ActionRow,
    flux_value: gtk::Label,
    kp_value: gtk::Label,
    essn_value: gtk::Label,
    sounding: adw::ActionRow,
    empty: adw::ActionRow,
    basis_title: gtk::Label,
    basis_body: gtk::Label,
}

fn value_label() -> gtk::Label {
    let l = gtk::Label::new(None);
    l.add_css_class("title-3");
    l.add_css_class("numeric");
    l
}

impl SunCard {
    pub fn new(app: &Rc<App>) -> Rc<SunCard> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 18);

        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        let expander = adw::ExpanderRow::builder()
            .title(tr("sections.sun"))
            .build();
        list.append(&expander);

        let row = |title: &str, subtitle: &str| {
            let r = adw::ActionRow::builder()
                .title(title)
                .subtitle(subtitle)
                .build();
            let value = value_label();
            r.add_suffix(&value);
            expander.add_row(&r);
            (r, value)
        };
        let (flux, flux_value) = row(&tr("spaceWeather.flux"), &tr("spaceWeather.fluxHint"));
        let (kp, kp_value) = row(&tr("spaceWeather.kp"), "");
        let (essn, essn_value) = row(
            &tr("spaceWeather.effectiveSsn"),
            &tr("spaceWeather.effectiveSsnHint"),
        );
        let sounding = adw::ActionRow::new();
        sounding.set_subtitle_lines(3);
        expander.add_row(&sounding);
        let empty = adw::ActionRow::new();
        empty.set_title_lines(4);
        expander.add_row(&empty);
        root.append(&list);

        let basis = gtk::Box::new(gtk::Orientation::Vertical, 6);
        basis.add_css_class("card");
        basis.add_css_class("hfp-card");
        let basis_title = gtk::Label::builder().xalign(0.0).wrap(true).build();
        basis_title.add_css_class("heading");
        let basis_body = gtk::Label::builder().xalign(0.0).wrap(true).build();
        basis_body.add_css_class("dim-label");
        let caveat = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .label(tr("basis.caveat"))
            .build();
        caveat.add_css_class("dim-label");
        caveat.add_css_class("caption");
        basis.append(&basis_title);
        basis.append(&basis_body);
        basis.append(&caveat);
        root.append(&basis);

        let card = Rc::new(SunCard {
            root,
            expander,
            flux,
            kp,
            essn,
            flux_value,
            kp_value,
            essn_value,
            sounding,
            empty,
            basis_title,
            basis_body,
        });
        let me = Rc::downgrade(&card);
        app.listen(move |app, change| {
            if let Some(me) = me.upgrade()
                && matches!(
                    change,
                    Change::Saved | Change::Readings | Change::Forecast | Change::Clock
                )
            {
                me.update(app);
            }
        });
        card.update(app);
        card
    }

    fn update(&self, app: &App) {
        let v = app.view.borrow();
        let s = &v.saved;
        let fresh = s.space_weather.as_ref().filter(|sw| sw.is_fresh(v.now));
        match (&s.space_weather, s.settings.network) {
            (Some(sw), _) => {
                let tag = if fresh.is_some() {
                    "spaceWeather.updated"
                } else {
                    "spaceWeather.asOf"
                };
                self.expander
                    .set_subtitle(&trf(tag, &[("time", &format::hm(sw.fetched_at))]));
                self.flux_value.set_text(&format::int(sw.f107));
                self.kp_value.set_text(&format::dec(sw.kp, 1));
                let kp_hint = if fresh.is_none() {
                    tr("spaceWeather.stale")
                } else if sw.kp_max_24h >= 5.0 {
                    tr("spaceWeather.kpStormy")
                } else if sw.kp_max_24h >= 4.0 {
                    tr("spaceWeather.kpUnsettled")
                } else {
                    tr("spaceWeather.kpQuiet")
                };
                self.kp.set_subtitle(&format!(
                    "{kp_hint}\n{}",
                    trf(
                        "spaceWeather.kpMax",
                        &[("kp", &format::dec(sw.kp_max_24h, 1))]
                    )
                ));
                self.essn_value.set_text(&format::int(sw.effective_ssn));
                for r in [&self.flux, &self.kp, &self.essn] {
                    r.set_visible(true);
                }
                self.empty.set_visible(false);
            }
            (None, network) => {
                for r in [&self.flux, &self.kp, &self.essn] {
                    r.set_visible(false);
                }
                self.empty.set_title(&if network {
                    tr("spaceWeather.unavailable")
                } else {
                    tr("spaceWeather.networkOff")
                });
                self.empty.set_visible(true);
                self.expander.set_subtitle("");
            }
        }
        match &s.sounding {
            Some(snd) => {
                self.sounding.set_title(&trf(
                    "spaceWeather.measuredTitle",
                    &[("value", &format::mhz(snd.fof2))],
                ));
                self.sounding.set_subtitle(&format!(
                    "{}\n{}",
                    trf(
                        "spaceWeather.measuredSub",
                        &[
                            ("station", &format!("{} ({})", snd.station, snd.ursi)),
                            ("distance", &format::distance(snd.km, v.imperial())),
                            ("time", &format::hm(snd.measured_at))
                        ]
                    ),
                    tr("spaceWeather.measuredNote")
                ));
                self.sounding.set_visible(true);
            }
            None => self.sounding.set_visible(false),
        }

        let model = s.settings.model;
        let (ssn, basis) = v
            .forecast
            .as_ref()
            .map_or((0.0, Basis::Climatology), |f| (f.ssn, f.basis));
        let key = match (model, basis) {
            (Model::Truecast, Basis::Nowcast) => "basis.truecastNowcast",
            (Model::Truecast, _) => "basis.truecastOffline",
            (Model::Voacap, Basis::Nowcast) => "basis.voacapNowcast",
            (Model::Voacap, Basis::Forecast) => "basis.voacapForecast",
            (Model::Voacap, Basis::Climatology) => "basis.voacapClimatology",
        };
        self.basis_title.set_text(&tr(&format!("{key}.title")));
        self.basis_body
            .set_text(&trf(&format!("{key}.body"), &[("ssn", &format::int(ssn))]));
    }
}
