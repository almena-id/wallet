//! Push: the phone is told something is waiting while the wallet is not
//! running, so somebody knows to open it.
//!
//! **The wallet only says where to knock.** It asks the system for a device
//! token (FCM on Android, APNs on iOS, through `tauri-plugin-notifications`)
//! and registers it with its mediation using the push protocol for that
//! platform (`set-device-info`, the mediator's SPEC.md §6.3). The
//! mediator then sends a visible notification whose words are keys into this
//! app's own strings — `almena_wake_title` and `almena_wake_body`, in
//! `src-tauri/push/` — so nothing in it is about any message, and nothing runs
//! here when it arrives: opening the wallet is what picks messages up.
//!
//! A mediation has one device per service and registering replaces it, so this
//! runs every time the wallet is unlocked with a mediation, and the token the
//! system hands out that day is the one the mediator holds. Desktop has no push:
//! it receives while running and syncs on start.

use serde_json::{json, Value};

use super::mediator::Mediator;
use super::peer::Peer;
use super::MessagingError;

#[cfg(target_os = "android")]
const SET_DEVICE_INFO: &str = "https://didcomm.org/push-notifications-fcm/1.0/set-device-info";
#[cfg(not(target_os = "android"))]
const SET_DEVICE_INFO: &str = "https://didcomm.org/push-notifications-apns/1.0/set-device-info";

/// What this platform's `set-device-info` says: the device `token`, or with
/// none, that there is no device any more. On iOS it also carries the PushKit
/// token calls ring through (`voip_device_token`, an Almena extension of the
/// APNs protocol); on Android a call comes through the same FCM token.
fn device_info(token: Option<&str>, voip: Option<&str>) -> Value {
    if cfg!(target_os = "android") {
        json!({"device_token": token, "device_platform": token.map(|_| "android")})
    } else {
        json!({"device_token": token, "voip_device_token": token.and(voip)})
    }
}

/// Registers `token` as the device of the inbox's mediation, or removes the
/// device with `None`.
///
/// # Errors
///
/// [`MessagingError::MediatorRefused`] from a mediator that does not push for
/// this platform, and its other errors when it cannot be reached.
pub async fn register(
    mediator: &Mediator,
    inbox: &Peer,
    token: Option<&str>,
    voip: Option<&str>,
) -> Result<(), MessagingError> {
    let reply = mediator
        .request(
            inbox,
            almena_didcomm::Message::new(SET_DEVICE_INFO, device_info(token, voip)),
        )
        .await?;
    if reply.body["status"] == "OK" {
        Ok(())
    } else {
        Err(MessagingError::MediatorRefused)
    }
}

/// The device token the system gives this app, asking the person for
/// permission the first time. `None` where there is no push, or when it was
/// refused.
#[cfg(mobile)]
pub async fn token<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<String> {
    use tauri_plugin_notifications::NotificationsExt;

    app.notifications()
        .register_for_push_notifications()
        .await
        .ok()
        .filter(|token| !token.is_empty())
}

#[cfg(not(mobile))]
pub async fn token<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) -> Option<String> {
    None
}

/// The PushKit token an incoming call rings this iPhone through, once iOS has
/// given one (`almena-call`); `None` elsewhere.
pub fn voip_token<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) -> Option<String> {
    #[cfg(mobile)]
    {
        almena_call::voip_token(_app)
    }
    #[cfg(not(mobile))]
    {
        None
    }
}

/// Tells the system this app wants no more pushes.
#[cfg(mobile)]
pub fn forget<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri_plugin_notifications::NotificationsExt;

    let _ = app.notifications().unregister_for_push_notifications();
}

#[cfg(not(mobile))]
pub fn forget<R: tauri::Runtime>(_app: &tauri::AppHandle<R>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_token_removes_the_device() {
        let removed = device_info(None, Some("beef"));
        assert!(removed["device_token"].is_null());
        if cfg!(target_os = "android") {
            assert!(removed["device_platform"].is_null());
        } else {
            // No device, no VoIP token either: one registration says it all.
            assert!(removed["voip_device_token"].is_null());
        }
        assert_eq!(device_info(Some("abc"), None)["device_token"], "abc");
    }

    #[test]
    fn an_iphone_registers_its_voip_token_beside_its_device_token() {
        let both = device_info(Some("abc"), Some("beef"));
        if cfg!(target_os = "android") {
            assert!(both.get("voip_device_token").is_none());
        } else {
            assert_eq!(both["voip_device_token"], "beef");
        }
    }
}
