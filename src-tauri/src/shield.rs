//! Keeping the window's contents out of screenshots, recordings and the app
//! switcher while a screen that holds the identity is shown.
//!
//! The recovery phrase, the quiz that checks it, the restore field and the PIN
//! pad raise it while they are on screen and lower it when they go
//! (`src/shield.ts`, which counts them). Each platform offers something
//! different, and some offer nothing:
//!
//! - **Windows and macOS**: the window's content protection
//!   (`set_content_protected`) — `WDA_EXCLUDEFROMCAPTURE` on Windows, a window
//!   that is not shared (`NSWindowSharingNone`) on macOS. Screenshots,
//!   recordings and screen sharing see the window as empty. On macOS it is the
//!   system's word that capture honours it: the window server keeps it, and a
//!   capture tool that reads the screen another way is beyond any app.
//! - **Android**: `FLAG_SECURE` on the activity's window. Screenshots and
//!   recordings come out black, the screen is not cast, and the recent-apps
//!   thumbnail is blank. Set from here over JNI, on the thread the webview
//!   runs on (the activity's), because the flags of a window are the UI
//!   thread's to change.
//! - **iOS**: there is no way for an app to refuse a screenshot or a
//!   recording. What can be kept out is the picture the system takes for the
//!   app switcher: while the shield is up, the window is covered by a plain
//!   view the moment the wallet stops being active (the scene's
//!   `sceneWillResignActive`, which tao reports as the window losing focus) —
//!   before the system takes that picture — and uncovered when it is active
//!   again ([`focus`]).
//! - **Linux**: nothing. Neither X11 nor Wayland lets a window refuse to be
//!   captured — X11 hands any client the whole screen, and on Wayland capture
//!   is the compositor's, which asks the person, not the app. The call is made
//!   and does nothing there.
//!
//! None of this is a reason to refuse a screen: an error here is dropped by the
//! interface, and the screen is shown anyway.

use serde::{Serialize, Serializer};
use tauri::{Runtime, Webview};

/// What can go wrong, as a code rather than prose.
#[derive(Debug, Clone, Copy)]
pub enum ShieldError {
    /// The platform would not do it. Never on iOS, where the shield is only a
    /// flag until the wallet stops being active.
    #[cfg_attr(target_os = "ios", allow(dead_code))]
    Refused,
}

impl ShieldError {
    const fn code(self) -> &'static str {
        match self {
            Self::Refused => "shield_refused",
        }
    }
}

impl Serialize for ShieldError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

/// Raise (`on`) or lower the shield over the calling webview's window.
///
/// Not `async`, so it runs on the main thread, which is where the window may be
/// touched on every platform — and on iOS where the cover is put up and taken
/// down.
#[tauri::command]
pub fn screen_shield<R: Runtime>(webview: Webview<R>, on: bool) -> Result<(), ShieldError> {
    #[cfg(desktop)]
    {
        // On Linux this returns without doing anything: see the module.
        webview
            .window()
            .set_content_protected(on)
            .map_err(|_| ShieldError::Refused)
    }

    #[cfg(target_os = "android")]
    {
        android::secure(&webview, on)
    }

    #[cfg(target_os = "ios")]
    {
        let _ = webview;
        ios::shield(on);
        Ok(())
    }
}

/// The window gained or lost the focus — on iOS, the scene became active or
/// is about to stop being so. Covers or uncovers it while the shield is up.
#[cfg(target_os = "ios")]
pub fn focus(focused: bool) {
    ios::focus(focused);
}

#[cfg(target_os = "android")]
mod android {
    use jni::objects::JValue;
    use tauri::{Runtime, Webview};

    use super::ShieldError;

    /// `WindowManager.LayoutParams.FLAG_SECURE`.
    const FLAG_SECURE: i32 = 0x0000_2000;

    pub fn secure<R: Runtime>(webview: &Webview<R>, on: bool) -> Result<(), ShieldError> {
        webview
            .with_webview(move |platform| {
                platform.jni_handle().exec(move |env, activity, _webview| {
                    let set = |env: &mut jni::JNIEnv| -> jni::errors::Result<()> {
                        let window = env
                            .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
                            .l()?;
                        let method = if on { "addFlags" } else { "clearFlags" };
                        env.call_method(&window, method, "(I)V", &[JValue::Int(FLAG_SECURE)])?;
                        Ok(())
                    };
                    // A Java exception left pending would take the next JNI
                    // call down with it; the screen is shown either way.
                    if set(env).is_err() {
                        let _ = env.exception_clear();
                    }
                });
            })
            .map_err(|_| ShieldError::Refused)
    }
}

#[cfg(target_os = "ios")]
mod ios {
    use std::cell::{Cell, RefCell};

    use objc2::rc::Retained;
    use objc2::MainThreadMarker;
    use objc2_ui_kit::{UIApplication, UIColor, UIView, UIViewAutoresizing, UIWindowScene};

    thread_local! {
        /// Whether a screen that holds the identity is shown. Main thread only,
        /// like everything UIKit.
        static UP: Cell<bool> = const { Cell::new(false) };
        /// The covers over the windows, while the wallet is not active.
        static COVERS: RefCell<Vec<Retained<UIView>>> = const { RefCell::new(Vec::new()) };
    }

    pub fn shield(on: bool) {
        UP.with(|up| up.set(on));
        if !on {
            uncover();
        }
    }

    pub fn focus(focused: bool) {
        if focused {
            uncover();
        } else if UP.with(Cell::get) {
            cover();
        }
    }

    /// A plain view in the system's background colour over every window of
    /// the wallet: the picture the app switcher keeps shows that, not the words.
    fn cover() {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        uncover();
        let application = UIApplication::sharedApplication(mtm);
        let mut covers = Vec::new();
        for scene in application.connectedScenes().iter() {
            let Some(scene) = scene.downcast_ref::<UIWindowScene>() else {
                continue;
            };
            for window in scene.windows().iter() {
                let view = UIView::new(mtm);
                view.setFrame(window.bounds());
                view.setAutoresizingMask(
                    UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
                );
                view.setBackgroundColor(Some(&UIColor::systemBackgroundColor()));
                window.addSubview(&view);
                covers.push(view);
            }
        }
        COVERS.with(|held| *held.borrow_mut() = covers);
    }

    fn uncover() {
        if MainThreadMarker::new().is_none() {
            return;
        }
        for view in COVERS.with(|held| std::mem::take(&mut *held.borrow_mut())) {
            view.removeFromSuperview();
        }
    }
}
