//! The main window on a computer: taking it off the screen, and bringing it back.
//!
//! Compiled for the desktops only. A phone has a single surface owned by the
//! system, and an app that took itself off it would be an app that had quit.

use tauri::{AppHandle, Emitter, Manager, Window};

/// Label of the main window, as declared under `app.windows` in `tauri.conf.json`.
const MAIN_WINDOW: &str = "main";

/// What the interface is told when the window comes back, so the lock can be
/// asked whether the time ran out while it was away.
///
/// **The webview has no other way of knowing.** On a phone it watches
/// `visibilitychange`, which the system fires when the app stops being what
/// somebody is looking at and again when it goes back to being it. A desktop
/// webview is *visible* the whole time its window exists — a window put away on
/// the tray and brought back is not something the page inside is told — so
/// without this the tray would be a way to leave a wallet past its auto-lock and
/// find it open, its timer having been run late or not at all by a browser that
/// throttles what nobody can see.
pub const SHOWN: &str = "window-shown";

/// The main window, or nothing when there is none.
pub fn main<R: tauri::Runtime>(app: &AppHandle<R>) -> Option<Window<R>> {
    app.get_webview_window(MAIN_WINDOW)
        .map(|window| window.as_ref().window())
}

/// Brings the window back to whoever asked for it.
///
/// Three callers, all meaning the same thing: the tray icon, a second launch of
/// the wallet, and the macOS Dock icon of an app with no window on screen. Every
/// step is needed — the window may be hidden, minimised, or merely behind
/// something else — and none is worth failing over: the worst case is that
/// somebody keeps looking at what they were already looking at.
pub fn show_main<R: tauri::Runtime>(app: &AppHandle<R>) {
    let Some(window) = main(app) else {
        return;
    };

    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    // Announced last, once there is something on the screen to have come back
    // to: what the interface does with it is check a clock, and a wallet whose
    // time ran out is one somebody must not be shown before it lets go.
    let _ = app.emit(SHOWN, ());
}

/// Takes the window off the screen without ending the wallet.
///
/// One caller: the close button, which stops meaning *quit* once there is a tray
/// to come back from. `lib.rs` only routes a close here when the tray is on the
/// bar — see [`crate::tray::installed`].
pub fn hide_main<R: tauri::Runtime>(app: &AppHandle<R>) {
    if let Some(window) = main(app) {
        // Nothing is announced. Putting the wallet on the tray is not using it,
        // which is what the lock's own clock counts — it goes on running while
        // the window is away, and [`show_main`] is where the interface finds out
        // whether it ran out.
        let _ = window.hide();
    }
}

/// Puts a restored window back on a screen that exists.
///
/// **The state plugin remembers a rectangle, not a screen.** It already refuses
/// to restore a position that lands on no monitor, leaving the placement to the
/// system — but it restores the *size* unconditionally, and that is the half
/// that goes wrong when the screens change. A wallet last closed full-height on
/// a 27-inch monitor comes back at that height on a laptop opened on its own:
/// taller than the display it is on, with its own buttons somewhere below the
/// bezel and no way to drag it back into view.
///
/// So the size is brought inside whichever screen the window ended up on, and a
/// window that landed on none is centred on the first one there is. A window
/// that came back maximised is left alone: the system already sized that to a
/// screen that exists.
pub fn settle<R: tauri::Runtime>(app: &AppHandle<R>) {
    let Some(window) = main(app) else {
        return;
    };

    if window.is_maximized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
        return;
    }

    let (Ok(position), Ok(size), Ok(monitors)) = (
        window.outer_position(),
        window.outer_size(),
        window.available_monitors(),
    ) else {
        return;
    };

    // The work area rather than the whole screen: a window the height of the
    // display is a window with its foot behind the dock.
    let screens: Vec<Rect> = monitors
        .iter()
        .map(|monitor| {
            let area = monitor.work_area();
            Rect {
                x: area.position.x,
                y: area.position.y,
                width: area.size.width,
                height: area.size.height,
            }
        })
        .collect();

    // The smallest this window is allowed to be, taken from the same declaration
    // the window itself was built from rather than repeated here, so the two
    // cannot drift apart. It is in logical pixels, which are not what the screens
    // are measured in on a display that scales.
    let scale = window.scale_factor().unwrap_or(1.0);
    let declared = app
        .config()
        .app
        .windows
        .iter()
        .find(|configured| configured.label == MAIN_WINDOW);
    let minimum = (
        (declared.and_then(|w| w.min_width).unwrap_or(0.0) * scale).round() as u32,
        (declared.and_then(|w| w.min_height).unwrap_or(0.0) * scale).round() as u32,
    );

    let current = Rect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };

    let Some(fitted) = fit(current, &screens, minimum) else {
        return;
    };

    // Size first: moving a window that is still too big only moves the part of
    // it that is off the screen.
    let _ = window.set_size(tauri::PhysicalSize {
        width: fitted.width,
        height: fitted.height,
    });
    let _ = window.set_position(tauri::PhysicalPosition {
        x: fitted.x,
        y: fitted.y,
    });
}

/// A rectangle, which both a window and a screen are.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Rect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl Rect {
    /// How much of this rectangle and another one are the same place.
    ///
    /// Used to choose which screen a window belongs to when it spans two, and to
    /// find out that it belongs to none.
    const fn overlap(self, other: Self) -> u64 {
        let left = if self.x > other.x { self.x } else { other.x };
        let top = if self.y > other.y { self.y } else { other.y };
        let self_right = self.x + self.width as i32;
        let other_right = other.x + other.width as i32;
        let right = if self_right < other_right {
            self_right
        } else {
            other_right
        };
        let self_bottom = self.y + self.height as i32;
        let other_bottom = other.y + other.height as i32;
        let bottom = if self_bottom < other_bottom {
            self_bottom
        } else {
            other_bottom
        };

        if right <= left || bottom <= top {
            return 0;
        }

        (right - left) as u64 * (bottom - top) as u64
    }
}

/// Whether every corner of a window is on some screen.
///
/// **Across all of them, not one of them.** A window deliberately straddling the
/// join between two monitors is entirely visible and entirely usable, and
/// dragging it onto whichever screen holds more of it would be the wallet
/// undoing something somebody chose.
fn covered(window: Rect, screens: &[Rect]) -> bool {
    let right = window.x + window.width as i32 - 1;
    let bottom = window.y + window.height as i32 - 1;

    [
        (window.x, window.y),
        (right, window.y),
        (window.x, bottom),
        (right, bottom),
    ]
    .into_iter()
    .all(|(x, y)| {
        screens.iter().any(|screen| {
            x >= screen.x
                && x < screen.x + screen.width as i32
                && y >= screen.y
                && y < screen.y + screen.height as i32
        })
    })
}

/// Where a window belongs, once the screens it was remembered on may be gone.
///
/// Answers `None` when it is already somewhere it can be seen and reached, so
/// that the ordinary case — the same computer, the same screens — moves nothing
/// at all. Only two things are worth correcting: a window bigger than the screen
/// it is on, and a window somebody could not drag back into view.
fn fit(window: Rect, screens: &[Rect], minimum: (u32, u32)) -> Option<Rect> {
    // The screen the window is most on. With no overlap anywhere it is on none
    // of them, and that screen is where it starts again.
    let screen = screens
        .iter()
        .copied()
        .max_by_key(|screen| window.overlap(*screen))?;
    let adrift = window.overlap(screen) == 0;

    // The minimum gives way to a screen too small to honour it: a window pushed
    // below the size somebody can use is still better than one they cannot see.
    let width = window
        .width
        .clamp(minimum.0.min(screen.width), screen.width);
    let height = window
        .height
        .clamp(minimum.1.min(screen.height), screen.height);

    let resized = Rect {
        width,
        height,
        ..window
    };

    if !adrift && covered(resized, screens) {
        return (resized != window).then_some(resized);
    }

    let (x, y) = if adrift {
        // Centred, because there is no telling where it was meant to be.
        (
            screen.x + (screen.width.saturating_sub(width) / 2) as i32,
            screen.y + (screen.height.saturating_sub(height) / 2) as i32,
        )
    } else {
        (
            window
                .x
                .min(screen.x + screen.width as i32 - width as i32)
                .max(screen.x),
            window
                .y
                .min(screen.y + screen.height as i32 - height as i32)
                .max(screen.y),
        )
    };

    let fitted = Rect {
        x,
        y,
        width,
        height,
    };

    (fitted != window).then_some(fitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAPTOP: Rect = Rect {
        x: 0,
        y: 0,
        width: 1512,
        height: 916,
    };
    /// A second screen standing to the left of the laptop, as one does.
    const EXTERNAL: Rect = Rect {
        x: -2560,
        y: -200,
        width: 2560,
        height: 1440,
    };
    /// What `tauri.conf.json` declares, which is what `settle` reads at runtime.
    const MINIMUM: (u32, u32) = (380, 620);

    #[test]
    fn a_window_where_it_was_left_is_not_moved() {
        let window = Rect {
            x: 100,
            y: 80,
            width: 960,
            height: 720,
        };

        assert_eq!(fit(window, &[LAPTOP, EXTERNAL], MINIMUM), None);
    }

    #[test]
    fn a_window_from_a_screen_that_is_gone_comes_back_to_one_that_is_not() {
        // Left on the external monitor, opened on the laptop alone.
        let window = Rect {
            x: -1800,
            y: -100,
            width: 960,
            height: 720,
        };

        let fitted = fit(window, &[LAPTOP], MINIMUM).expect("it is nowhere");
        assert!(fitted.overlap(LAPTOP) > 0);
        assert_eq!(fitted.width, 960);
        assert_eq!(fitted.height, 720);
    }

    #[test]
    fn a_window_taller_than_the_screen_is_brought_inside_it() {
        // Full height on the big monitor, opened on the laptop.
        let window = Rect {
            x: 10,
            y: 0,
            width: 1200,
            height: 1400,
        };

        let fitted = fit(window, &[LAPTOP], MINIMUM).expect("it does not fit");
        assert_eq!(fitted.height, LAPTOP.height);
        assert!(fitted.y >= LAPTOP.y);
        assert!(fitted.y + fitted.height as i32 <= LAPTOP.y + LAPTOP.height as i32);
    }

    #[test]
    fn a_window_hanging_off_the_edge_is_pulled_back_without_being_resized() {
        let window = Rect {
            x: 1400,
            y: 700,
            width: 960,
            height: 720,
        };

        let fitted = fit(window, &[LAPTOP], MINIMUM).expect("it hangs off");
        assert_eq!((fitted.width, fitted.height), (960, 720));
        assert_eq!(fitted.x + 960, LAPTOP.width as i32);
        assert_eq!(fitted.y + 720, LAPTOP.height as i32);
    }

    #[test]
    fn a_window_spanning_two_screens_is_left_where_somebody_put_it() {
        // Mostly on the external monitor, its right edge over the laptop.
        let window = Rect {
            x: -900,
            y: 100,
            width: 960,
            height: 720,
        };

        // Every corner of it is on one screen or the other, so it is visible and
        // it is reachable, and the wallet has no business tidying it onto one.
        assert_eq!(fit(window, &[LAPTOP, EXTERNAL], MINIMUM), None);
    }

    #[test]
    fn a_screen_smaller_than_the_minimum_still_gets_a_window() {
        // Nothing ships a display this small, but the arithmetic must not send
        // the window off it in the name of a minimum it cannot honour.
        let tiny = Rect {
            x: 0,
            y: 0,
            width: 320,
            height: 400,
        };
        let window = Rect {
            x: 0,
            y: 0,
            width: 960,
            height: 720,
        };

        let fitted = fit(window, &[tiny], MINIMUM).expect("it does not fit");
        assert_eq!((fitted.width, fitted.height), (tiny.width, tiny.height));
    }

    #[test]
    fn with_no_screens_at_all_nothing_is_decided() {
        let window = Rect {
            x: 0,
            y: 0,
            width: 960,
            height: 720,
        };

        assert_eq!(fit(window, &[], MINIMUM), None);
    }
}
