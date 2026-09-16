//! The colour behind the webview, and the appearance of the window around it.
//!
//! A webview is a rectangle drawn on top of a native view, and that view has a
//! colour of its own that the page cannot declare. It shows wherever the page is
//! not painted yet — most visibly while a tablet is being turned, because the
//! window changes shape a moment before the page has been laid out again inside
//! it. Left alone it is white, and a white sheet flashing behind a dark wallet
//! is what somebody rotating a device sees.
//!
//! On a computer the window itself is a second surface with the same problem,
//! and a more visible one: the title bar. Since macOS Tahoe a standard title bar
//! is a slab of the system's own grey, drawn in whichever of light or dark the
//! window is in, and over a dark page that is a light band with a hard edge
//! under it. The window is declared with `titleBarStyle: "Transparent"` in
//! `tauri.conf.json`, which makes the bar show the window's *background colour*
//! instead — the page stays below it, nothing is drawn underneath the title —
//! and so the window is painted the page's colour here too, not only the
//! webview. `tauri.conf.json` gives it the light background so the first frame
//! is not white; on macOS the webview setter is not implemented, so without this
//! the bar would keep that light colour under a dark page.
//!
//! The window is also told which appearance it is wearing, because the bar's
//! text and controls are drawn by the system in the appearance it believes the
//! window to be in — the system's setting, until the wallet says otherwise —
//! and a dark title over a dark page is somebody who chose dark against a light
//! system.
//!
//! **The value is not written down here.** The stylesheet is the only place this
//! project names a colour, so the interface reads back what it is actually
//! painting and hands that across — see `src/backdrop.ts`. This side only
//! forwards it, which is why the argument is four numbers and not a palette.
//!
//! It is a command of this application's rather than the ones Tauri already has,
//! because Tauri's are registered `#[cfg(desktop)]`: `webview.setBackgroundColor`
//! from the interface rejects on a phone, which is the one place it is needed.
//! The Rust API underneath has no such restriction and reaches both platforms.

use serde::{Serialize, Serializer};
use tauri::utils::config::Color;
use tauri::{Runtime, Theme, Webview};

/// What can go wrong, as a code rather than prose.
#[derive(Debug, Clone, Copy)]
pub enum BackdropError {
    /// The platform would not take the colour.
    Refused,
}

impl BackdropError {
    const fn code(self) -> &'static str {
        match self {
            Self::Refused => "backdrop_refused",
        }
    }
}

impl Serialize for BackdropError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

/// Paint the window behind the page the same colour the page is painting, and
/// dress its chrome to match.
///
/// `theme` is the palette the page is on — `light` or `dark` — or nothing when
/// the wallet is following the system, in which case the window is handed back
/// to the system too. It is the interface's choice and not `prefers-color-scheme`
/// that is sent, because the two differ exactly when somebody picked a theme
/// against the system's, and that is the case the chrome has to be told about.
///
/// Answers the calling webview rather than looking one up by name: the wallet
/// has exactly one, and asking for it by label would be a second place that has
/// to agree with `tauri.conf.json`.
#[tauri::command]
pub fn backdrop_set<R: Runtime>(
    webview: Webview<R>,
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
    theme: Option<Theme>,
) -> Result<(), BackdropError> {
    let colour = Some(Color(red, green, blue, alpha));

    // The window before the webview: on the desktops it is the one that shows,
    // and a webview that will not take the colour is not worth losing it over.
    #[cfg(desktop)]
    {
        let window = webview.window();
        window
            .set_background_color(colour)
            .map_err(|_| BackdropError::Refused)?;
        window
            .set_theme(theme)
            .map_err(|_| BackdropError::Refused)?;
    }
    #[cfg(not(desktop))]
    let _ = theme;

    webview
        .set_background_color(colour)
        .map_err(|_| BackdropError::Refused)
}
