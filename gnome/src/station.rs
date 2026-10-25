//! "Your radio": the station editor.
//!
//! Edits a draft; Save writes it and recomputes, Cancel (or closing) with
//! unsaved changes asks first.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use hfpredict_core::antenna::{
    self, AntennaKind, INVERTED_V_HEIGHT_FRACTION, MAX_GAIN_DBD, MAX_HEIGHT_M, MAX_WATTS,
    MIN_GAIN_DBD, MIN_HEIGHT_M, MIN_WATTS,
};
use hfpredict_core::geo;
use hfpredict_core::modes::Mode;
use hfpredict_core::station::{MAX_NAME_LENGTH, Station};

use crate::app::App;
use crate::canvas::{self, Canvas};
use crate::format;
use crate::i18n::{tr, trf};
use crate::window::{Window, antenna_key};

const FEET: f64 = 0.3048;

struct Editor {
    app: Rc<App>,
    draft: RefCell<Station>,
    saved: RefCell<Station>,
    /// Set while the code fills the controls, so their signals are ignored.
    filling: Cell<bool>,
    imperial: bool,
    stations: adw::ComboRow,
    name: adw::EntryRow,
    delete: adw::ButtonRow,
    mode_buttons: Vec<(Mode, gtk::ToggleButton)>,
    mode_note: gtk::Label,
    watts: adw::SpinRow,
    antenna_buttons: Vec<(AntennaKind, gtk::ToggleButton)>,
    antenna_note: gtk::Label,
    height: adw::SpinRow,
    gain: adw::SpinRow,
    direction_group: adw::PreferencesGroup,
    direction: adw::SpinRow,
    compass: Canvas,
    favours: gtk::Label,
    aim: gtk::Label,
    align: gtk::Button,
}

fn chips<T: Copy + 'static>(items: &[(T, String)]) -> (gtk::FlowBox, Vec<(T, gtk::ToggleButton)>) {
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .column_spacing(6)
        .row_spacing(6)
        .homogeneous(false)
        .max_children_per_line(12)
        .build();
    let mut group: Option<gtk::ToggleButton> = None;
    let mut out = vec![];
    for (value, label) in items {
        let b = gtk::ToggleButton::with_label(label);
        b.add_css_class("hfp-chip-toggle");
        if let Some(g) = &group {
            b.set_group(Some(g));
        } else {
            group = Some(b.clone());
        }
        flow.append(&b);
        out.push((*value, b));
    }
    (flow, out)
}

fn note() -> gtk::Label {
    let l = gtk::Label::builder().wrap(true).xalign(0.0).build();
    l.add_css_class("dim-label");
    l.set_margin_top(6);
    l
}

pub fn open(w: &Rc<Window>) {
    let app = w.app.clone();
    let station = app.view.borrow().saved.station();
    let imperial = app.view.borrow().imperial();

    let dialog = adw::Dialog::builder()
        .title(tr("station.title"))
        .content_width(560)
        .content_height(760)
        .build();
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .show_start_title_buttons(false)
        .build();
    let cancel = gtk::Button::with_label(&tr("common.cancel"));
    let save = gtk::Button::with_label(&tr("common.save"));
    save.add_css_class("suggested-action");
    header.pack_start(&cancel);
    header.pack_end(&save);
    view.add_top_bar(&header);

    let page = adw::PreferencesPage::new();

    // Station: which, and its name.
    let g = adw::PreferencesGroup::builder()
        .title(tr("station.pickSection"))
        .build();
    let names = gtk::StringList::new(&[]);
    let stations = adw::ComboRow::builder()
        .title(tr("station.pickSection"))
        .model(&names)
        .build();
    let name = adw::EntryRow::builder()
        .title(tr("station.nameSection"))
        .build();
    name.set_max_length(MAX_NAME_LENGTH as i32);
    let add = adw::ButtonRow::builder()
        .title(tr("station.add"))
        .start_icon_name("list-add-symbolic")
        .build();
    add.set_tooltip_text(Some(&tr("station.addHint")));
    let delete = adw::ButtonRow::builder()
        .title(tr("station.remove"))
        .start_icon_name("user-trash-symbolic")
        .build();
    delete.add_css_class("destructive-action");
    g.add(&stations);
    g.add(&name);
    g.add(&add);
    g.add(&delete);
    page.add(&g);

    // Mode.
    let g = adw::PreferencesGroup::builder()
        .title(tr("station.modeSection"))
        .description(tr("station.modeHint"))
        .build();
    let (mode_flow, mode_buttons) = chips(
        &Mode::ORDER
            .iter()
            .map(|m| (*m, tr(&format!("station.mode.{}", m.key()))))
            .collect::<Vec<_>>(),
    );
    let mode_note = note();
    let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
    b.append(&mode_flow);
    b.append(&mode_note);
    g.add(&b);
    page.add(&g);

    // Power.
    let g = adw::PreferencesGroup::builder()
        .title(tr("station.powerSection"))
        .build();
    let watts = adw::SpinRow::builder()
        .title(tr("units.watts").replace("{value}", "").trim().to_string())
        .subtitle(trf(
            "station.powerRange",
            &[
                ("min", &format::dec(MIN_WATTS, 1)),
                ("max", &format::int(MAX_WATTS)),
            ],
        ))
        .adjustment(&gtk::Adjustment::new(
            100.0, MIN_WATTS, MAX_WATTS, 1.0, 10.0, 0.0,
        ))
        .digits(1)
        .build();
    watts.set_title(&tr("station.powerSection"));
    g.add(&watts);
    page.add(&g);

    // Antenna.
    let g = adw::PreferencesGroup::builder()
        .title(tr("station.antennaSection"))
        .build();
    let (antenna_flow, antenna_buttons) = chips(
        &AntennaKind::ORDER
            .iter()
            .map(|k| (*k, tr(&format!("station.antenna.{}", antenna_key(*k)))))
            .collect::<Vec<_>>(),
    );
    let antenna_note = note();
    let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
    b.set_margin_bottom(12);
    b.append(&antenna_flow);
    b.append(&antenna_note);
    g.add(&b);
    let (hmin, hmax) = if imperial {
        ((MIN_HEIGHT_M / FEET).ceil(), (MAX_HEIGHT_M / FEET).floor())
    } else {
        (MIN_HEIGHT_M, MAX_HEIGHT_M)
    };
    let height = adw::SpinRow::builder()
        .title(tr("station.height"))
        .subtitle(trf(
            "station.heightRange",
            &[
                ("min", &format::height(MIN_HEIGHT_M, imperial)),
                ("max", &format::height(MAX_HEIGHT_M, imperial)),
            ],
        ))
        .adjustment(&gtk::Adjustment::new(10.0, hmin, hmax, 1.0, 5.0, 0.0))
        .build();
    let gain = adw::SpinRow::builder()
        .title(tr("station.gain"))
        .adjustment(&gtk::Adjustment::new(
            6.0,
            MIN_GAIN_DBD,
            MAX_GAIN_DBD,
            0.5,
            1.0,
            0.0,
        ))
        .digits(1)
        .build();
    g.add(&height);
    g.add(&gain);
    page.add(&g);

    // Direction.
    let direction_group = adw::PreferencesGroup::builder()
        .title(tr("station.beam"))
        .build();
    let direction = adw::SpinRow::builder()
        .title(tr("station.beam"))
        .adjustment(&gtk::Adjustment::new(0.0, 0.0, 359.0, 5.0, 15.0, 0.0))
        .wrap(true)
        .build();
    direction_group.add(&direction);
    let compass = Canvas::new();
    compass.set_widths(200, 240);
    compass.set_height_for_width(|w| w.min(240));
    compass.set_halign(gtk::Align::Center);
    compass.set_margin_top(12);
    let favours = note();
    favours.set_justify(gtk::Justification::Center);
    favours.set_xalign(0.5);
    let aim = note();
    let align = gtk::Button::new();
    align.add_css_class("pill");
    align.set_halign(gtk::Align::Center);
    align.set_margin_top(6);
    let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
    b.append(&compass);
    b.append(&favours);
    b.append(&aim);
    b.append(&align);
    direction_group.add(&b);
    page.add(&direction_group);

    view.set_content(Some(&page));
    dialog.set_child(Some(&view));

    let ed = Rc::new(Editor {
        app,
        draft: RefCell::new(station.clone()),
        saved: RefCell::new(station),
        filling: Cell::new(false),
        imperial,
        stations,
        name,
        delete,
        mode_buttons,
        mode_note,
        watts,
        antenna_buttons,
        antenna_note,
        height,
        gain,
        direction_group,
        direction,
        compass,
        favours,
        aim,
        align,
    });
    ed.wire(&dialog, &cancel, &save, &add, &names);
    ed.fill(&names);
    dialog.present(Some(&w.window));
}

impl Editor {
    fn dirty(&self) -> bool {
        *self.draft.borrow() != *self.saved.borrow()
    }

    fn change(self: &Rc<Self>, f: impl FnOnce(&mut Station)) {
        if self.filling.get() {
            return;
        }
        f(&mut self.draft.borrow_mut());
        self.refresh_notes();
    }

    fn wire(
        self: &Rc<Self>,
        dialog: &adw::Dialog,
        cancel: &gtk::Button,
        save: &gtk::Button,
        add: &adw::ButtonRow,
        names: &gtk::StringList,
    ) {
        for (mode, b) in &self.mode_buttons {
            let me = Rc::downgrade(self);
            let mode = *mode;
            b.connect_toggled(move |b| {
                if let Some(me) = me.upgrade()
                    && b.is_active()
                {
                    me.change(|s| s.mode = mode);
                }
            });
        }
        for (kind, b) in &self.antenna_buttons {
            let me = Rc::downgrade(self);
            let kind = *kind;
            b.connect_toggled(move |b| {
                if let Some(me) = me.upgrade()
                    && b.is_active()
                {
                    me.change(|s| s.antenna.kind = kind);
                    me.fill_direction();
                }
            });
        }
        let me = Rc::downgrade(self);
        self.watts.connect_value_notify(move |r| {
            if let Some(me) = me.upgrade() {
                me.change(|s| s.watts = r.value());
            }
        });
        let me = Rc::downgrade(self);
        self.height.connect_value_notify(move |r| {
            if let Some(me) = me.upgrade() {
                let m = if me.imperial {
                    r.value() * FEET
                } else {
                    r.value()
                };
                me.change(|s| s.antenna.height_m = m);
            }
        });
        let me = Rc::downgrade(self);
        self.gain.connect_value_notify(move |r| {
            if let Some(me) = me.upgrade() {
                me.change(|s| s.antenna.gain_dbd = r.value());
            }
        });
        let me = Rc::downgrade(self);
        self.direction.connect_value_notify(move |r| {
            if let Some(me) = me.upgrade() {
                // A wire is asked which way it runs; its beam is square to it.
                let asked = r.value();
                me.change(|s| {
                    s.antenna.beam_deg = if s.antenna.kind.asked_as_wire() {
                        antenna::beam_from_wire(asked)
                    } else {
                        asked
                    }
                });
            }
        });
        let me = Rc::downgrade(self);
        self.name.connect_changed(move |e| {
            if let Some(me) = me.upgrade() {
                let text = e.text().to_string();
                me.change(|s| s.name = text);
            }
        });
        let me = Rc::downgrade(self);
        self.align.connect_clicked(move |_| {
            if let Some(me) = me.upgrade()
                && let Some(bearing) = me.bearing_to_destination()
            {
                me.draft.borrow_mut().antenna.beam_deg = bearing.round().rem_euclid(360.0);
                me.fill_direction();
                me.refresh_notes();
            }
        });

        // Switching station: the draft is saved first if it changed.
        let me = Rc::downgrade(self);
        let names2 = names.clone();
        self.stations.connect_selected_notify(move |row| {
            let Some(me) = me.upgrade() else { return };
            if me.filling.get() {
                return;
            }
            let id = me
                .app
                .view
                .borrow()
                .saved
                .stations
                .get(row.selected() as usize)
                .map(|s| s.id.clone());
            if let Some(id) = id {
                me.commit();
                me.app.save(|c| c.select_station(id));
                let st = me.app.view.borrow().saved.station();
                *me.draft.borrow_mut() = st.clone();
                *me.saved.borrow_mut() = st;
                me.fill(&names2);
            }
        });
        let me = Rc::downgrade(self);
        let names2 = names.clone();
        add.connect_activated(move |_| {
            let Some(me) = me.upgrade() else { return };
            me.commit();
            let fresh = me.app.core.new_station();
            me.app.save(|c| c.put_station(fresh.clone()));
            *me.draft.borrow_mut() = fresh.clone();
            *me.saved.borrow_mut() = fresh;
            me.fill(&names2);
            me.name.grab_focus();
        });
        let me = Rc::downgrade(self);
        let names2 = names.clone();
        self.delete.connect_activated(move |_| {
            let Some(me) = me.upgrade() else { return };
            let id = me.draft.borrow().id.clone();
            me.app.save(|c| c.delete_station(id));
            let st = me.app.view.borrow().saved.station();
            *me.draft.borrow_mut() = st.clone();
            *me.saved.borrow_mut() = st;
            me.fill(&names2);
        });

        let me = Rc::downgrade(self);
        let d = dialog.clone();
        save.connect_clicked(move |_| {
            if let Some(me) = me.upgrade() {
                me.commit();
            }
            d.force_close();
        });
        let d = dialog.clone();
        cancel.connect_clicked(move |_| {
            d.close();
        });
        dialog.set_can_close(false);
        let me = Rc::downgrade(self);
        dialog.connect_close_attempt(move |d| {
            let Some(me) = me.upgrade() else {
                d.force_close();
                return;
            };
            if !me.dirty() {
                d.force_close();
                return;
            }
            let alert = adw::AlertDialog::new(
                Some(&tr("station.discardTitle")),
                Some(&tr("station.discardBody")),
            );
            alert.add_responses(&[
                ("keep", &tr("station.keepEditing")),
                ("discard", &tr("station.discard")),
            ]);
            alert.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
            alert.set_default_response(Some("keep"));
            alert.set_close_response("keep");
            let d2 = d.clone();
            alert.connect_response(None, move |_, r| {
                if r == "discard" {
                    d2.force_close();
                }
            });
            alert.present(Some(d));
        });
    }

    /// Saves the draft if it changed.
    fn commit(&self) {
        if self.dirty() {
            let st = self.draft.borrow().clone();
            self.app.save(|c| c.put_station(st.clone()));
            *self.saved.borrow_mut() = st;
        }
    }

    fn fill(&self, names: &gtk::StringList) {
        self.filling.set(true);
        let v = self.app.view.borrow();
        let list: Vec<String> = v
            .saved
            .stations
            .iter()
            .map(|s| {
                if s.name.is_empty() {
                    tr("station.unnamed")
                } else {
                    s.name.clone()
                }
            })
            .collect();
        names.splice(
            0,
            names.n_items(),
            &list.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        let st = self.draft.borrow().clone();
        let idx = v
            .saved
            .stations
            .iter()
            .position(|s| s.id == st.id)
            .unwrap_or(0);
        self.stations.set_selected(idx as u32);
        self.delete.set_visible(v.saved.stations.len() > 1);
        drop(v);
        self.name.set_text(&st.name);
        for (m, b) in &self.mode_buttons {
            b.set_active(*m == st.mode);
        }
        self.watts.set_value(st.watts);
        for (k, b) in &self.antenna_buttons {
            b.set_active(*k == st.antenna.kind);
        }
        self.height.set_value(if self.imperial {
            (st.antenna.height_m / FEET).round()
        } else {
            st.antenna.height_m.round()
        });
        self.gain.set_value(st.antenna.gain_dbd);
        self.filling.set(false);
        self.fill_direction();
        self.refresh_notes();
    }

    fn fill_direction(&self) {
        self.filling.set(true);
        let a = self.draft.borrow().antenna.clone();
        let shown = if a.kind.asked_as_wire() {
            antenna::wire_from_beam(a.beam_deg)
        } else {
            a.beam_deg
        };
        self.direction.set_value(shown.round());
        let title = if a.kind.asked_as_wire() {
            tr("station.wireRuns")
        } else {
            tr("station.beam")
        };
        self.direction.set_title(&title);
        self.direction_group.set_title(&title);
        self.filling.set(false);
    }

    fn bearing_to_destination(&self) -> Option<f64> {
        let v = self.app.view.borrow();
        let (from, to) = (v.saved.from.as_ref()?, v.saved.to.as_ref()?);
        Some(geo::bearing_deg(from.lat_lon(), to.lat_lon()))
    }

    fn refresh_notes(&self) {
        let st = self.draft.borrow().clone();
        let mode = tr(&format!("station.mode.{}", st.mode.key()));
        self.mode_note.set_text(&trf(
            "station.modeNeeds",
            &[
                ("mode", &mode),
                ("db", &st.mode.required_snr_db().to_string()),
            ],
        ));

        let kind = st.antenna.kind;
        self.height.set_visible(kind.uses_height());
        self.gain.set_visible(kind.uses_gain());
        self.height.set_title(&if kind == AntennaKind::InvertedV {
            tr("station.apexHeight")
        } else {
            tr("station.height")
        });
        let note = match kind {
            AntennaKind::Isotropic => tr("station.isotropicNote"),
            AntennaKind::InvertedV => trf(
                "station.invertedVNote",
                &[
                    (
                        "height",
                        &format::height(st.antenna.effective_height_m(), self.imperial),
                    ),
                    ("percent", &format::int(INVERTED_V_HEIGHT_FRACTION * 100.0)),
                ],
            ),
            AntennaKind::Vertical => tr("station.verticalNote"),
            _ => tr("station.heightNote"),
        };
        self.antenna_note.set_text(&note);
        self.direction_group.set_visible(kind.uses_beam());

        let lobes = antenna::lobes(st.antenna.beam_deg, kind);
        self.favours.set_text(&match lobes.as_slice() {
            [a, b] => trf(
                "station.favoursTwo",
                &[
                    ("first", &format::degrees(*a)),
                    ("second", &format::degrees(*b)),
                ],
            ),
            [a] => trf("station.favoursOne", &[("first", &format::degrees(*a))]),
            _ => String::new(),
        });

        let to = self.app.view.borrow().saved.to.clone();
        match (self.bearing_to_destination(), to) {
            (Some(bearing), Some(to)) if kind.uses_beam() => {
                let lobe = antenna::nearest_lobe(st.antenna.beam_deg, kind, bearing);
                let off = antenna::separation_deg(lobe, bearing);
                let key = if off <= 30.0 {
                    "station.aim.best"
                } else if off >= 60.0 && kind.is_bidirectional() {
                    "station.aim.nearNull"
                } else {
                    "station.aim.offToOneSide"
                };
                self.aim.set_text(&trf(
                    key,
                    &[
                        ("place", &to.name),
                        ("degrees", &format::degrees(bearing)),
                        ("lobe", &format::degrees(lobe)),
                        ("offset", &format::degrees(off)),
                    ],
                ));
                self.aim.set_visible(true);
                self.align.set_label(&trf(
                    "station.alignFor",
                    &[("place", &to.name), ("degrees", &format::degrees(bearing))],
                ));
                self.align.set_visible(true);
            }
            _ => {
                self.aim.set_visible(false);
                self.align.set_visible(false);
            }
        }

        let colors = self.app.view.borrow().colors;
        let wire = kind.asked_as_wire();
        let beam = st.antenna.beam_deg;
        let target = self.bearing_to_destination();
        let compass = self.compass.clone();
        self.compass.set_draw(move |_, s, w, h| {
            let r = w.min(h) / 2.0 - 18.0;
            let (cx, cy) = (w / 2.0, h / 2.0);
            let at = |deg: f64, rad: f32| {
                let t = (deg as f32).to_radians();
                (cx + rad * t.sin(), cy - rad * t.cos())
            };
            canvas::outline_round(
                s,
                canvas::rect(cx - r, cy - r, 2.0 * r, 2.0 * r),
                r,
                1.5,
                &colors.dim,
            );
            for (i, key) in ["n", "e", "s", "w"].iter().enumerate() {
                let (x, y) = at(i as f64 * 90.0, r + 10.0);
                let t = tr(&format!("station.compass.{key}"));
                let (tw, th) = canvas::text_size(&compass, &t);
                canvas::text(&compass, s, x - tw / 2.0, y - th / 2.0, &t, &colors.dim);
            }
            // The lobes, as wedges of dots.
            for lobe in &lobes {
                for k in -12..=12 {
                    let spread = f64::from(k) * 2.5;
                    let reach = r * (1.0 - (spread.abs() as f32 / 40.0).powi(2)) * 0.92;
                    let (x, y) = at(lobe + spread, reach);
                    canvas::line(s, cx, cy, x, y, 2.0, &{
                        let mut c = colors.cells[2];
                        c.set_alpha(0.35);
                        c
                    });
                }
                let (x, y) = at(*lobe, r * 0.92);
                canvas::line(s, cx, cy, x, y, 3.0, &colors.cells[3]);
            }
            if wire {
                let run = antenna::wire_from_beam(beam);
                let (x1, y1) = at(run, r * 0.75);
                let (x2, y2) = at(run + 180.0, r * 0.75);
                canvas::line(s, x1, y1, x2, y2, 4.0, &colors.fg);
            }
            if let Some(t) = target {
                let (x, y) = at(t, r);
                canvas::circle(s, x, y, 6.0, &colors.now);
            }
            canvas::circle(s, cx, cy, 4.0, &colors.fg);
        });
    }
}
