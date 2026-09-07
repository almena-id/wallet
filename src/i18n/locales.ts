/**
 * Locale plumbing for the wallet.
 *
 * Catalogues are picked up from `./messages/*.json`, so adding a language is
 * adding a file: nothing here needs to change. English is the fallback, and it
 * is also the shape every other catalogue is checked against — a key missing
 * from a translation resolves to English rather than to a blank or a raw key.
 *
 * **The language is the system's.** There is no switch in the wallet: somebody
 * who reads in Spanish has already said so, once, to their phone or their
 * computer, and an application that asks again is an application that can be
 * left saying something they did not choose.
 */

import en from "./messages/en.json";

export type Dictionary = typeof en;

/** Fallback locale. A missing translation always resolves to this one. */
export const defaultLocale = "en";

const catalogueModules = import.meta.glob<Record<string, unknown>>("./messages/*.json", {
  eager: true,
  import: "default",
});

/** Deep merge of a partial catalogue over the English one. */
function withFallback<T>(base: T, override: unknown): T {
  if (override === null || typeof override !== "object" || Array.isArray(override)) {
    return base;
  }
  const source = override as Record<string, unknown>;
  const merged: Record<string, unknown> = { ...(base as Record<string, unknown>) };
  for (const [key, value] of Object.entries(base as Record<string, unknown>)) {
    if (!(key in source)) {
      continue;
    }
    merged[key] =
      value !== null && typeof value === "object" && !Array.isArray(value)
        ? withFallback(value, source[key])
        : (source[key] ?? value);
  }

  // Keys the translation has and English does not are kept rather than dropped.
  // Plural categories are why: a language with `few` and `many` has to be able to
  // supply them, and English — which has only `one` and `other` — cannot declare
  // a shape it has no words for. Adding a language stays a matter of adding a
  // file, which is the whole point of this arrangement.
  for (const [key, value] of Object.entries(source)) {
    if (!(key in merged)) {
      merged[key] = value;
    }
  }

  return merged as T;
}

function localeOf(path: string): string {
  return path.replace(/^.*\/(.+)\.json$/, "$1");
}

export const catalogues: Record<string, Dictionary> = Object.fromEntries(
  Object.entries(catalogueModules).map(([path, catalogue]) => [
    localeOf(path),
    withFallback(en, catalogue),
  ]),
);

/** Available locales, with the fallback locale first. */
export const locales: string[] = Object.keys(catalogues).sort((a, b) => {
  if (a === defaultLocale) return -1;
  if (b === defaultLocale) return 1;
  return a.localeCompare(b);
});

export function isLocale(value: string | null | undefined): boolean {
  return typeof value === "string" && locales.includes(value);
}

/** The locale to read in: whatever the system asks for, or the fallback. */
export function resolveLocale(): string {
  const preferred = typeof navigator === "undefined" ? [] : [...(navigator.languages ?? [])];
  if (typeof navigator !== "undefined" && navigator.language) {
    preferred.push(navigator.language);
  }

  for (const tag of preferred) {
    // BCP 47: the full tag first, then its language subtag — `es-419` is read
    // in Spanish by a wallet that only carries `es`.
    if (isLocale(tag)) {
      return tag;
    }
    const language = tag.split("-")[0]?.toLowerCase();
    if (isLocale(language)) {
      return language;
    }
  }

  return defaultLocale;
}
