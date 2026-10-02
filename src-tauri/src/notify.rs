//! The system's notifications, for messages that arrive while nobody is
//! looking at the wallet.
//!
//! **A computer only, and only while it runs.** Live delivery is what receives
//! a message there — the wallet on the tray, or minimised — so this is told of
//! each new one from [`crate::messaging`] and says so through the official
//! notification plugin. On a phone live delivery stops when the wallet leaves
//! the screen, and what arrives then is announced by the mediator's push; the
//! notification plugin could not be registered there anyway, because the push
//! plugin is a fork of it and both claim the same native symbols.
//!
//! **How much is said is the person's choice** ([`Privacy`]), made in the
//! interface and handed over with the words in their language — this side has
//! no catalogues — through [`notifications_configure`]. Whatever is shown is
//! kept by the system in its own history, outside the wallet's sealed storage,
//! which is why the choice exists.

use std::sync::Mutex;

use serde::Deserialize;
use tauri::{Runtime, State};

/// One message that arrived: who it is from, as the wallet calls them, and
/// what it says.
// Read only where notifications are shown, on a computer.
#[cfg_attr(not(desktop), allow(dead_code))]
pub struct Fresh {
    pub from: String,
    pub content: String,
}

/// How much a notification says.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Privacy {
    /// Who it is from, and what it says.
    Full,
    /// Who it is from.
    #[default]
    Name,
    /// That something arrived, and nothing else.
    None,
}

/// The choice, and the words to say it with. Empty until the interface has
/// configured it, and nothing is shown until it has.
#[derive(Default)]
pub struct Notices(Mutex<Config>);

#[derive(Default)]
// Read only where notifications are shown, on a computer.
#[cfg_attr(not(desktop), allow(dead_code))]
struct Config {
    privacy: Privacy,
    /// "New message", in the language the interface is read in.
    new_message: String,
    /// The wallet's name, for a notification that names nobody.
    app_name: String,
}

/// Sets how much notifications say, and the words they say it with. Called by
/// the interface on start and whenever the choice or the language changes.
#[tauri::command]
pub fn notifications_configure(
    notices: State<'_, Notices>,
    privacy: Privacy,
    new_message: String,
    app_name: String,
) {
    *notices
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Config {
        privacy,
        new_message,
        app_name,
    };
}

/// What to show for `fresh`, as `(title, body)` pairs: one per message, or a
/// single one that names nobody.
// Read only where notifications are shown, on a computer.
#[cfg_attr(not(desktop), allow(dead_code))]
fn notices(config: &Config, fresh: &[Fresh]) -> Vec<(String, String)> {
    if fresh.is_empty() || config.new_message.is_empty() {
        return Vec::new();
    }
    match config.privacy {
        Privacy::None => vec![(config.app_name.clone(), config.new_message.clone())],
        Privacy::Name => fresh
            .iter()
            .map(|message| (message.from.clone(), config.new_message.clone()))
            .collect(),
        Privacy::Full => fresh
            .iter()
            .map(|message| (message.from.clone(), message.content.clone()))
            .collect(),
    }
}

/// Tells the system about messages that arrived, unless the wallet is what
/// somebody is looking at — then the conversation already shows them.
#[cfg(desktop)]
pub fn arrived<R: Runtime>(app: &tauri::AppHandle<R>, fresh: &[Fresh]) {
    use tauri::Manager;
    use tauri_plugin_notification::NotificationExt;

    if crate::window::main(app).is_some_and(|window| window.is_focused().unwrap_or(false)) {
        return;
    }
    let Some(state) = app.try_state::<Notices>() else {
        return;
    };
    let shown = {
        let config = state
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        notices(&config, fresh)
    };
    for (title, body) in shown {
        let _ = app.notification().builder().title(title).body(body).show();
    }
}

#[cfg(not(desktop))]
pub fn arrived<R: Runtime>(_app: &tauri::AppHandle<R>, _fresh: &[Fresh]) {}

/// Registers the choice, empty until the interface configures it.
pub fn manage<R: Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    app.manage(Notices::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(privacy: Privacy) -> Config {
        Config {
            privacy,
            new_message: "New message".into(),
            app_name: "Almena Wallet".into(),
        }
    }

    fn fresh() -> Vec<Fresh> {
        vec![
            Fresh {
                from: "Lucía".into(),
                content: "¿Nos vemos a las diez?".into(),
            },
            Fresh {
                from: "Papá".into(),
                content: "Perfecto".into(),
            },
        ]
    }

    #[test]
    fn each_level_says_exactly_what_it_promises() {
        assert_eq!(
            notices(&config(Privacy::Full), &fresh())[0],
            ("Lucía".into(), "¿Nos vemos a las diez?".into())
        );
        let named = notices(&config(Privacy::Name), &fresh());
        assert_eq!(named.len(), 2);
        assert!(named.iter().all(|(_, body)| body == "New message"));
        // Nothing identifying, and one notification however many arrived.
        assert_eq!(
            notices(&config(Privacy::None), &fresh()),
            vec![("Almena Wallet".into(), "New message".into())]
        );
    }

    #[test]
    fn nothing_is_shown_before_the_interface_configured_it() {
        assert!(notices(&Config::default(), &fresh()).is_empty());
        assert!(notices(&config(Privacy::Full), &[]).is_empty());
    }
}
