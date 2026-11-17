//! Restarting the app, which a language change needs (the string tables load
//! once, at start-up).
//!
//! The running copy quits; once its main loop is over, `main` starts a new
//! one and exits. Where GApplication keeps to one instance (the session bus,
//! on Linux), the new copy first waits for the old one to let go of the app
//! id, or it would only raise the old window and quit.
//!
//! - Flatpak: a new sandbox through the Flatpak portal (`flatpak-spawn`
//!   without `--host`), since the old sandbox ends with the old copy.
//! - AppImage: the AppImage file itself (`$APPIMAGE`), since the old copy's
//!   mount goes away with it.
//! - Elsewhere: this program again.

use std::cell::Cell;
use std::process::{Command, Stdio};

use gtk::prelude::*;

/// Set in the new copy's environment.
const RESTARTED: &str = "HFPREDICT_RESTARTED";

thread_local! {
    static PENDING: Cell<bool> = const { Cell::new(false) };
}

/// Closes the windows and quits; `main` then starts the app again.
pub fn request(window: &gtk::Window) {
    PENDING.set(true);
    match window.application() {
        Some(app) => {
            for w in app.windows() {
                w.close();
            }
            app.quit();
        }
        None => window.close(),
    }
}

/// After the main loop: starts the new copy, if a restart was asked for.
pub fn relaunch_if_requested() {
    if !PENDING.get() {
        return;
    }
    if let Err(e) = command().spawn() {
        eprintln!("hfpredict: could not restart: {e}");
    }
}

fn command() -> Command {
    let exe = std::env::current_exe().unwrap_or_else(|_| "hfpredict".into());
    let mut c = if std::path::Path::new("/.flatpak-info").exists() {
        let mut c = Command::new("flatpak-spawn");
        c.arg(format!("--env={RESTARTED}=1")).arg(&exe);
        c
    } else if let Some(appimage) = std::env::var_os("APPIMAGE") {
        Command::new(appimage)
    } else {
        Command::new(&exe)
    };
    c.env(RESTARTED, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        // Its own process group, so it outlives this one.
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    c
}

/// In the new copy, before the application registers: waits (five seconds
/// at most) until the old copy no longer owns `app_id` on the session bus.
pub fn wait_for_old_copy(app_id: &str) {
    if std::env::var_os(RESTARTED).is_none() {
        return;
    }
    #[cfg(target_os = "linux")]
    {
        use gtk::{gio, glib};
        let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
            return;
        };
        for _ in 0..50 {
            let owned = bus
                .call_sync(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    "NameHasOwner",
                    Some(&(app_id,).to_variant()),
                    Some(glib::VariantTy::new("(b)").unwrap()),
                    gio::DBusCallFlags::NONE,
                    1000,
                    gio::Cancellable::NONE,
                )
                .ok()
                .and_then(|v| v.get::<(bool,)>())
                .is_some_and(|(b,)| b);
            if !owned {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app_id;
}
