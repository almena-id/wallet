import { useCallback, useEffect, useState } from "react";

/**
 * Light, dark, or whatever the system is doing.
 *
 * `system` is the default and is not a third palette: it is the absence of a
 * choice, which leaves `prefers-color-scheme` to answer. The other two say so on
 * the root element, and the stylesheet reads it from there.
 */
export const themes = ["system", "dark", "light"] as const;

export type Theme = (typeof themes)[number];

export const defaultTheme: Theme = "system";

const storageKey = "almena.theme";

function isTheme(value: unknown): value is Theme {
  return typeof value === "string" && (themes as readonly string[]).includes(value);
}

function stored(): Theme {
  try {
    const value = window.localStorage.getItem(storageKey);
    return isTheme(value) ? value : defaultTheme;
  } catch {
    // Private modes and locked down webviews can refuse storage entirely.
    return defaultTheme;
  }
}

export function useTheme(): { theme: Theme; setTheme: (theme: Theme) => void } {
  const [theme, setThemeState] = useState<Theme>(() => stored());

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  const setTheme = useCallback((next: Theme) => {
    setThemeState(next);
    try {
      window.localStorage.setItem(storageKey, next);
    } catch {
      // Losing the preference is acceptable; refusing the change is not.
    }
  }, []);

  return { theme, setTheme };
}
