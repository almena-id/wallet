import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";

/** The event the Rust side sends when the window comes back from the tray. */
const WINDOW_SHOWN = "window-shown";

/**
 * The moment the wallet is on the screen again.
 *
 * **Leaving is not locking.** The wallet used to let go the instant it went out
 * of sight, which made switching to another app for a moment — or answering a
 * system prompt, or clicking a window in front — cost a PIN. What decides now is
 * the length somebody chose, counted across the absence like any other stretch
 * of nobody using the wallet: see [`useIdle`].
 *
 * Coming back is still worth knowing about, for one reason: the timer may not
 * have run. A webview the system froze ran nothing at all while the wallet was
 * away, and a hidden one is throttled by the browser. So the return asks the
 * clock the time, and a wallet whose time ran out while it was gone lets go
 * here, at the first moment anybody could have seen it open.
 *
 * Two ways it happens: the system putting the wallet back on the screen, and the
 * window coming back from the tray, which the webview is never told about.
 */
export function useBackInSight(back: () => void) {
  const latest = useRef(back);
  latest.current = back;

  useEffect(() => {
    const onVisibility = () => {
      if (!document.hidden) {
        latest.current();
      }
    };

    document.addEventListener("visibilitychange", onVisibility);
    let stop: (() => void) | undefined;
    listen(WINDOW_SHOWN, () => latest.current())
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
  }, []);
}
