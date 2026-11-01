import type { Dictionary } from "./i18n/locales";
import type { BiometricKind, BiometricPrompt, VaultStatus } from "./vault";

/** What the switch and the lock screen's button are called on this device. */
export type BiometricWords = {
  /** The Security row: "Biometrics", "Fingerprint or face", "Windows Hello". */
  label: string;
  /** The lock screen's button, read out: "Use Windows Hello". */
  use: string;
};

/**
 * The name each system gives its own check. Apple's is the word the wallet
 * always used, since the device may have Face ID or Touch ID and the system's
 * prompt says which; Android's says both, as its settings do; Windows' is the
 * product's name.
 */
export function biometricWords(t: Dictionary, kind: BiometricKind | null): BiometricWords {
  switch (kind) {
    case "android":
      return t.biometrics.android;
    case "windows":
      return t.biometrics.windows;
    default:
      return { label: t.settings.security.biometricsLabel, use: t.pin.useBiometrics };
  }
}

/**
 * The words for the system's prompt. Only Android draws one from them: the
 * negative button leaves the unlock and the confirmation for the PIN, and
 * arming for nothing.
 */
export function biometricPrompt(
  t: Dictionary,
  purpose: "unlock" | "arm" | "confirm",
): BiometricPrompt {
  const words = t.biometrics.prompt;
  switch (purpose) {
    case "unlock":
      return { title: words.unlockTitle, subtitle: words.unlockSubtitle, cancel: words.unlockCancel };
    case "confirm":
      return { title: words.confirmTitle, subtitle: words.confirmSubtitle, cancel: words.confirmCancel };
    default:
      return { title: words.armTitle, subtitle: words.armSubtitle, cancel: words.armCancel };
  }
}

/**
 * Whether the lock screen offers the device: armed, with a copy the system can
 * still hand back. One whose copy is gone offers the PIN alone and says why
 * (`lockNote`).
 */
export function offersBiometrics(status: VaultStatus): boolean {
  return status.pin && status.deviceKey && !status.deviceChanged;
}

/** What the lock screen says before anything is typed, if anything. */
export function lockNote(t: Dictionary, status: VaultStatus): string | null {
  return status.pin && status.deviceChanged ? t.vault.errors.vault_device_changed : null;
}

/** What the Security row says under its label, if anything. */
export function biometricsHint(t: Dictionary, status: VaultStatus): string | undefined {
  // One answer for every way this can be off — no sensor on this Mac, a build
  // the system will not give a protected keychain to, nothing enrolled, Linux.
  // They are the same fact to the person reading the row: this device will not
  // do it. Which of them it is belongs in the log, not here.
  if (!status.deviceUnlock) {
    return t.settings.security.biometricsUnavailable;
  }
  return status.deviceChanged ? t.biometrics.changedHint : undefined;
}
