import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import en from "./i18n/messages/en.json";
import es from "./i18n/messages/es.json";
import type { Dictionary } from "./i18n/locales";

const { biometricPrompt, biometricWords, biometricsHint, lockNote, offersBiometrics } =
  await import("./biometrics");
const { emptyVault, errorCode } = await import("./vault");

const armed = {
  ...emptyVault,
  exists: true,
  pin: true,
  digits: 6,
  deviceKey: true,
  deviceUnlock: true,
};

describe("biometrics", () => {
  it("are named as each system names its own check", () => {
    expect(biometricWords(en, "android").label).toBe("Fingerprint or face");
    expect(biometricWords(en, "windows").use).toBe("Use Windows Hello");
    expect(biometricWords(en, "apple").label).toBe(en.settings.security.biometricsLabel);
    expect(biometricWords(en, null).use).toBe(en.pin.useBiometrics);
    expect(biometricWords(es as Dictionary, "android").label).toBe("Huella o cara");
  });

  it("word the prompt for the PIN when unlocking and for nothing when arming", () => {
    expect(biometricPrompt(en, "unlock").cancel).toBe("Use PIN");
    expect(biometricPrompt(en, "arm").cancel).toBe("Cancel");
    for (const purpose of ["unlock", "arm"] as const) {
      const words = biometricPrompt(en, purpose);
      expect(words.title).not.toBe("");
      expect(words.cancel).not.toBe("");
    }
  });

  it("are offered on the lock only while the copy can still come back", () => {
    expect(offersBiometrics(armed)).toBe(true);
    expect(offersBiometrics({ ...armed, deviceKey: false })).toBe(false);
    expect(offersBiometrics({ ...armed, deviceKey: false, deviceChanged: true })).toBe(false);
    // The iPhone's own lock has its own screen.
    expect(offersBiometrics({ ...armed, pin: false })).toBe(false);
  });

  it("say on the lock that they changed, and the PIN is the way in", () => {
    expect(lockNote(en, armed)).toBeNull();
    expect(lockNote(en, { ...armed, deviceKey: false, deviceChanged: true })).toBe(
      en.vault.errors.vault_device_changed,
    );
  });

  it("explain a switch that is off for a reason", () => {
    expect(biometricsHint(en, armed)).toBeUndefined();
    expect(biometricsHint(en, { ...armed, deviceUnlock: false })).toBe(
      en.settings.security.biometricsUnavailable,
    );
    expect(biometricsHint(en, { ...armed, deviceKey: false, deviceChanged: true })).toBe(
      en.biometrics.changedHint,
    );
  });

  it("have a word for a copy that changed", () => {
    expect(errorCode("vault_device_changed")).toBe("vault_device_changed");
  });
});
