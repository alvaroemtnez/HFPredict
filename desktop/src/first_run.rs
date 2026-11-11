//! The first launch: where are you transmitting from?

use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use hfpredict_core::places::Place;

use crate::i18n::tr;
use crate::location::{self, Search};
use crate::window::Window;

pub fn page(w: &Rc<Window>) -> gtk::Widget {
    let app = w.app.clone();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 14);
    body.set_margin_top(36);
    body.set_margin_bottom(36);
    body.set_margin_start(18);
    body.set_margin_end(18);

    let icon = gtk::Image::from_icon_name("io.alvaroedo.HFPredict");
    icon.set_pixel_size(96);
    icon.set_margin_bottom(6);
    body.append(&icon);
    let kicker = gtk::Label::new(Some(&tr("firstRun.kicker").to_uppercase()));
    kicker.add_css_class("caption-heading");
    kicker.add_css_class("accent");
    body.append(&kicker);
    let title = gtk::Label::builder()
        .label(tr("firstRun.title"))
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    title.add_css_class("title-1");
    body.append(&title);
    let subtitle = gtk::Label::builder()
        .label(tr("firstRun.subtitle"))
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    subtitle.add_css_class("dim-label");
    body.append(&subtitle);

    let finish = {
        let app = app.clone();
        let w = Rc::downgrade(w);
        move |p: Place| {
            app.save(|c| c.set_from(p));
            if let Some(w) = w.upgrade() {
                w.show_main();
            }
            app.refresh_readings();
        }
    };

    let gps = gtk::Button::builder()
        .label(tr("firstRun.useGps"))
        .halign(gtk::Align::Center)
        .margin_top(12)
        .build();
    gps.add_css_class("pill");
    gps.add_css_class("suggested-action");
    {
        let app = app.clone();
        let finish = finish.clone();
        let w = Rc::downgrade(w);
        gps.connect_clicked(move |b| {
            b.set_sensitive(false);
            b.set_label(&tr("location.locating"));
            let (app, finish, b, w) = (app.clone(), finish.clone(), b.clone(), w.clone());
            glib::spawn_future_local(async move {
                let r = location::device_position().await;
                b.set_sensitive(true);
                b.set_label(&tr("firstRun.useGps"));
                match r {
                    Ok((lat, lon)) => finish(app.core.place_at(lat, lon)),
                    Err(e) => {
                        eprintln!("hfpredict: location: {e}");
                        if let Some(w) = w.upgrade() {
                            w.toast(&tr("location.unavailable"));
                        }
                    }
                }
            });
        });
    }
    body.append(&gps);

    let or = gtk::Label::new(Some(&tr("firstRun.orType").to_uppercase()));
    or.add_css_class("caption-heading");
    or.add_css_class("dim-label");
    or.set_margin_top(18);
    body.append(&or);

    let search = Search::new(&app, finish.clone());
    body.append(&search.entry);
    let examples = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    examples.set_halign(gtk::Align::Center);
    for example in ["València", "39.47, -0.38", "IM99sl"] {
        let b = gtk::Button::with_label(example);
        b.add_css_class("hfp-chip");
        let entry = search.entry.clone();
        b.connect_clicked(move |_| {
            entry.set_text(example);
            entry.grab_focus();
        });
        examples.append(&b);
    }
    body.append(&examples);
    let formats = gtk::Label::builder()
        .label(format!(
            "{}\n{}",
            tr("firstRun.formats"),
            tr("firstRun.formatsHint")
        ))
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();
    formats.add_css_class("caption");
    formats.add_css_class("dim-label");
    body.append(&formats);
    body.append(&search.status);
    body.append(&search.list);

    let foot = gtk::Label::builder()
        .label(tr("firstRun.footnote"))
        .wrap(true)
        .justify(gtk::Justification::Center)
        .margin_top(18)
        .build();
    foot.add_css_class("caption");
    foot.add_css_class("dim-label");
    body.append(&foot);
    let skip = gtk::Button::builder()
        .label(tr("firstRun.skip"))
        .halign(gtk::Align::Center)
        .build();
    skip.add_css_class("flat");
    {
        let finish = finish.clone();
        skip.connect_clicked(move |_| finish(Place::greenwich()));
    }
    body.append(&skip);

    let clamp = adw::Clamp::builder().maximum_size(520).child(&body).build();
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&clamp)
        .vexpand(true)
        .build();
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(&tr("app.name"), "")));
    view.add_top_bar(&header);
    view.set_content(Some(&scroll));
    view.upcast()
}
