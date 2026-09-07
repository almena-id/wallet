import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

/** The event the Rust side sends when the window is put away on the tray. */
const WINDOW_HIDDEN = "window-hidden";

/**
 * The moment the wallet leaves the screen.
 *
 * **Locking is letting go, not hiding.** There is no flag that says the wallet
 * is closed while the seed sits behind it in memory: going out of sight drops
 * the identity, and coming back opens the record again with a PIN or a face.
 * A wallet backgrounded on a phone is a wallet whose secret is no longer in this
 * process at all, which is a stronger thing to say than that a screen is over
 * it — and it costs one derivation to come back, which is the point.
 *
 * Two ways it happens: the system taking the wallet off the screen, and the
 * window being put away on the tray, which the webview is never told about.
 */
export function useOutOfSight(away: () => void) {
  useEffect(() => {
    const onVisibility = () => {
      if (document.hidden) {
        away();
      }
    };

    document.addEventListener("visibilitychange", onVisibility);
    let stop: (() => void) | undefined;
    listen(WINDOW_HIDDEN, away)
      .then((unlisten) => {
        stop = unlisten;
      })
      .catch(() => {
        // No native backend behind the webview.
      });

    return () => {
      document.removeEventListener("visibilitychange", onVisibility);
      stop?.();
    };
  }, [away]);
}
