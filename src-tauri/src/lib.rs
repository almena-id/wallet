//! Almena Wallet.
//!
//! One code base for Android, iOS, macOS, Linux and Windows. What differs per
//! platform is decided here with `cfg`, never by sniffing in the front end —
//! which is why the front end asks [`platform_info`] instead.
//!
//! The plugin set differs per platform, because the platforms themselves do:
//!
//! - `single-instance` and `window-state` only exist on a computer. A phone runs
//!   one instance of an app and manages its window itself.
//! - `barcode-scanner` only exists on a phone or a tablet, where there is a
//!   camera the system lets an app drive to read a code.
//! - `autostart` only exists on a computer: the Security switch that opens the
//!   wallet at login, minimised.
//! - `notification` only exists on a computer: messages that arrive while the
//!   wallet is not in front are told to the system, saying as much as the
//!   person chose ([`notify`]). A phone is told by push instead, and the push
//!   plugin — a fork of this one — takes its native symbols there.
//! - `deep-link` exists everywhere: an `almena://` link — the scheme the
//!   wallet's own invitations are written with — opens the wallet. On a
//!   computer a link opened while it runs arrives through `single-instance`.
//!
//! The tray is the other thing only a computer has, and it changes what closing
//! the window means: with a tray on the bar the wallet goes on running — live
//! delivery included — with nothing on screen, and the tray is where it is
//! ended. See [`tray`].
//!
//! [`window`] takes the window off the screen and brings it back, and puts a
//! restored window back on a screen that exists.
//!
//! [`backdrop`] is the colour behind the page and the appearance of the window
//! around it, both of which the window has to be told.
//!
//! [`scene`] is the second window an iPad offers, and why it is turned down.
//!
//! [`identity`] makes an identity from a phrase and holds its seed while the
//! wallet is open; [`vault`] keeps that seed on the device between launches,
//! encrypted behind a PIN. The seed never crosses to the front end.
//!
//! [`messaging`] is the wallet's mediation with a DIDComm mediator and the
//! relationships it has through it.

mod backdrop;
mod identity;
mod messaging;
mod notify;
mod registry;
#[cfg(target_os = "ios")]
mod scene;
mod tray;
mod vault;
#[cfg(desktop)]
mod window;

use serde::Serialize;

/// What the front end needs to know about the host to decide what it offers,
/// answered from the same compile-time switches that register the plugins.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlatformInfo {
    /// `desktop` or `mobile`.
    kind: &'static str,
    /// Whether this build can read a code with the camera.
    barcode_scanner: bool,
}

#[tauri::command]
fn platform_info() -> PlatformInfo {
    PlatformInfo {
        kind: if cfg!(mobile) { "mobile" } else { "desktop" },
        barcode_scanner: cfg!(mobile),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();

    // **Single instance first**, so it answers a second launch before any other
    // plugin reacts to it: the window already open takes the focus, and a
    // `almena://` link the second launch carried is handed to the deep-link
    // plugin of this one (its `deep-link` feature).
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        window::show_main(app);
    }));

    // **On the builder, not in `setup`.** Tauri creates the windows declared in
    // `tauri.conf.json` before `setup` runs, so a plugin registered there has
    // already missed the one window it exists to restore. The flags are named:
    // size, position and maximised are what "where I left it" means; the
    // window is never hidden or made full screen, so those are not kept.
    #[cfg(desktop)]
    let builder = builder.plugin(
        tauri_plugin_window_state::Builder::default()
            .with_state_flags(
                tauri_plugin_window_state::StateFlags::SIZE
                    | tauri_plugin_window_state::StateFlags::POSITION
                    | tauri_plugin_window_state::StateFlags::MAXIMIZED,
            )
            .build(),
    );

    // Opening at login, which Security turns on and off. The login item starts
    // the wallet with `--minimized`, and `setup` below keeps it out of the way.
    // A LaunchAgent on macOS, which needs no permission to run AppleScript.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent,
        Some(vec![window::MINIMIZED]),
    ));

    // The system's notifications, for messages that arrive with the wallet on
    // the tray or minimised — see `notify`.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_notification::init());

    // The camera, where there is one the wallet may drive.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());

    let builder = builder.plugin(tauri_plugin_deep_link::init());

    // What the close button means on a computer with a tray: put away rather
    // than quit. Only where there is a tray to come back from — if it failed to
    // build, a close is a close and the wallet ends the way it always did.
    #[cfg(desktop)]
    let builder = builder.on_window_event(|window, event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            use tauri::Manager;
            if tray::installed(window.app_handle()) {
                api.prevent_close();
                window::hide_main(window.app_handle());
            }
        }
    });
    // Push: the device token the mediator notifies. Mobile only — see
    // `messaging::push`; desktop has no push.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_notifications::init());

    builder
        .plugin(tauri_plugin_opener::init())
        // Writing to the clipboard: the webview's own `navigator.clipboard`
        // is refused on iOS. Only `write-text` is granted — see capabilities.
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Who opens `almena://`, where the system is not told by the
            // bundle. **Linux, always:** an AppImage is never installed, so the
            // app registers itself (a `.desktop` handler and `xdg-mime`); a
            // `.deb` or `.rpm` already declares it. **Windows, in development
            // only:** the installer writes the registry keys, and a build run
            // from `target/` must not take the scheme from the installed one.
            // macOS and the phones read it from the bundle.
            #[cfg(any(target_os = "linux", all(windows, debug_assertions)))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let _ = app.deep_link().register_all();
            }

            // The state plugin put the window back where it was; whether that
            // place still exists is `window::settle`'s question.
            #[cfg(desktop)]
            window::settle(app.handle());

            identity::manage(app.handle());
            vault::manage(app.handle());
            messaging::manage(app.handle());
            notify::manage(app.handle());
            registry::manage(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            platform_info,
            notify::notifications_configure,
            tray::install_tray,
            backdrop::backdrop_set,
            identity::identity_draft,
            identity::identity_create,
            identity::identity_restore,
            identity::identity_discard,
            identity::identity_forget,
            vault::vault_status,
            vault::vault_create,
            vault::vault_open,
            vault::vault_open_with_device,
            vault::vault_change_pin,
            vault::vault_set_device,
            vault::vault_destroy,
            messaging::mediator_status,
            messaging::mediator_connect,
            messaging::mediator_check,
            messaging::mediator_disconnect,
            messaging::contacts_list,
            messaging::invitation_show,
            messaging::invitation_kind,
            messaging::link_details,
            messaging::contact_accept,
            messaging::messages_sync,
            messaging::conversation_read,
            messaging::conversation_seen,
            messaging::conversation_clear,
            messaging::message_send,
            messaging::message_retry,
            messaging::contact_rename,
            messaging::profile_read,
            messaging::profile_write,
            messaging::call_ice_servers,
            messaging::call_send,
            messaging::profile_photo_read,
            messaging::profile_photo_write,
            messaging::live::live_start,
            messaging::live::live_stop,
            messaging::push_register,
            messaging::push_unregister,
            registry::registry_request,
            registry::registry_answer,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the Almena Wallet")
        .run(|_app, _event| {
            // Started by the login item: open, but minimised. Here and not in
            // `setup`, which runs before the system has put the window on the
            // screen — a window minimised then is shown anyway.
            #[cfg(desktop)]
            if let tauri::RunEvent::Ready = _event {
                if std::env::args().any(|arg| arg == window::MINIMIZED) {
                    window::minimize_main(_app);
                }
            }

            // The Dock icon of a wallet with no window on screen: the way back
            // that is neither the tray nor a second launch.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                window::show_main(_app);
            }

            // An iPad opening a second window of a wallet that has one — see
            // [`scene`].
            #[cfg(target_os = "ios")]
            if let tauri::RunEvent::SceneRequested { scene, .. } = &_event {
                scene::refuse(scene);
            }
        });
}
