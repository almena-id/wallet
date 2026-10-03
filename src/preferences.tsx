import { createContext, useContext, useMemo, type ReactNode } from "react";

import { useAccent, type Accent } from "./appearance";
import { useAutoLock, type AutoLock } from "./autolock";
import { useNotificationPrivacy, type Privacy } from "./notify";
import { useTheme, type Theme } from "./theme";

/**
 * What this device keeps about how the wallet looks and behaves — not the
 * identity's: the identity colour, light or dark, the auto-lock, and how much a
 * notification says. Applied once, at the root (each hook puts its token on
 * the document or hands it to Rust), and read wherever it is shown or chosen.
 */
export type Preferences = {
  accent: Accent;
  setAccent: (accent: Accent) => void;
  theme: Theme;
  setTheme: (theme: Theme) => void;
  autoLock: AutoLock;
  setAutoLock: (minutes: AutoLock) => void;
  privacy: Privacy;
  setPrivacy: (privacy: Privacy) => void;
};

const PreferencesContext = createContext<Preferences | null>(null);

/** The preferences, applied; for the root, which also needs some of them. */
export function useDevicePreferences(): Preferences {
  const { accent, setAccent } = useAccent();
  const { theme, setTheme } = useTheme();
  const { autoLock, setAutoLock } = useAutoLock();
  const { privacy, setPrivacy } = useNotificationPrivacy();
  return useMemo(
    () => ({ accent, setAccent, theme, setTheme, autoLock, setAutoLock, privacy, setPrivacy }),
    [accent, setAccent, theme, setTheme, autoLock, setAutoLock, privacy, setPrivacy],
  );
}

export function PreferencesProvider({ value, children }: { value: Preferences; children: ReactNode }) {
  return <PreferencesContext.Provider value={value}>{children}</PreferencesContext.Provider>;
}

/** The preferences, from anywhere under the root. */
export function usePreferences(): Preferences {
  const preferences = useContext(PreferencesContext);
  if (preferences === null) {
    throw new Error("usePreferences outside PreferencesProvider");
  }
  return preferences;
}
