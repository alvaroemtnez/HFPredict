//! Choosing the two ends of the path.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use hfpredict_core::places::Place;

use crate::app::{self, App};
use crate::i18n::{self, tr};
use crate::window::Window;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum End {
    From,
    To,
}

/// Asks the desktop for the device's position (the XDG location portal,
/// GeoClue behind it). `Err` carries a reason to show.
#[cfg(not(all(unix, not(target_os = "macos"))))]
pub async fn device_position() -> Result<(f64, f64), String> {
    Err("no location portal on this system".to_string())
}

/// Asks the desktop for the device's position (the XDG location portal,
/// GeoClue behind it). `Err` carries a reason to show.
#[cfg(all(unix, not(target_os = "macos")))]
pub async fn device_position() -> Result<(f64, f64), String> {
    use ashpd::desktop::location::{Accuracy, CreateSessionOptions, LocationProxy};
    let work = async {
        let proxy = LocationProxy::new().await.map_err(|e| e.to_string())?;
        let session = proxy
            .create_session(CreateSessionOptions::default().set_accuracy(Accuracy::Exact))
            .await
            .map_err(|e| e.to_string())?;
        let mut updates = proxy
            .receive_location_updated()
            .await
            .map_err(|e| e.to_string())?;
        proxy
            .start(&session, None, Default::default())
            .await
            .map_err(|e| e.to_string())?;
        use futures_util::StreamExt;
        let location = updates
            .next()
            .await
            .ok_or_else(|| "no position".to_string())?;
        let _ = session.close().await;
        Ok((location.latitude(), location.longitude()))
    };
    let timeout = async {
        glib::timeout_future(Duration::from_secs(30)).await;
        Err("timed out".to_string())
    };
    futures_util::pin_mut!(work);
    futures_util::pin_mut!(timeout);
    match futures_util::future::select(work, timeout).await {
        futures_util::future::Either::Left((r, _))
        | futures_util::future::Either::Right((r, _)) => r,
    }
}

/// A list row for a place.
pub fn place_row(place: &Place) -> adw::ActionRow {
    let subtitle = if place.detail.is_empty() {
        place.grid.clone()
    } else {
        format!("{} · {}", place.detail, place.grid)
    };
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(&place.name))
        .subtitle(glib::markup_escape_text(&subtitle))
        .activatable(true)
        .build();
    row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
    row
}

/// Searches as the reader types: built-in places and typed coordinates at
/// once, online after a pause when those find little. `pick` receives the
/// chosen place.
pub struct Search {
    pub entry: gtk::SearchEntry,
    pub list: gtk::ListBox,
    pub status: gtk::Label,
    serial: Cell<u64>,
    results: RefCell<Vec<Place>>,
}

impl Search {
    pub fn new(app: &Rc<App>, pick: impl Fn(Place) + 'static) -> Rc<Search> {
        let entry = gtk::SearchEntry::builder()
            .placeholder_text(tr("location.searchPlaceholder"))
            .hexpand(true)
            .build();
        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        list.set_visible(false);
        let status = gtk::Label::builder()
            .wrap(true)
            .xalign(0.0)
            .visible(false)
            .build();
        status.add_css_class("dim-label");
        let search = Rc::new(Search {
            entry,
            list,
            status,
            serial: Cell::new(0),
            results: RefCell::new(vec![]),
        });
        // The entry owns the search: callers keep only its widgets.
        app::keep_alive(&search.entry, search.clone());
        let pick = Rc::new(pick);
        {
            let s = Rc::downgrade(&search);
            let pick = pick.clone();
            search.list.connect_row_activated(move |_, row| {
                if let Some(s) = s.upgrade()
                    && let Some(p) = s.results.borrow().get(row.index() as usize).cloned()
                {
                    pick(p);
                }
            });
        }
        {
            let s = Rc::downgrade(&search);
            let pick = pick.clone();
            search.entry.connect_activate(move |_| {
                if let Some(s) = s.upgrade()
                    && let Some(p) = s.results.borrow().first().cloned()
                {
                    pick(p);
                }
            });
        }
        {
            let s = Rc::downgrade(&search);
            let a = Rc::downgrade(app);
            search.entry.connect_search_changed(move |e| {
                if let (Some(s), Some(app)) = (s.upgrade(), a.upgrade()) {
                    s.run(&app, e.text().to_string());
                }
            });
        }
        search
    }

    fn show(&self, places: Vec<Place>) {
        while let Some(c) = self.list.first_child() {
            self.list.remove(&c);
        }
        for p in &places {
            self.list.append(&place_row(p));
        }
        self.list.set_visible(!places.is_empty());
        *self.results.borrow_mut() = places;
    }

    fn run(self: &Rc<Self>, app: &Rc<App>, query: String) {
        let serial = self.serial.get() + 1;
        self.serial.set(serial);
        self.status.set_visible(false);
        if query.trim().is_empty() {
            self.show(vec![]);
            return;
        }
        let local = app
            .core
            .search_places(query.clone(), i18n::search_language().into(), false)
            .unwrap_or_default();
        let few = local.len() < 3;
        self.show(local);
        let online = app.view.borrow().saved.settings.network;
        if !few || !online || query.trim().chars().count() < 3 {
            if self.results.borrow().is_empty() {
                self.status.set_text(&tr("location.noResults"));
                self.status.set_visible(true);
            }
            return;
        }
        // A pause before reaching the network, so typing is not a stream of
        // requests.
        let me = Rc::downgrade(self);
        let core = app.core.clone();
        glib::timeout_add_local_once(Duration::from_millis(600), move || {
            let Some(me) = me.upgrade() else { return };
            if me.serial.get() != serial {
                return;
            }
            me.status.set_text(&tr("location.searching"));
            me.status.set_visible(true);
            let lang = i18n::search_language().to_string();
            let weak = Rc::downgrade(&me);
            app::spawn(
                move || core.search_places(query, lang, true),
                move |result| {
                    let Some(me) = weak.upgrade() else { return };
                    if me.serial.get() != serial {
                        return;
                    }
                    match result {
                        Ok(found) => {
                            let empty = found.is_empty();
                            me.show(found);
                            me.status.set_text(&tr("location.noResults"));
                            me.status.set_visible(empty);
                        }
                        Err(_) => {
                            me.status.set_text(&tr("location.searchFailed"));
                            me.status.set_visible(true);
                        }
                    }
                },
            );
        });
    }
}

pub fn open(w: &Rc<Window>, start: End) {
    let app = w.app.clone();
    let dialog = adw::Dialog::builder()
        .title(tr("location.title"))
        .content_width(520)
        .content_height(640)
        .follows_content_size(false)
        .build();
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());

    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.set_margin_top(6);
    page.set_margin_bottom(18);
    page.set_margin_start(12);
    page.set_margin_end(12);

    let end = Rc::new(Cell::new(start));
    let toggles = adw::ToggleGroup::new();
    toggles.add(
        adw::Toggle::builder()
            .name("from")
            .label(tr("location.from"))
            .build(),
    );
    toggles.add(
        adw::Toggle::builder()
            .name("to")
            .label(tr("location.to"))
            .build(),
    );
    toggles.set_active_name(Some(if start == End::From { "from" } else { "to" }));
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    toggles.set_hexpand(true);
    head.append(&toggles);
    let swap = gtk::Button::builder()
        .icon_name("object-flip-vertical-symbolic")
        .tooltip_text(tr("location.swap"))
        .build();
    swap.update_property(&[gtk::accessible::Property::Label(&tr("location.swap"))]);
    head.append(&swap);
    page.append(&head);

    let current = gtk::Label::builder().xalign(0.0).wrap(true).build();
    current.add_css_class("dim-label");
    page.append(&current);

    let close = {
        let d = dialog.clone();
        move || {
            d.close();
        }
    };
    let choose = {
        let app = app.clone();
        let end = end.clone();
        let close = close.clone();
        move |p: Place| {
            match end.get() {
                End::From => app.save(|c| c.set_from(p)),
                End::To => app.save(|c| c.set_to(Some(p))),
            }
            close();
        }
    };
    let search = Search::new(&app, choose.clone());
    page.append(&search.entry);

    let actions = gtk::ListBox::new();
    actions.add_css_class("boxed-list");
    actions.set_selection_mode(gtk::SelectionMode::None);
    let gps = adw::ButtonRow::builder()
        .title(tr("location.useDevice"))
        .start_icon_name("find-location-symbolic")
        .build();
    let anywhere = adw::ActionRow::builder()
        .title(tr("location.everyDirection"))
        .subtitle(tr("location.everyDirectionHint"))
        .activatable(true)
        .build();
    anywhere.add_prefix(&gtk::Image::from_icon_name("emblem-shared-symbolic"));
    actions.append(&gps);
    actions.append(&anywhere);
    page.append(&actions);
    page.append(&search.status);
    page.append(&search.list);

    let recent_title = gtk::Label::builder()
        .label(tr("location.recent"))
        .xalign(0.0)
        .build();
    recent_title.add_css_class("heading");
    let recent = gtk::ListBox::new();
    recent.add_css_class("boxed-list");
    recent.set_selection_mode(gtk::SelectionMode::None);
    let places = app.view.borrow().saved.recent.clone();
    for p in &places {
        recent.append(&place_row(p));
    }
    recent_title.set_visible(!places.is_empty());
    recent.set_visible(!places.is_empty());
    {
        let choose = choose.clone();
        recent.connect_row_activated(move |_, row| {
            if let Some(p) = places.get(row.index() as usize).cloned() {
                choose(p);
            }
        });
    }
    page.append(&recent_title);
    page.append(&recent);

    let refresh = {
        let app = app.clone();
        let end = end.clone();
        let (gps, anywhere, swap, current) =
            (gps.clone(), anywhere.clone(), swap.clone(), current.clone());
        move || {
            let v = app.view.borrow();
            let e = end.get();
            gps.set_visible(e == End::From);
            anywhere.set_visible(e == End::To && v.saved.to.is_some());
            swap.set_sensitive(v.saved.to.is_some());
            let place = if e == End::From {
                v.saved.from.as_ref()
            } else {
                v.saved.to.as_ref()
            };
            current.set_text(&match place {
                Some(p) => format!("{} · {}", p.name, p.grid),
                None => tr("path.everyDirection"),
            });
        }
    };
    refresh();
    {
        let end = end.clone();
        let refresh = refresh.clone();
        toggles.connect_active_name_notify(move |t| {
            end.set(if t.active_name().as_deref() == Some("to") {
                End::To
            } else {
                End::From
            });
            refresh();
        });
    }
    {
        let app = app.clone();
        let refresh = refresh.clone();
        swap.connect_clicked(move |_| {
            app.save(|c| c.swap_ends());
            refresh();
        });
    }
    {
        let app = app.clone();
        let close = close.clone();
        actions.connect_row_activated(move |_, row| {
            if row.index() == 1 {
                app.save(|c| c.set_to(None));
                close();
            }
        });
    }
    {
        let app = app.clone();
        let w = Rc::downgrade(w);
        let close = close.clone();
        gps.connect_activated(move |row| {
            row.set_sensitive(false);
            row.set_title(&tr("location.locating"));
            let app = app.clone();
            let row = row.clone();
            let w = w.clone();
            let close = close.clone();
            glib::spawn_future_local(async move {
                let result = device_position().await;
                row.set_sensitive(true);
                row.set_title(&tr("location.useDevice"));
                match result {
                    Ok((lat, lon)) => {
                        let place = app.core.place_at(lat, lon);
                        app.save(|c| c.set_from(place));
                        close();
                    }
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

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&adw::Clamp::builder().maximum_size(560).child(&page).build())
        .build();
    view.set_content(Some(&scroll));
    dialog.set_child(Some(&view));
    dialog.present(Some(&w.window));
    search.entry.grab_focus();
}
