import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";

import {
  catalogues,
  defaultLocale,
  resolveLocale,
  storeLanguage,
  storedLanguage,
  type Dictionary,
  type Language,
} from "./locales";

type I18nValue = {
  /** The active locale, a BCP 47 tag: the one chosen, or the system's. */
  locale: string;
  /** The active catalogue, always complete: missing keys fall back to English. */
  t: Dictionary;
  /** What the person chose in Profile → Appearance; `system` by default. */
  language: Language;
  /** Changes the language at once and keeps the choice on this device. */
  setLanguage: (language: Language) => void;
};

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState<Language>(() => storedLanguage());
  const locale = useMemo(() => resolveLocale(language), [language]);

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  const setLanguage = useCallback((next: Language) => {
    setLanguageState(next);
    storeLanguage(next);
  }, []);

  const value = useMemo<I18nValue>(
    () => ({
      locale,
      t: catalogues[locale] ?? catalogues[defaultLocale],
      language,
      setLanguage,
    }),
    [locale, language, setLanguage],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const value = useContext(I18nContext);
  if (!value) {
    throw new Error("useI18n was called outside of I18nProvider");
  }
  return value;
}

/** Shorthand for the common case of only needing the catalogue. */
export function useTranslations(): Dictionary {
  return useI18n().t;
}
