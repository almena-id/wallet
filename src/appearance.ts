import { useCallback, useEffect, useState } from "react";

/**
 * The colour the wallet wears.
 *
 * The palettes themselves live in the stylesheet, keyed by `data-accent` on the
 * root element: this side only says which one, so a colour is a set of tokens
 * and never a value hardcoded in a component.
 */
export const accents = ["orange", "blue", "red", "yellow", "green"] as const;

export type Accent = (typeof accents)[number];

/** The one the wallet wears when nobody has said otherwise. */
export const defaultAccent: Accent = "orange";

/**
 * Where the choice is kept.
 *
 * Unlike the language — which the operating system already answers — there is
 * nothing else to ask about a colour, so it is remembered here. It is a
 * preference about this device and holds nothing about the identity.
 */
const storageKey = "almena.accent";

function isAccent(value: unknown): value is Accent {
  return typeof value === "string" && (accents as readonly string[]).includes(value);
}

function stored(): Accent {
  try {
    const value = window.localStorage.getItem(storageKey);
    return isAccent(value) ? value : defaultAccent;
  } catch {
    // Private modes and locked down webviews can refuse storage entirely.
    return defaultAccent;
  }
}

export function useAccent(): { accent: Accent; setAccent: (accent: Accent) => void } {
  const [accent, setAccentState] = useState<Accent>(() => stored());

  useEffect(() => {
    document.documentElement.dataset.accent = accent;
  }, [accent]);

  const setAccent = useCallback((next: Accent) => {
    setAccentState(next);
    try {
      window.localStorage.setItem(storageKey, next);
    } catch {
      // Losing the preference is acceptable; refusing the change is not.
    }
  }, []);

  return { accent, setAccent };
}
