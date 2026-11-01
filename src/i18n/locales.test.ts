import { afterEach, describe, expect, it, vi } from "vitest";

import {
  defaultLocale,
  isLanguage,
  locales,
  resolveLocale,
  storeLanguage,
  storedLanguage,
  systemLanguage,
} from "./locales";

describe("the language", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("carries English first and Spanish", () => {
    expect(locales[0]).toBe(defaultLocale);
    expect(locales).toContain("es");
  });

  it("follows the system when nothing was chosen", () => {
    expect(resolveLocale(systemLanguage, ["es-ES", "en-US"])).toBe("es");
    expect(resolveLocale(systemLanguage, ["en-GB", "es"])).toBe("en");
  });

  it("reads a regional tag in its language, and falls back to English", () => {
    expect(resolveLocale(systemLanguage, ["es-419"])).toBe("es");
    expect(resolveLocale(systemLanguage, ["fr-FR", "de"])).toBe(defaultLocale);
    expect(resolveLocale(systemLanguage, [])).toBe(defaultLocale);
  });

  it("is the one chosen, whatever the system says", () => {
    expect(resolveLocale("es", ["en-US"])).toBe("es");
    expect(resolveLocale("en", ["es-ES"])).toBe("en");
  });

  it("ignores a choice the wallet no longer carries", () => {
    expect(isLanguage("fr")).toBe(false);
    expect(resolveLocale("fr", ["es-ES"])).toBe("es");
  });

  it("is kept on this device and read back", () => {
    const kept = new Map<string, string>();
    vi.stubGlobal("window", {
      localStorage: {
        getItem: (key: string) => kept.get(key) ?? null,
        setItem: (key: string, value: string) => kept.set(key, value),
      },
    });

    expect(storedLanguage()).toBe(systemLanguage);
    storeLanguage("es");
    expect(storedLanguage()).toBe("es");
    kept.set("almena.language", "klingon");
    expect(storedLanguage()).toBe(systemLanguage);
  });

  it("is the system's when storage is refused", () => {
    vi.stubGlobal("window", {
      localStorage: {
        getItem: () => {
          throw new Error("refused");
        },
        setItem: () => {
          throw new Error("refused");
        },
      },
    });

    expect(() => storeLanguage("es")).not.toThrow();
    expect(storedLanguage()).toBe(systemLanguage);
  });
});
