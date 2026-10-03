import { useEffect, useSyncExternalStore } from "react";
import { addPluginListener, invoke } from "@tauri-apps/api/core";

/**
 * A call that rang while the wallet was closed (`src-tauri/plugins/call`).
 *
 * The phone rang it — full screen on Android, CallKit on iOS — without knowing
 * who was calling, because a closed wallet cannot read that. What the person
 * did there reaches this side: they opened the wallet from it (`open`) or
 * answered (`answer`). Once the wallet is unlocked and picks the offer up, an
 * answered call is answered here (`call.ts`), and an opened one rings as any
 * other. Either way the native ringing is settled then, or when the offer
 * would have expired, whichever comes first.
 */

export type RungAction = "open" | "answer";

/** As long as an offer lives (`OFFER_TTL` in `messaging/call.rs`). */
const FRESH_MS = 60_000;

let rung: { action: RungAction; until: number } | null = null;
let expiry: ReturnType<typeof setTimeout> | null = null;
const listeners = new Set<() => void>();

function set(next: typeof rung) {
  rung = next;
  listeners.forEach((listener) => listener());
}

function arrived(action: unknown, at = Date.now()) {
  if (action !== "open" && action !== "answer") {
    return;
  }
  const until = at + FRESH_MS;
  if (until <= Date.now()) {
    return;
  }
  set({ action, until });
  if (expiry !== null) {
    clearTimeout(expiry);
  }
  expiry = setTimeout(settleRung, until - Date.now());
}

/**
 * Takes what the person did with the call, if it is still fresh, and leaves
 * nothing behind: it is acted on once.
 */
export function takeRung(): RungAction | null {
  const taken = rung !== null && rung.until > Date.now() ? rung.action : null;
  if (rung !== null) {
    set(null);
  }
  return taken;
}

/**
 * The wallet has dealt with it — the offer is ringing here, or it was
 * answered, or it has expired: the phone stops ringing, CallKit's call ends,
 * and on Android the wallet no longer shows over the lock screen.
 */
export function settleRung() {
  if (expiry !== null) {
    clearTimeout(expiry);
    expiry = null;
  }
  set(null);
  void invoke("plugin:almena-call|settle").catch(() => undefined);
}

/**
 * Listens for calls the phone rang, from the start: also one that opened the
 * wallet, which the plugin kept until it was asked. Only a phone has them; on
 * a computer the plugin is not there and this does nothing.
 */
export function useRungCalls() {
  useEffect(() => {
    let active = true;
    void invoke<{ action?: string; at?: number }>("plugin:almena-call|take")
      .then((taken) => active && arrived(taken.action, taken.at))
      .catch(() => undefined);
    const listener = addPluginListener<{ action: string }>("almena-call", "call", (event) =>
      arrived(event.action),
    ).catch(() => null);
    return () => {
      active = false;
      void listener.then((registered) => registered?.unregister()).catch(() => undefined);
    };
  }, []);
}

/** What the person did with a call the phone rang, while it is fresh. */
export function useRung(): RungAction | null {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => rung?.action ?? null,
  );
}
