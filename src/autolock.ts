import { useCallback, useEffect, useRef, useState } from "react";

/**
 * How long the wallet stays open with nobody using it.
 *
 * The one length there is, and it covers both ways of not using a wallet: left
 * open on a desk, and left behind for another app. Nothing about a wallet being
 * on the screen means somebody is still there, and nothing about it being off
 * the screen means they are not coming back — so what decides is the same
 * number either way, and it is the number somebody chose here.
 *
 * The choices are minutes rather than a free number because there is no useful
 * answer between them, and because every one of them has to be short enough
 * that walking away is safe. Ten is the longest offered on purpose.
 */
export const autoLockMinutes = [1, 5, 10] as const;

export type AutoLock = (typeof autoLockMinutes)[number];

/** The shortest of them, because the safe end is the one to default to. */
export const defaultAutoLock: AutoLock = 1;

/**
 * Where the choice is kept.
 *
 * A preference about this device, holding nothing about the identity — the same
 * reasoning as the theme and the accent, and the same store.
 */
const storageKey = "almena.autolock";

function isAutoLock(value: unknown): value is AutoLock {
  return (
    typeof value === "number" && (autoLockMinutes as readonly number[]).includes(value)
  );
}

function stored(): AutoLock {
  try {
    const value = Number(window.localStorage.getItem(storageKey));
    return isAutoLock(value) ? value : defaultAutoLock;
  } catch {
    // Private modes and locked down webviews can refuse storage entirely.
    return defaultAutoLock;
  }
}

export function useAutoLock(): {
  autoLock: AutoLock;
  setAutoLock: (minutes: AutoLock) => void;
} {
  const [autoLock, setAutoLockState] = useState<AutoLock>(() => stored());

  const setAutoLock = useCallback((next: AutoLock) => {
    setAutoLockState(next);
    try {
      window.localStorage.setItem(storageKey, String(next));
    } catch {
      // Losing the preference is acceptable; refusing the change is not — and
      // what it falls back to is the shortest, which is the safe direction.
    }
  }, []);

  return { autoLock, setAutoLock };
}

/**
 * Let go when nobody has used the wallet for a while.
 *
 * Counted from the last sign of somebody being here rather than on a fixed
 * schedule, so the clock a person is racing is the one they can see the effect
 * of: doing anything at all starts it again.
 *
 * **Leaving the wallet is not using it, and it is not locking it either.** A
 * wallet that goes off the screen — another app brought to the front, a window
 * put away on the tray, the system's own prompt over it — is a wallet nobody is
 * doing anything with, which is exactly what this counts. So the count carries
 * on across the absence, and somebody who comes back inside the time they chose
 * finds the wallet as they left it. The length is what decides, never the
 * leaving.
 *
 * One timer stands between the last sign of life and the lock, and it rebuilds
 * itself rather than being rebuilt by every event: a single scroll is dozens of
 * them, and tearing the timer down and putting it back each time would be work
 * done to learn nothing. Activity only writes down when it happened; the timer
 * reads that when it fires, and either lets go or waits out what is left.
 *
 * **What it counts must not depend on what the screen is doing.** `away` is held
 * in a ref rather than watched, because the callback handed in is rebuilt on
 * every render — and an effect that watched it would start the minute again
 * each time anything at all re-rendered, which on a busy screen is a wallet
 * that never locks. Only the length and whether it is armed can restart it.
 *
 * The answer is for the moment the wallet is on the screen again: a webview the
 * system froze ran no timer at all while it was away, and a hidden one ran it
 * late. Asking the time rather than trusting the timer settles both — see
 * [`useBackInSight`].
 */
export function useIdle(minutes: AutoLock, armed: boolean, away: () => void): () => void {
  const latest = useRef(away);
  latest.current = away;
  const span = useRef(minutes * 60_000);
  span.current = minutes * 60_000;
  const counting = useRef(armed);
  counting.current = armed;

  // When somebody was last here, and the timer standing between that and the
  // lock. Refs rather than state: nothing on the screen is drawn from either,
  // and a render per pointer move is a wallet that stutters while it is used.
  const seen = useRef(Date.now());
  const timer = useRef<number | undefined>(undefined);

  const arm: () => void = useCallback(() => {
    window.clearTimeout(timer.current);
    if (!counting.current) {
      return;
    }

    const left = seen.current + span.current - Date.now();
    timer.current = window.setTimeout(() => {
      if (!counting.current) {
        return;
      }
      if (Date.now() - seen.current >= span.current) {
        latest.current();
      } else {
        arm();
      }
    }, Math.max(left, 0));
  }, []);

  // The wallet is back where somebody can see it: the time decides, because the
  // timer may not have run.
  const catchUp = useCallback(() => {
    if (!counting.current) {
      return;
    }
    if (Date.now() - seen.current >= span.current) {
      window.clearTimeout(timer.current);
      latest.current();
      return;
    }
    arm();
  }, [arm]);

  useEffect(() => {
    if (!armed) {
      window.clearTimeout(timer.current);
      return;
    }

    seen.current = Date.now();
    arm();

    const stir = () => {
      // Not while the wallet is off the screen. A window sitting behind another
      // one still hears the mouse pass over it, and a pointer crossing a wallet
      // nobody is looking at is not somebody using it — counting that would be a
      // wallet kept open by the cursor resting on top of it.
      if (document.hidden) {
        return;
      }
      seen.current = Date.now();
    };

    // Everything a person does with a wallet, on a phone and on a computer.
    // Captured, so a handler that stops an event on its way down still counts
    // as somebody being here.
    const doings = ["pointerdown", "pointermove", "keydown", "wheel", "touchstart"];
    for (const doing of doings) {
      window.addEventListener(doing, stir, { capture: true, passive: true });
    }

    return () => {
      window.clearTimeout(timer.current);
      for (const doing of doings) {
        window.removeEventListener(doing, stir, { capture: true });
      }
    };
  }, [minutes, armed, arm]);

  return catchUp;
}
