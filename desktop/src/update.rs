//! Updates for the Windows and macOS builds, which no store keeps current
//! (the core finds and checks them: `hfpredict_core::update`).
//!
//! At start the app asks for the latest release; a newer one brings up a
//! dialog, unless its reminder was turned off, and the main menu's "Check
//! for updates" becomes "Update to HFPredict X". Updating downloads the
//! release file, checks it against `SHA256SUMS` and hands over:
//!
//! - Windows: the installer runs silently (`/S`) over the installed copy,
//!   waits for the app to quit, then opens it again (`/RELAUNCH`;
//!   packaging/windows/installer.nsi). Per user, so no administrator prompt.
//! - macOS: a small script waits for the app to quit, swaps the .app for
//!   the one in the disk image and opens it. Where the app cannot replace
//!   itself (run from the disk image, or from a folder it cannot write to),
//!   the release page opens instead.
//!
//! Files the app downloads itself carry no quarantine or "mark of the web",
//! so neither system asks again before opening the new version.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gio, glib};
use hfpredict_core::Error;
use hfpredict_core::update::{self, Progress, Update};

use crate::app::spawn;
use crate::i18n::{tr, trf};
use crate::window::Window;

/// Whether this build updates itself (Windows and macOS).
pub fn supported() -> bool {
    update::platform_file().is_some()
}

/// Where downloads wait for the installer; emptied at the next start.
fn download_dir() -> PathBuf {
    std::env::temp_dir().join("HFPredict-update")
}

/// The check at start: quiet unless there is an update to offer.
pub fn check_at_start(w: &Rc<Window>) {
    if !supported() {
        return;
    }
    let core = w.app.core.clone();
    let weak = Rc::downgrade(w);
    spawn(
        move || {
            // The last update's installer has long finished with it.
            let _ = std::fs::remove_dir_all(download_dir());
            core.check_update()
        },
        move |result| {
            let (Some(w), Ok(Some(u))) = (weak.upgrade(), result) else {
                return;
            };
            found(&w, &u);
            if w.app.core.should_remind(&u) {
                offer(&w, &u, true);
            }
        },
    );
}

/// The menu's "Check for updates", or "Update to …" once one is known.
pub fn from_menu(w: &Rc<Window>) {
    if let Some(u) = w.update.borrow().clone() {
        offer(w, &u, false);
        return;
    }
    let core = w.app.core.clone();
    let weak = Rc::downgrade(w);
    spawn(
        move || core.check_update(),
        move |result| {
            let Some(w) = weak.upgrade() else {
                return;
            };
            let toast = match result {
                Ok(Some(u)) => {
                    found(&w, &u);
                    offer(&w, &u, false);
                    return;
                }
                Ok(None) => tr("update.upToDate"),
                Err(Error::Offline) => tr("update.offline"),
                Err(_) => tr("update.checkFailed"),
            };
            w.toasts.add_toast(adw::Toast::new(&toast));
        },
    );
}

/// Remembers the update and names it in the menu.
fn found(w: &Rc<Window>, u: &Update) {
    *w.update.borrow_mut() = Some(u.clone());
    let section = &w.update_section;
    if section.n_items() > 0 {
        section.remove(0);
    }
    section.append(
        Some(&trf("update.menuInstall", &[("version", &u.version)])),
        Some("app.update"),
    );
}

/// The dialog offering the update. At start it also offers to stop
/// reminding about this version.
pub fn offer(w: &Rc<Window>, u: &Update, at_start: bool) {
    let in_place = can_install_here();
    let body = format!(
        "{}\n\n<a href=\"{}\">{}</a>",
        glib::markup_escape_text(&trf(
            if in_place {
                "update.body"
            } else {
                "update.bodyManual"
            },
            &[("current", update::CURRENT), ("version", &u.version)],
        )),
        glib::markup_escape_text(&u.page),
        glib::markup_escape_text(&tr("update.whatsNew")),
    );
    let dialog =
        adw::AlertDialog::new(Some(&trf("update.title", &[("version", &u.version)])), None);
    dialog.set_body_use_markup(true);
    dialog.set_body(&body);
    let remind = gtk::CheckButton::with_label(&tr("update.dontRemind"));
    if at_start {
        dialog.set_extra_child(Some(&remind));
    }
    dialog.add_responses(&[
        ("later", &tr("update.later")),
        (
            "update",
            &tr(if in_place {
                "update.install"
            } else {
                "update.download"
            }),
        ),
    ]);
    dialog.set_response_appearance("update", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("update"));
    dialog.set_close_response("later");
    let weak = Rc::downgrade(w);
    let u = u.clone();
    dialog.connect_response(None, move |_, response| {
        let Some(w) = weak.upgrade() else {
            return;
        };
        if response == "update" {
            if in_place {
                install(&w, &u);
            } else {
                open_page(&w, &u.page);
            }
        } else if remind.is_active()
            && let Err(e) = w.app.core.skip_update(&u)
        {
            eprintln!("hfpredict: could not save: {e}");
        }
    });
    dialog.present(Some(&w.window));
}

fn open_page(w: &Window, url: &str) {
    gtk::UriLauncher::new(url).launch(Some(&w.window), gio::Cancellable::NONE, |_| {});
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_000_000.0)
}

/// Downloads the update with a progress dialog, then hands over to the
/// installer and quits.
fn install(w: &Rc<Window>, u: &Update) {
    let bar = gtk::ProgressBar::builder().show_text(true).build();
    let dialog = adw::AlertDialog::new(
        Some(&trf("update.downloading", &[("version", &u.version)])),
        Some(&tr("update.downloadingBody")),
    );
    dialog.set_extra_child(Some(&bar));
    dialog.add_response("cancel", &tr("common.cancel"));
    dialog.set_close_response("cancel");
    let progress = Arc::new(Progress::default());
    {
        let progress = progress.clone();
        dialog.connect_response(None, move |_, _| {
            progress.cancel.store(true, Ordering::Relaxed);
        });
    }
    dialog.present(Some(&w.window));

    // The bar follows the download until the dialog goes.
    {
        let progress = progress.clone();
        let bar = bar.clone();
        let dialog = dialog.downgrade();
        let size = u.size.max(1);
        glib::timeout_add_local(Duration::from_millis(200), move || {
            if dialog.upgrade().is_none_or(|d| !d.is_mapped()) {
                return glib::ControlFlow::Break;
            }
            let got = progress.received.load(Ordering::Relaxed);
            bar.set_fraction((got as f64 / size as f64).min(1.0));
            bar.set_text(Some(&trf(
                "update.progress",
                &[("done", &megabytes(got)), ("total", &megabytes(size))],
            )));
            glib::ControlFlow::Continue
        });
    }

    let weak = Rc::downgrade(w);
    let job = u.clone();
    let page = u.page.clone();
    spawn(
        move || update::download(&job, &download_dir(), &progress),
        move |result| {
            let Some(w) = weak.upgrade() else {
                return;
            };
            if !dialog.is_mapped() {
                // Cancelled.
                return;
            }
            let result = result.and_then(|path| hand_over(&path).map_err(Error::from));
            dialog.force_close();
            match result {
                Ok(()) => {
                    if let Some(a) = w.window.application() {
                        a.quit();
                    }
                }
                Err(e) => failed(&w, &e.to_string(), &page),
            }
        },
    );
}

fn failed(w: &Rc<Window>, error: &str, page: &str) {
    let dialog = adw::AlertDialog::new(
        Some(&tr("update.failed")),
        Some(&trf("update.failedBody", &[("error", error)])),
    );
    dialog.add_responses(&[
        ("close", &tr("common.close")),
        ("page", &tr("update.download")),
    ]);
    dialog.set_close_response("close");
    let weak = Rc::downgrade(w);
    let page = page.to_string();
    dialog.connect_response(Some("page"), move |_, _| {
        if let Some(w) = weak.upgrade() {
            open_page(&w, &page);
        }
    });
    dialog.present(Some(&w.window));
}

// ---- Windows ---------------------------------------------------------------

#[cfg(windows)]
fn can_install_here() -> bool {
    true
}

/// Starts the installer silently, on its own: it waits for the app to quit,
/// installs over it and opens it again.
#[cfg(windows)]
fn hand_over(installer: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    std::process::Command::new(installer)
        .args(["/S", "/RELAUNCH"])
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(drop)
}

// ---- macOS -----------------------------------------------------------------

/// The HFPredict.app this program runs from (…/X.app/Contents/MacOS/bin).
#[cfg(target_os = "macos")]
fn bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let app = exe.ancestors().nth(3)?;
    (app.extension()? == "app").then(|| app.to_path_buf())
}

/// Not from the disk image, nor from where macOS moved a quarantined app to
/// run it (App Translocation, read-only), nor from a folder it cannot write.
#[cfg(target_os = "macos")]
fn can_install_here() -> bool {
    let Some(app) = bundle() else {
        return false;
    };
    let path = app.to_string_lossy();
    if path.starts_with("/Volumes/") || path.contains("/AppTranslocation/") {
        return false;
    }
    let Some(parent) = app.parent() else {
        return false;
    };
    let probe = parent.join(".hfpredict-update-check");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Swaps the .app once the app has quit, keeping the old one if anything
/// fails, then opens whichever is in place.
#[cfg(target_os = "macos")]
const SWAP: &str = r#"#!/bin/sh
pid="$1"; dmg="$2"; app="$3"
while kill -0 "$pid" 2>/dev/null; do sleep 0.5; done
mnt="$(mktemp -d)"
if hdiutil attach -nobrowse -readonly -noverify -mountpoint "$mnt" "$dmg" >/dev/null; then
    rm -rf "$app.new" "$app.old"
    if ditto "$mnt/HFPredict.app" "$app.new"; then
        if mv "$app" "$app.old"; then
            mv "$app.new" "$app" || mv "$app.old" "$app"
        fi
    fi
    hdiutil detach "$mnt" -quiet || hdiutil detach "$mnt" -force -quiet
    rm -rf "$app.new" "$app.old"
fi
rmdir "$mnt" 2>/dev/null
rm -f "$dmg" "$0"
open "$app"
"#;

#[cfg(target_os = "macos")]
fn hand_over(dmg: &Path) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    let app = bundle().ok_or_else(|| std::io::Error::other("not running from an .app"))?;
    let script = dmg.with_extension("sh");
    std::fs::write(&script, SWAP)?;
    std::process::Command::new("/bin/sh")
        .arg(&script)
        .arg(std::process::id().to_string())
        .arg(dmg)
        .arg(&app)
        // Its own process group, so it outlives the app.
        .process_group(0)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
}

// ---- Elsewhere (Flatpak and AppImage update by other means) ------------

#[cfg(not(any(windows, target_os = "macos")))]
fn can_install_here() -> bool {
    false
}

#[cfg(not(any(windows, target_os = "macos")))]
fn hand_over(_: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("this build does not update itself"))
}

/// For screenshots: the dialog as a reader would see it at start.
pub fn sample(w: &Rc<Window>) {
    let u = Update {
        version: "9.9.9".into(),
        page: format!("https://github.com/{}/releases/latest", update::REPO),
        file: String::new(),
        url: String::new(),
        size: 0,
        sums_url: String::new(),
        sig_url: String::new(),
    };
    offer(w, &u, true);
}
