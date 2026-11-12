import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Live delivery: a WebSocket to the mediator, kept open while the wallet is
 * in front so messages arrive as they are sent — see
 * `src-tauri/src/messaging/live.rs`. What arrives is announced with the
 * `messaging-changed` event (`onMessagesChanged` in `contacts.ts`).
 *
 * **Only while it is seen.** A socket held open in the background is battery
 * and data spent on nobody, and the system closes it anyway; the mediator's
 * transports assume exactly this (its `docs/didcomm.md` §5). Coming back to
 * the front starts a new one, since the old may have died unannounced.
 */
export function useLive(open: boolean) {
  useEffect(() => {
    if (!open) {
      return;
    }
    const follow = () => {
      if (document.visibilityState === "visible") {
        void invoke("live_start").catch(() => undefined);
      } else {
        void invoke("live_stop").catch(() => undefined);
      }
    };
    follow();
    document.addEventListener("visibilitychange", follow);
    return () => {
      document.removeEventListener("visibilitychange", follow);
      void invoke("live_stop").catch(() => undefined);
    };
  }, [open]);
}
