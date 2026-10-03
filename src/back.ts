import { useEffect, useRef } from "react";
import { onBackButtonPress } from "@tauri-apps/api/app";
import type { PluginListener } from "@tauri-apps/api/core";

/**
 * Android's back button and gesture, answered by whatever is on top.
 *
 * Every screen with a way back on it — its back arrow, a sheet's Cancel —
 * puts that way back here while it is shown; the system's back takes the
 * last one put. Screens are not a browser history, so nothing is pushed onto
 * one: this is a stack of the answers currently on screen.
 *
 * **Only while there is one.** Tauri hands the back button to the page for as
 * long as somebody listens, and leaves it to Android otherwise; the listener
 * is held only while the stack is not empty, so on a screen with nowhere to go
 * back to the system does what it always does and leaves the app.
 *
 * Elsewhere nothing listens: iOS has no back button, and a computer has its
 * own arrows on screen.
 */

export type Entry = { current: () => void };

const stack: Entry[] = [];
let listening: Promise<PluginListener | null> | null = null;

const android = typeof navigator !== "undefined" && /Android/i.test(navigator.userAgent);

function listen() {
  if (!android) {
    return;
  }
  if (stack.length > 0 && listening === null) {
    listening = onBackButtonPress(() => {
      stack[stack.length - 1]?.current();
    }).catch(() => null);
  } else if (stack.length === 0 && listening !== null) {
    const held = listening;
    listening = null;
    void held.then((listener) => listener?.unregister()).catch(() => undefined);
  }
}

/**
 * Puts `entry` on top of the stack until the returned function takes it off.
 * What `useSystemBack` does while its component is shown.
 */
export function offer(entry: Entry): () => void {
  stack.push(entry);
  listen();
  return () => {
    const at = stack.lastIndexOf(entry);
    if (at >= 0) {
      stack.splice(at, 1);
    }
    listen();
  };
}

/** Offers `onBack` to the system's back while the calling component is shown. */
export function useSystemBack(onBack: (() => void) | undefined) {
  const latest = useRef(onBack);
  latest.current = onBack;
  const offered = onBack !== undefined;

  useEffect(() => {
    if (!offered) {
      return;
    }
    return offer({ current: () => latest.current?.() });
  }, [offered]);
}
