import { createContext, useContext, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";

import { catalogues, defaultLocale, resolveLocale, type Dictionary } from "./locales";

type I18nValue = {
  /** The active locale, a BCP 47 tag, as the system asked for it. */
  locale: string;
  /** The active catalogue, always complete: missing keys fall back to English. */
  t: Dictionary;
};

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale] = useState<string>(() => resolveLocale());

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  const value = useMemo<I18nValue>(
    () => ({ locale, t: catalogues[locale] ?? catalogues[defaultLocale] }),
    [locale],
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

/**
 * Fills the `{name}` placeholders of a catalogue string.
 *
 * Catalogues hold whole sentences, placeholders included, because word order is
 * not the same in every language — "Word 3" and "Palabra 3" happen to agree,
 * but nothing guarantees the next language will.
 */
export function fill(template: string, values: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in values ? String(values[name]) : match,
  );
}

/**
 * Picks the wording a count calls for, and writes the count into it.
 *
 * **Which forms exist is the language's business, not this file's.** English has
 * two and Spanish has two, but Polish has four and Arabic six, and a catalogue
 * that carries them is one this reads without changing — `Intl.PluralRules` names
 * the category and the catalogue answers to that name. `other` is the fallback,
 * because every language has one.
 *
 * The number goes through `Intl` on the way in: a thousand is written with a
 * comma in one language and a full stop in another.
 */
export function plural(
  forms: Record<string, string>,
  count: number,
  locale: string,
): string {
  const category = new Intl.PluralRules(locale).select(count);
  const wording = forms[category] ?? forms.other ?? "";

  return fill(wording, { count: new Intl.NumberFormat(locale).format(count) });
}

/** Shorthand for the common case of only needing the catalogue. */
export function useTranslations(): Dictionary {
  return useI18n().t;
}
