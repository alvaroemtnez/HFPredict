//! "When else": every band at every hour, and the usable window.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use hfpredict_core::bands::Band;
use hfpredict_core::quality::Quality;

use crate::app::{App, Change};
use crate::canvas::{self, Canvas};
use crate::format;
use crate::i18n::{tr, trf};

const ROW_H: f32 = 26.0;
const GAP: f32 = 3.0;
const LABEL_W: f32 = 48.0;
const AXIS_H: f32 = 22.0;

pub struct BandsCard {
    pub root: gtk::Box,
    title: gtk::Label,
    readout: gtk::Label,
    rail_box: gtk::Box,
    rail_label: gtk::Label,
    rail: Canvas,
    unit: gtk::Label,
    toggle: gtk::Button,
    grid: Canvas,
    table: gtk::Grid,
    table_scroll: gtk::ScrolledWindow,
    legend: adw::WrapBox,
    note: gtk::Label,
    footnote: gtk::Label,
    hover: Cell<Option<(Band, u32)>>,
}

/// A row that puts its second child under the first when both do not fit,
/// at the far end otherwise.
fn spread_row() -> adw::WrapBox {
    let row = adw::WrapBox::new();
    row.set_child_spacing(8);
    row.set_line_spacing(2);
    row.set_justify(adw::JustifyMode::Spread);
    // A single line is also the last one.
    row.set_justify_last_line(true);
    row
}

fn label(classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::builder().xalign(0.0).wrap(true).build();
    for c in classes {
        l.add_css_class(c);
    }
    l
}

/// Which band and hour sit under a point of the grid.
fn cell_at(x: f64, y: f64, width: f64, start: u32) -> Option<(Band, u32)> {
    let col_w = (width as f32 - LABEL_W) / 24.0;
    let col = ((x as f32 - LABEL_W) / col_w).floor();
    let row = (y as f32 / ROW_H).floor();
    if !(0.0..24.0).contains(&col) || !(0.0..10.0).contains(&row) {
        return None;
    }
    Some((Band::DISPLAY[row as usize], (start + col as u32) % 24))
}

impl BandsCard {
    pub fn new(app: &Rc<App>) -> Rc<BandsCard> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
        root.add_css_class("card");
        root.add_css_class("hfp-card");

        let title = label(&["title-4"]);
        let readout = label(&["caption-heading", "accent"]);
        root.append(&title);
        root.append(&readout);

        let rail_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        rail_box.add_css_class("hfp-inset");
        // Rows of a title and a value (or a button) put the second under
        // the first when the window is too narrow for both.
        let rail_head = spread_row();
        let rail_title = label(&["caption-heading", "dim-label"]);
        rail_title.set_text(&tr("window.label"));
        let rail_label = gtk::Label::new(None);
        rail_label.add_css_class("caption");
        rail_head.append(&rail_title);
        rail_head.append(&rail_label);
        rail_box.append(&rail_head);
        let rail = Canvas::new();
        rail.set_height_for_width(|_| 34);
        rail_box.append(&rail);
        root.append(&rail_box);

        let unit_row = spread_row();
        unit_row.set_margin_top(6);
        let unit = label(&["caption-heading", "dim-label"]);
        unit.set_valign(gtk::Align::Center);
        let toggle = gtk::Button::with_label(&tr("grid.showAsTable"));
        toggle.add_css_class("flat");
        toggle.add_css_class("accent");
        unit_row.append(&unit);
        unit_row.append(&toggle);
        root.append(&unit_row);

        let grid = Canvas::new();
        // 24 hours after the band names: 10 px an hour at the least.
        grid.set_widths(280, 640);
        grid.set_height_for_width(|_| (10.0 * ROW_H + AXIS_H) as i32);
        grid.set_focusable(true);
        root.append(&grid);

        let table = gtk::Grid::builder()
            .column_spacing(10)
            .row_spacing(4)
            .build();
        table.add_css_class("hfp-table");
        let table_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .child(&table)
            .visible(false)
            .build();
        root.append(&table_scroll);

        let legend = adw::WrapBox::new();
        legend.set_child_spacing(14);
        legend.set_line_spacing(4);
        legend.add_css_class("caption");
        root.append(&legend);
        let note = label(&["heading"]);
        root.append(&note);
        let footnote = label(&["caption", "dim-label"]);
        root.append(&footnote);

        let card = Rc::new(BandsCard {
            root,
            title,
            readout,
            rail_box,
            rail_label,
            rail,
            unit,
            toggle,
            grid,
            table,
            table_scroll,
            legend,
            note,
            footnote,
            hover: Cell::new(None),
        });
        card.wire(app);
        card
    }

    fn wire(self: &Rc<Self>, app: &Rc<App>) {
        let a = Rc::downgrade(app);
        let me = Rc::downgrade(self);
        self.grid.set_draw(move |c, s, w, _| {
            let (Some(app), Some(me)) = (a.upgrade(), me.upgrade()) else {
                return;
            };
            let v = app.view.borrow();
            let colors = v.colors;
            let start = v.track_start();
            let col_w = (w - LABEL_W) / 24.0;
            let hover = me.hover.get();
            for (row, band) in Band::DISPLAY.iter().enumerate() {
                let y = row as f32 * ROW_H;
                let bold = *band == v.band;
                let markup = if bold {
                    format!("<b>{}</b>", band.label())
                } else {
                    band.label().to_string()
                };
                let (_, th) = canvas::text_size(c, &markup);
                canvas::text(
                    c,
                    s,
                    0.0,
                    y + (ROW_H - th) / 2.0,
                    &markup,
                    if bold { &colors.fg } else { &colors.dim },
                );
                for i in 0..24u32 {
                    let hour = (start + i) % 24;
                    let x = LABEL_W + i as f32 * col_w;
                    let r = v
                        .forecast
                        .as_ref()
                        .and_then(|f| f.cell(*band, hour))
                        .map_or(0.0, |c| c.reliability);
                    let mut colour = colors.cell(Quality::of(r));
                    if v.forecast.is_none() {
                        colour = colors.cells[0];
                    }
                    if hover == Some((*band, hour)) {
                        colour.set_alpha((colour.alpha() * 0.75).max(0.2));
                    }
                    canvas::fill_round(
                        s,
                        canvas::rect(x + GAP / 2.0, y + GAP / 2.0, col_w - GAP, ROW_H - GAP),
                        4.0,
                        &colour,
                    );
                }
            }
            // The hour on screen, outlined; "now" marked above the axis.
            let sel = (v.hour + 24 - start) % 24;
            let x = LABEL_W + sel as f32 * col_w;
            canvas::outline_round(
                s,
                canvas::rect(x, 0.0, col_w, 10.0 * ROW_H),
                5.0,
                2.0,
                &colors.now,
            );
            let band_row = Band::DISPLAY.iter().position(|b| *b == v.band).unwrap_or(0) as f32;
            canvas::outline_round(
                s,
                canvas::rect(x + 1.5, band_row * ROW_H + 1.5, col_w - 3.0, ROW_H - 3.0),
                4.0,
                2.0,
                &colors.fg,
            );
            let now_x = LABEL_W + (crate::app::PAST_HOURS as f32 + 0.5) * col_w;
            canvas::circle(s, now_x, 10.0 * ROW_H + 3.0, 2.5, &colors.now);
            for i in (0..24u32).step_by(if col_w < 22.0 { 4 } else { 2 }) {
                let hour = (start + i) % 24;
                let t = format!("{hour:02}");
                let (tw, _) = canvas::text_size(c, &format!("<small>{t}</small>"));
                let color = if i == sel { colors.now } else { colors.dim };
                canvas::text(
                    c,
                    s,
                    LABEL_W + (i as f32 + 0.5) * col_w - tw / 2.0,
                    10.0 * ROW_H + 6.0,
                    &format!("<small>{t}</small>"),
                    &color,
                );
            }
        });

        let click = gtk::GestureClick::new();
        {
            let a = Rc::downgrade(app);
            let grid = self.grid.clone();
            click.connect_released(move |_, _, x, y| {
                if let Some(app) = a.upgrade() {
                    let start = app.view.borrow().track_start();
                    if let Some((band, hour)) = cell_at(x, y, f64::from(grid.width()), start) {
                        app.select(band, hour);
                    }
                }
            });
        }
        self.grid.add_controller(click);
        // Dragging along a row slides through the hours with the cursor
        // under the pointer, rather than jumping at the end.
        let drag = gtk::GestureDrag::new();
        {
            let a = Rc::downgrade(app);
            let grid = self.grid.clone();
            drag.connect_drag_update(move |g, dx, _| {
                let (Some(app), Some((x0, y0))) = (a.upgrade(), g.start_point()) else {
                    return;
                };
                let (band, start, hour) = {
                    let v = app.view.borrow();
                    (v.band, v.track_start(), v.hour)
                };
                if let Some((_, h)) = cell_at(x0 + dx, y0, f64::from(grid.width()), start)
                    && h != hour
                {
                    app.select(band, h);
                }
            });
        }
        self.grid.add_controller(drag);
        let motion = gtk::EventControllerMotion::new();
        {
            let a = Rc::downgrade(app);
            let me = Rc::downgrade(self);
            motion.connect_motion(move |_, x, y| {
                let (Some(app), Some(me)) = (a.upgrade(), me.upgrade()) else {
                    return;
                };
                let start = app.view.borrow().track_start();
                let cell = cell_at(x, y, f64::from(me.grid.width()), start);
                if cell != me.hover.get() {
                    me.hover.set(cell);
                    let tip = cell.and_then(|(band, hour)| {
                        let v = app.view.borrow();
                        let c = v.forecast.as_ref()?.cell(band, hour)?;
                        Some(trf(
                            "grid.cell",
                            &[
                                ("band", band.label()),
                                ("hour", &format::hour(hour)),
                                ("percent", &format::percent(c.reliability)),
                                ("quality", &format::quality(Quality::of(c.reliability))),
                            ],
                        ))
                    });
                    me.grid.set_tooltip_text(tip.as_deref());
                    me.grid.queue_draw();
                }
            });
        }
        {
            let me = Rc::downgrade(self);
            motion.connect_leave(move |_| {
                if let Some(me) = me.upgrade() {
                    me.hover.set(None);
                    me.grid.queue_draw();
                }
            });
        }
        self.grid.add_controller(motion);

        {
            let a = Rc::downgrade(app);
            self.toggle.connect_clicked(move |_| {
                if let Some(app) = a.upgrade() {
                    let show = !app.view.borrow().show_table;
                    app.view.borrow_mut().show_table = show;
                    app.emit(Change::Selection);
                }
            });
        }

        let a = Rc::downgrade(app);
        self.rail.set_draw(move |c, s, w, h| {
            let Some(app) = a.upgrade() else { return };
            let v = app.view.borrow();
            let Some(window) = v.forecast.as_ref().and_then(|f| f.window.as_ref()) else {
                return;
            };
            let colors = v.colors;
            // 2 to 30 MHz, logarithmic, as an operator thinks of the bands.
            let (lo, hi) = (2.0f64.ln(), 30.0f64.ln());
            let x_of = |mhz: f64| ((mhz.clamp(2.0, 30.0).ln() - lo) / (hi - lo)) as f32 * w;
            let y = h / 2.0 - 3.0;
            canvas::fill_round(s, canvas::rect(0.0, y, w, 6.0), 3.0, &colors.cells[0]);
            let hour = v.hour as usize;
            let ceiling = window.hpf_by_hour.get(hour).copied().flatten().or_else(|| {
                v.forecast
                    .as_ref()
                    .map(|f| f.muf_by_hour[hour])
                    .filter(|m| *m > 0.0)
            });
            let floor = window.luf_by_hour.get(hour).copied().flatten();
            if let Some(top) = ceiling {
                let x0 = floor.map_or(0.0, x_of);
                let x1 = x_of(top);
                if x1 > x0 {
                    canvas::fill_round(s, canvas::rect(x0, y, x1 - x0, 6.0), 3.0, &colors.cells[2]);
                }
            }
            for band in Band::BY_FREQ {
                let x = x_of(band.mhz());
                let active = band == v.band;
                let c = if active { colors.now } else { colors.dim };
                canvas::fill_round(
                    s,
                    canvas::rect(
                        x - 1.0,
                        y - if active { 5.0 } else { 3.0 },
                        2.0,
                        6.0 + if active { 10.0 } else { 6.0 },
                    ),
                    1.0,
                    &c,
                );
            }
            let small = |t: &str| format!("<small>{t}</small>");
            canvas::text(c, s, 0.0, h - 12.0, &small("2 MHz"), &colors.dim);
            let (tw, _) = canvas::text_size(c, &small("30 MHz"));
            canvas::text(c, s, w - tw, h - 12.0, &small("30 MHz"), &colors.dim);
        });

        let me = Rc::downgrade(self);
        app.listen(move |app, change| {
            if let Some(me) = me.upgrade() {
                me.update(app, change);
            }
        });
        self.update(app, Change::Saved);
    }

    fn update(&self, app: &App, change: Change) {
        let v = app.view.borrow();
        let to = v.forecast.as_ref().and_then(|f| f.to.clone());
        let from_name = v
            .saved
            .from
            .as_ref()
            .map_or_else(|| "Greenwich".to_string(), |p| p.name.clone());
        self.title.set_text(&match &to {
            Some(t) => trf("sections.allBandsTo", &[("place", &t.name)]),
            None => tr("sections.allBandsAnywhere"),
        });
        let hour = format::hour(v.hour);
        self.readout.set_text(&if v.hour == v.now_hour() {
            trf(
                "grid.readoutNow",
                &[("hour", &hour), ("band", v.band.label())],
            )
            .to_uppercase()
        } else {
            trf(
                "grid.readoutAt",
                &[("hour", &hour), ("band", v.band.label())],
            )
            .to_uppercase()
        });
        self.unit.set_text(
            &if to.is_some() {
                tr("grid.unitChance")
            } else {
                tr("grid.unitDirections")
            }
            .to_uppercase(),
        );

        // The usable window, for one path.
        let window = v.forecast.as_ref().and_then(|f| f.window.as_ref());
        self.rail_box.set_visible(window.is_some());
        if let Some(w) = window {
            let h = v.hour as usize;
            let ceiling = w.hpf_by_hour.get(h).copied().flatten();
            let floor = w.luf_by_hour.get(h).copied().flatten();
            let text = match (floor, ceiling) {
                (Some(f), Some(c)) => trf(
                    "window.floorCeiling",
                    &[("floor", &format::mhz(f)), ("ceiling", &format::mhz(c))],
                ),
                (None, Some(c)) => trf("window.ceilingOnly", &[("ceiling", &format::mhz(c))]),
                _ => tr("window.none"),
            };
            self.rail_label.set_text(&text);
            let explain = match (floor, ceiling) {
                (Some(f), Some(c)) => trf(
                    "window.explain",
                    &[("floor", &format::mhz(f)), ("ceiling", &format::mhz(c))],
                ),
                (None, Some(c)) => trf("window.explainNoFloor", &[("ceiling", &format::mhz(c))]),
                _ => String::new(),
            };
            self.rail_box.set_tooltip_text(Some(&explain));
        }

        let show_table = v.show_table;
        self.toggle.set_label(&if show_table {
            tr("grid.showAsGrid")
        } else {
            tr("grid.showAsTable")
        });
        self.grid.set_visible(!show_table);
        self.table_scroll.set_visible(show_table);
        if show_table
            && matches!(
                change,
                Change::Forecast | Change::Selection | Change::Saved | Change::Clock
            )
        {
            self.fill_table(app);
        }

        let all_closed = v.forecast.as_ref().is_some_and(|f| f.all_closed());
        self.note.set_visible(all_closed);
        if all_closed {
            self.note.set_text(&match &to {
                Some(t) => trf("sections.noneReach", &[("place", &t.name)]),
                None => tr("sections.noneReachAnywhere"),
            });
        }
        let mut foot = trf("grid.footnoteHours", &[("place", &from_name)]);
        if !app.core.is_nowcast(v.now)
            && v.saved.settings.network
            && v.saved.space_weather.is_some()
        {
            foot.push(' ');
            foot.push_str(&tr("grid.footnoteSaved"));
        }
        self.footnote.set_text(&foot);

        if matches!(change, Change::Colors | Change::Saved) {
            while let Some(c) = self.legend.first_child() {
                self.legend.remove(&c);
            }
            for q in Quality::ORDER {
                let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let sw = Canvas::new();
                sw.set_widths(12, 12);
                sw.set_height_for_width(|_| 12);
                sw.set_valign(gtk::Align::Center);
                let col = v.colors.cell(q);
                let fg = v.colors.dim;
                sw.set_draw(move |_, s, w, h| {
                    canvas::fill_round(s, canvas::rect(0.0, 0.0, w, h), 3.0, &col);
                    if q == Quality::Closed {
                        canvas::outline_round(s, canvas::rect(0.0, 0.0, w, h), 3.0, 1.0, &fg);
                    }
                });
                b.append(&sw);
                let l = gtk::Label::new(Some(&format::quality(q)));
                l.set_tooltip_text(Some(&tr(&format!("qualityDescription.{}", q.key()))));
                b.append(&l);
                self.legend.append(&b);
            }
        }
        drop(v);
        self.grid.queue_draw();
        self.rail.queue_draw();
    }

    fn fill_table(&self, app: &App) {
        while let Some(c) = self.table.first_child() {
            self.table.remove(&c);
        }
        let v = app.view.borrow();
        let Some(f) = v.forecast.as_ref() else { return };
        let start = v.track_start();
        let head = gtk::Label::new(Some(&tr("grid.bandColumn")));
        head.add_css_class("caption-heading");
        head.set_xalign(0.0);
        self.table.attach(&head, 0, 0, 1, 1);
        for i in 0..24u32 {
            let hour = (start + i) % 24;
            let l = gtk::Label::new(Some(&format!("{hour:02}")));
            l.add_css_class("caption-heading");
            if hour == v.hour {
                l.add_css_class("accent");
            }
            self.table.attach(&l, i as i32 + 1, 0, 1, 1);
        }
        for (row, band) in Band::DISPLAY.iter().enumerate() {
            let l = gtk::Label::new(Some(band.label()));
            l.set_xalign(0.0);
            if *band == v.band {
                l.add_css_class("heading");
            }
            self.table.attach(&l, 0, row as i32 + 1, 1, 1);
            for i in 0..24u32 {
                let hour = (start + i) % 24;
                let r = f.cell(*band, hour).map_or(0.0, |c| c.reliability);
                let cell = gtk::Label::new(Some(&format!("{}", (r * 100.0).round())));
                cell.add_css_class("numeric");
                cell.add_css_class("caption");
                // Traffic-light numbers (main.rs set_dynamic_css).
                let q = Quality::of(r);
                cell.add_css_class(&format!("hfp-sem-{}", q.key()));
                cell.set_tooltip_text(Some(&format::quality(q)));
                cell.set_xalign(1.0);
                self.table.attach(&cell, i as i32 + 1, row as i32 + 1, 1, 1);
            }
        }
    }
}
