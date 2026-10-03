//! Ringing for a call while the wallet is closed.
//!
//! A locked wallet holds no seed, so it cannot read what arrives: the caller
//! marks the offer's `forward` as a call (`Urgency::Call` in almena-didcomm)
//! and the mediator, which can see that much and no more, sends a call push
//! instead of the generic wake. This plugin is what answers it:
//!
//! - **Android**: the push is a data-only FCM message, taken by this plugin's
//!   messaging service (`CallMessagingService`, ahead of the notifications
//!   plugin's, to which it passes everything else) and shown as a full-screen
//!   incoming call with Answer and Decline.
//! - **iOS**: a PushKit VoIP push, reported to CallKit at once, as Apple
//!   requires of every VoIP push. Its token goes to the mediator beside the
//!   ordinary one ([`voip_token`]).
//!
//! Either way the call cannot say who is calling — nothing on the phone can
//! read that yet — and answering it opens the wallet, which asks to be
//! unlocked, picks the offer up and answers it (`take`, in `src/ring.ts`).

use tauri::plugin::{Builder, TauriPlugin};
use tauri::Runtime;

#[cfg(mobile)]
use tauri::plugin::PluginHandle;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_almena_call);

#[cfg(mobile)]
struct Call<R: Runtime>(PluginHandle<R>);

/// Registers the plugin; on a computer it is empty.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("almena-call")
        .setup(|_app, _api| {
            #[cfg(target_os = "android")]
            {
                use tauri::Manager as _;
                let handle = _api.register_android_plugin("id.almena.call", "CallPlugin")?;
                _app.manage(Call(handle));
            }
            #[cfg(target_os = "ios")]
            {
                use tauri::Manager as _;
                let handle = _api.register_ios_plugin(init_plugin_almena_call)?;
                _app.manage(Call(handle));
            }
            Ok(())
        })
        .build()
}

/// The PushKit token the mediator rings this iPhone through, once iOS has
/// given one; `None` elsewhere and until then.
pub fn voip_token<R: Runtime>(_app: &tauri::AppHandle<R>) -> Option<String> {
    #[cfg(target_os = "ios")]
    {
        use tauri::Manager as _;
        #[derive(serde::Deserialize)]
        struct Token {
            token: Option<String>,
        }
        let call = _app.try_state::<Call<R>>()?;
        call.0
            .run_mobile_plugin::<Token>("voipToken", ())
            .ok()
            .and_then(|answer| answer.token)
    }
    #[cfg(not(target_os = "ios"))]
    None
}
