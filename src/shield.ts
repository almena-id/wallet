import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Keeping a screen out of screenshots, recordings and the app switcher.
 *
 * The recovery phrase, the quiz that checks it, the restore field and the PIN
 * pad are the screens whose contents are the identity, or the way into it. A
 * picture of any of them is a copy of the identity in somebody's photo library,
 * a shared screen or the system's own thumbnail of the wallet — so while one of
 * them is on screen the window is asked to keep its contents to itself, and
 * told it may stop once none is (`screen_shield`, `src-tauri/src/shield.rs`,
 * which says what each platform can and cannot do).
 *
 * **Counted, not switched.** A screen raises the shield while it is shown and
 * lowers it when it goes; two of them can be on screen at once — the PIN pad
 * inside a screen that holds it, one screen leaving as the next arrives — so
 * the shield stays up for as long as any of them holds it. Going from one to
 * the next is a release and a hold inside the same render, and lowering it in
 * between would leave the window unprotected for a moment, so the window is
 * only told to stop once the count is still nothing after that render.
 *
 * A host that cannot do it — a plain browser in `task dev:web`, a platform with
 * no such thing — answers with an error, which is dropped: the screen is shown
 * either way, and there is nothing the person could do about it.
 */

type Send = (on: boolean) => Promise<unknown>;

const native: Send = (on) => invoke("screen_shield", { on });

let send: Send = native;
let holders = 0;
/** What the window was last told, so it is told each change once. */
let told = false;
let pending: ReturnType<typeof setTimeout> | undefined;

function tell(on: boolean) {
  if (told === on) {
    return;
  }
  told = on;
  void send(on).catch(() => undefined);
}

/** Raise the shield until the function it answers with is called. */
export function hold(): () => void {
  holders += 1;
  clearTimeout(pending);
  pending = undefined;
  tell(true);

  let released = false;
  return () => {
    if (released) {
      return;
    }
    released = true;
    holders -= 1;
    if (holders === 0) {
      clearTimeout(pending);
      pending = setTimeout(() => {
        pending = undefined;
        if (holders === 0) {
          tell(false);
        }
      }, 0);
    }
  };
}

/** Keep the screen that calls it out of captures while it is shown. */
export function useShield(): void {
  useEffect(() => hold(), []);
}

/** For the tests: who the window is told through, and a clean start. */
export function reset(through: Send = native): void {
  clearTimeout(pending);
  pending = undefined;
  holders = 0;
  told = false;
  send = through;
}
