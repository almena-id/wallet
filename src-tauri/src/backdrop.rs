//! The colour behind the webview.
//!
//! A webview is a rectangle drawn on top of a native view, and that view has a
//! colour of its own that the page cannot declare. It shows wherever the page is
//! not painted yet — most visibly while a tablet is being turned, because the
//! window changes shape a moment before the page has been laid out again inside
//! it. Left alone it is white, and a white sheet flashing behind a dark wallet
//! is what somebody rotating a device sees.
//!
//! **The value is not written down here.** The stylesheet is the only place this
//! project names a colour, so the interface reads back what it is actually
//! painting and hands that across — see `src/backdrop.ts`. This side only
//! forwards it, which is why the argument is four numbers and not a palette.
//!
//! It is a command of this application's rather than the one Tauri already has,
//! because Tauri's is registered `#[cfg(desktop)]`: `webview.setBackgroundColor`
//! from the interface rejects on a phone, which is the one place it is needed.
//! The Rust API underneath has no such restriction and reaches both platforms.

use serde::{Serialize, Serializer};
use tauri::utils::config::Color;
use tauri::{Runtime, Webview};

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

/// Paint the window behind the page the same colour the page is painting.
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
) -> Result<(), BackdropError> {
    webview
        .set_background_color(Some(Color(red, green, blue, alpha)))
        .map_err(|_| BackdropError::Refused)
}
