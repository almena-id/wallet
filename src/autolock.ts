import { useCallback, useEffect, useRef, useState } from "react";

/**
 * How long the wallet stays open with nobody using it.
 *
 * The wallet already lets go the moment it leaves the screen — see [`lock`].
 * This is the other half: a wallet left open on a desk is on screen the whole
 * time, and nothing about it being visible means somebody is still there.
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
 * Let go when nobody has touched the wallet for a while.
 *
 * Counted from the last thing somebody did rather than on a fixed schedule, so
 * the clock a person is racing is the one they can see the effect of: doing
 * anything at all starts it again.
 *
 * The timer is rebuilt on activity rather than checked on a tick, so a wallet
 * genuinely sitting idle is not waking anything up once a second to ask.
 * Rebuilding it is throttled because a single scroll is dozens of events and
 * each one would otherwise tear the timer down and put it back.
 *
 * **What it counts must not depend on what the screen is doing.** `away` is held
 * in a ref rather than watched, because the callback handed in is rebuilt on
 * every render — and an effect that watched it would start the minute again
 * each time anything at all re-rendered, which on a busy screen is a wallet
 * that never locks. Only the length and whether it is armed can restart it.
 */
export function useIdle(minutes: AutoLock, armed: boolean, away: () => void): void {
  const latest = useRef(away);
  latest.current = away;

  useEffect(() => {
    if (!armed) {
      return;
    }

    const leave = () => latest.current();

    const after = minutes * 60_000;
    let timer = window.setTimeout(leave, after);
    let rebuilt = Date.now();

    const stir = () => {
      const now = Date.now();
      if (now - rebuilt < 1000) {
        return;
      }
      rebuilt = now;
      window.clearTimeout(timer);
      timer = window.setTimeout(leave, after);
    };

    // Everything a person does with a wallet, on a phone and on a computer.
    // Captured, so a handler that stops an event on its way down still counts
    // as somebody being here.
    const doings = ["pointerdown", "pointermove", "keydown", "wheel", "touchstart"];
    for (const doing of doings) {
      window.addEventListener(doing, stir, { capture: true, passive: true });
    }

    return () => {
      window.clearTimeout(timer);
      for (const doing of doings) {
        window.removeEventListener(doing, stir, { capture: true });
      }
    };
  }, [minutes, armed]);
}
