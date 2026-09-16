//! The second window an iPad offers, and why it is turned down.
//!
//! The wallet has one window. The approval screen, the keypad and the twelve
//! words are a single column read in a single place, and a second copy of it
//! beside the first would be two views of one vault, each believing it is the
//! one that was unlocked.
//!
//! An iPad offers a second window anyway, because `Info.ios.plist` says the
//! application supports multiple scenes — and it says so because UIKit on the
//! iOS 27 SDK will not launch an application without a scene manifest, and tao,
//! the window layer under Tauri, only installs its scene delegate when the
//! manifest sets that flag. The flag is the price of starting; this module is
//! what keeps it from meaning anything.
//!
//! So when the system connects a scene that no window of ours asked for — the
//! person long-pressed the icon and chose *New Window*, or dragged the wallet
//! out in Stage Manager — the session behind it is asked to be destroyed
//! before it draws. The first scene is never reported here: tao emits the event
//! only for scenes beyond the one the wallet's window lives in.

use objc2::MainThreadMarker;
use objc2_ui_kit::{UIApplication, UIScene};

/// Close the scene the system just opened for a second window.
///
/// Answered from the scene's own connection callback, which is where tao
/// reports it, and on the main thread, which is the only thread UIKit's
/// application object may be reached from. The destruction is requested and
/// not performed: UIKit tears the scene down on its own schedule, and there is
/// nothing to do if it declines.
pub fn refuse(scene: &UIScene) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let application = UIApplication::sharedApplication(mtm);
    application.requestSceneSessionDestruction_options_errorHandler(&scene.session(), None, None);
}
