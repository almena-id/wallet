//! The tray icon on a computer, its menu, and the way back to a hidden window.
//!
//! This is what makes closing the window something other than quitting: with a
//! tray on the bar the wallet goes on running with nothing on screen, so this is
//! also where somebody ends it. [`installed`] is what the close button asks
//! before it dares hide — a window that hides with no tray to come back from is
//! a window somebody has lost.
//!
//! **A computer only**, with one exception: [`install_tray`] is compiled
//! everywhere. The list of commands a build answers is one list, and a command
//! that existed on three platforms and not on two would be a rejection the
//! frontend has to expect rather than an honest *there is no bar here*.
//!
//! **The menu is named by the frontend.** Its entries are text a person reads,
//! and only that side holds the catalogues — so this is a command called once
//! the interface knows its language, rather than something done at startup.

use tauri::AppHandle;

#[cfg(desktop)]
use {
    crate::window,
    tauri::{
        menu::{Menu, MenuItem, PredefinedMenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
        Manager, Runtime,
    },
};

/// Puts nothing anywhere, on the two platforms with nowhere to put it.
///
/// Answers `false`, which is the frontend's cue that closing means closing here.
#[cfg(not(desktop))]
#[tauri::command]
pub fn install_tray<R: tauri::Runtime>(app: AppHandle<R>, open: String, quit: String) -> bool {
    let _ = (app, open, quit);
    false
}

/// Identifier of the tray icon, so it can be asked for rather than remembered.
#[cfg(desktop)]
const TRAY: &str = "main";

/// Identifier of the menu entry that brings the window back.
#[cfg(desktop)]
const OPEN: &str = "open";

/// Identifier of the menu entry that ends the wallet.
#[cfg(desktop)]
const QUIT: &str = "quit";

/// The entries of the menu, kept so they can be given their names again.
///
/// The language can change while the wallet runs, and [`install_tray`] is called
/// again when it does. Holding the entries rather than rebuilding the menu is
/// what keeps the tray's wiring untouched: the same items, renamed.
#[cfg(desktop)]
struct Entries<R: Runtime> {
    open: MenuItem<R>,
    quit: MenuItem<R>,
}

/// The glyph the tray draws. Monochrome everywhere, but not the same file.
///
/// macOS is handed the black mark as a template image, which is an alpha mask it
/// fills to suit its bar, light or dark. Windows and Linux do not tint what they
/// are given, and a black mark on the dark bar both default to is a mark nobody
/// can see — so they get the same shape painted white.
#[cfg(target_os = "macos")]
const GLYPH: &[u8] = include_bytes!("../icons/tray.png");
#[cfg(all(desktop, not(target_os = "macos")))]
const GLYPH: &[u8] = include_bytes!("../icons/tray-light.png");

/// Whether the tray icon is on the bar.
///
/// The close button asks this before hiding. If the tray failed to build, hiding
/// would put the wallet somewhere this screen has no route back to, so a close
/// stays a close and the wallet ends with it.
#[cfg(desktop)]
pub fn installed<R: tauri::Runtime>(app: &AppHandle<R>) -> bool {
    app.tray_by_id(TRAY).is_some()
}

/// Puts the tray icon on the bar, with a menu holding the entries it was handed.
///
/// `open` and `quit` are those entries' names, already translated by the caller
/// — this side has no catalogues to translate them from. Called again it renames rather than
/// builds, which is what a reload or a change of language amounts to.
///
/// Answers whether there is a tray on the bar, so the frontend can say what the
/// close button now does.
#[cfg(desktop)]
#[tauri::command]
pub fn install_tray<R: tauri::Runtime>(app: AppHandle<R>, open: String, quit: String) -> bool {
    if installed(&app) {
        rename(&app, &open, &quit);
        return true;
    }

    build(&app, &open, &quit).is_ok()
}

/// Gives the entries their names again, in whatever language the interface now
/// shows.
///
/// A rename that fails leaves the word that was there before, which is a menu in
/// the wrong language and not a menu anybody has lost.
#[cfg(desktop)]
fn rename<R: tauri::Runtime>(app: &AppHandle<R>, open: &str, quit: &str) {
    if let Some(entries) = app.try_state::<Entries<R>>() {
        let _ = entries.open.set_text(open);
        let _ = entries.quit.set_text(quit);
    }
}

/// Builds the icon and the menu, and hangs both behaviours off it.
///
/// # Errors
///
/// Whatever Tauri raised: the glyph not decoding, the menu not building, or the
/// platform refusing a tray icon — on Linux, most often because nothing on the
/// desktop is serving one.
#[cfg(desktop)]
fn build<R: tauri::Runtime>(app: &AppHandle<R>, open: &str, quit: &str) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(app, OPEN, open, true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, QUIT, quit, true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open_item, &separator, &quit_item])?;

    TrayIconBuilder::with_id(TRAY)
        .icon(tauri::image::Image::from_bytes(GLYPH)?)
        .icon_as_template(cfg!(target_os = "macos"))
        .menu(&menu)
        // The left button is the way back to the window, so it must not be spent
        // opening the menu as well. The menu stays on the right button, where
        // every platform puts it.
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                window::show_main(tray.app_handle());
            }
        })
        // The open entry is the left click again, for whoever looks for it in the
        // menu — and the only way back on a Linux bar that never reports clicks.
        .on_menu_event(|app, event| {
            if event.id() == OPEN {
                window::show_main(app);
            } else if event.id() == QUIT {
                app.exit(0);
            }
        })
        .build(app)?;

    app.manage(Entries {
        open: open_item,
        quit: quit_item,
    });

    Ok(())
}
