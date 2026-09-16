//! Almena ID wallet.
//!
//! The plugin set differs per platform, because the platforms themselves do:
//!
//! - `single-instance` and `window-state` only exist on desktop. A phone runs
//!   one instance of an app and manages its window itself.
//! - `biometric` only exists on mobile, where it is what answers whether this
//!   phone can recognise its owner.
//! - `sharekit` is registered on mobile alone, and it is the one plugin here
//!   that is not Tauri's: there is no first-party share sheet. A computer needs
//!   none — the log already has a path, a folder to open it in and a save
//!   dialog — so the phones get it and the desktops do not.
//! - `dialog` and `fs` exist everywhere, and only one of them is offered to the
//!   webview. `dialog` raises the system save dialog, which is how a copy of the
//!   log gets somewhere the person chooses; `fs` is registered for its Rust API
//!   alone, because it is the one call that writes both to a path and to the
//!   `content://` URI Android answers a save dialog with. The capability files
//!   grant the webview none of its commands, deliberately.
//! - `notification` exists everywhere and always goes through the notification
//!   mechanism of the host system.
//!
//! The tray is the other thing only a computer has, and it changes what closing
//! the window means — see [`tray`].
//!
//! Making an identity is [`identity`]: the words, the key they derive, and the
//! DID document that describes it.
//!
//! [`vault`] is what keeps that identity between launches, and what stands in
//! front of it: the seed encrypted on the device, opened by a PIN or — where the
//! platform has somewhere to keep a key — by the device itself.
//!
//! [`develop`] is the log, and how it leaves the device it was written on.
//!
//! [`backdrop`] is the colour behind the page and the appearance of the window
//! around it, both of which the window has to be told.

mod backdrop;
mod develop;
mod identity;
mod tray;
mod vault;
#[cfg(desktop)]
mod window;

use serde::Serialize;

/// What the frontend needs to know about the host in order to decide which
/// features it can offer. Sent to the webview instead of sniffing the user
/// agent, so the answer comes from the same compile-time switches that decide
/// which plugins are registered.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlatformInfo {
    /// Either `desktop` or `mobile`.
    kind: &'static str,
    /// The operating system, as reported by the Rust standard library.
    os: &'static str,
    /// Whether this build keeps window geometry across restarts.
    window_state: bool,
    /// Whether this build refuses to run twice at once.
    single_instance: bool,
    /// Whether this build can put an icon on the system tray.
    tray: bool,
    /// The wallet version, so the frontend does not hardcode it.
    version: &'static str,
}

#[tauri::command]
fn platform_info() -> PlatformInfo {
    PlatformInfo {
        kind: if cfg!(mobile) { "mobile" } else { "desktop" },
        os: std::env::consts::OS,
        window_state: cfg!(desktop),
        single_instance: cfg!(desktop),
        tray: cfg!(desktop),
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    // Single instance has to be registered before anything else, so it runs
    // before another plugin can react to a second launch. When one is
    // attempted, the window already open takes the focus instead.
    #[cfg(desktop)]
    {
        use tauri::Manager;

        // **Registered on the builder, not in `setup`.** Tauri creates the windows
        // declared in `tauri.conf.json` and only then runs the setup closure, so a
        // plugin registered there has already missed the one window it exists to
        // restore: its `on_window_ready` never fires, nothing is tracked, nothing
        // is saved, and the wallet opens centred at its default size every time.
        //
        // The flags are named rather than defaulted. `SIZE`, `POSITION` and
        // `MAXIMIZED` are what somebody means by "where I left it". `VISIBLE` is
        // deliberately not among them: the close button puts the wallet away on
        // the tray, so the state on exit is usually "hidden", and restoring that
        // would start the wallet with no window at all — on a desktop where the
        // tray failed to install, with no way to reach it. `DECORATIONS` and
        // `FULLSCREEN` are not things this window ever changes.
        builder = builder.plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        );

        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // With a tray on the bar, a second launch is somebody looking for a
            // wallet that is running with no window on screen. macOS asks the
            // same thing through `RunEvent::Reopen`, answered with the same call.
            window::show_main(app);
        }));

        // What the close button means on a computer with a tray: put away rather
        // than quit. Only where there is a tray to come back from — if it failed
        // to build, a close is a close and the wallet ends the way it always did.
        builder = builder.on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if tray::installed(window.app_handle()) {
                    api.prevent_close();
                    window::hide_main(window.app_handle());
                }
            }
        });
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        // The save dialog, and the filesystem call behind it. `fs` is registered
        // for its Rust API and for nothing else: the capability files grant the
        // webview none of its commands.
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // The phrase somebody is being shown while they create an identity
            // lives here, and only until they confirm it.
            identity::manage(app.handle());
            vault::manage(app.handle());

            // The log, written where the person holding the device can reach it
            // — see [`develop`]. Registered here rather than on the builder
            // because the directory is one only a running application can
            // resolve; the few milliseconds before this are not logged.
            if let Some(directory) = develop::directory(app.handle()) {
                // Before the log is opened, because this is the one moment at
                // which the file about to be written to can still be thrown
                // away — and because a phone hands out no other moment. See
                // [`develop::sweep`].
                develop::sweep(&directory);

                app.handle().plugin(
                    tauri_plugin_log::Builder::new()
                        .clear_targets()
                        .targets([
                            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Folder {
                                path: directory,
                                file_name: Some(develop::STEM.to_string()),
                            }),
                        ])
                        // Spelled out rather than inherited: the plugin's
                        // default on mobile leaves out the time and the level,
                        // and a log exported from a phone with neither is a list
                        // of sentences nobody can put in order.
                        .format(|out, message, record| {
                            out.finish(format_args!(
                                "{} [{}] {}: {}",
                                tauri_plugin_log::TimezoneStrategy::UseUtc
                                    .get_now()
                                    .format(&time::format_description::well_known::Rfc3339)
                                    .unwrap_or_default(),
                                record.level(),
                                record.target(),
                                message
                            ));
                        })
                        // **An allowlist, not a level.** This file is one the
                        // person hands to somebody else, so what may write into
                        // it is decided here and not by whichever dependency
                        // happens to call `log::info!`. A bare `Info` would let
                        // the keyring store, or anything else in the tree, put
                        // a store error into a file that leaves the device.
                        // Only this crate writes to the log.
                        .level(log::LevelFilter::Off)
                        .level_for(develop::TARGET, log::LevelFilter::Info)
                        // Two megabytes each, and three of them kept. Not
                        // `KeepOne`, which does not keep one: at rotation it
                        // deletes the file outright, so somebody asked to
                        // reproduce a fault and then send the log would hand
                        // over everything that happened after the fault and
                        // nothing before it.
                        //
                        // This bounds what the log costs and not how old it
                        // gets: six megabytes is years of a wallet somebody
                        // opens once a month. Age is the sweep above.
                        .max_file_size(2 * 1024 * 1024)
                        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(3))
                        .build(),
                )?;

                // A panic that reaches nobody is a bug report that never gets
                // written. The message goes to the log the person can hand over.
                let previous = std::panic::take_hook();
                std::panic::set_hook(Box::new(move |panic| {
                    log::error!("panic: {panic}");
                    previous(panic);
                }));

                develop::opened(
                    env!("CARGO_PKG_VERSION"),
                    std::env::consts::OS,
                    vault::home_name(app.handle()),
                );
            }

            // The window has been created and the state plugin has already put
            // it back where it was. What it cannot know is whether that place
            // still exists — see [`window::settle`].
            #[cfg(desktop)]
            window::settle(app.handle());

            // The plugin that answers whether this phone can recognise its
            // owner, which only the mobile platforms have. **Touch ID needs no
            // plugin**: on macOS the device key is held in the data protection
            // keychain and the system itself asks for the finger before handing
            // it back — see `vault::store`. On Windows and Linux the stores
            // hand their items to whoever is logged in, so there the lock is
            // the PIN alone.
            #[cfg(mobile)]
            {
                app.handle().plugin(tauri_plugin_biometric::init())?;
                // The share sheet, which only a phone has. On a computer the
                // log already has a path, a folder to reveal it in and a save
                // dialog, and a third button would do less than either.
                app.handle().plugin(tauri_plugin_sharekit::init())?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            platform_info,
            backdrop::backdrop_set,
            tray::install_tray,
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
            develop::develop_logs,
            develop::develop_logs_share,
            develop::develop_logs_save_to,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the Almena ID wallet")
        .run(|_app, _event| {
            // The Dock icon of a macOS app with no window on screen: the one way
            // back that is neither the tray nor a second launch.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                window::show_main(_app);
            }
        });
}
