import { offersBiometrics } from "./biometrics";
import type { Dictionary } from "./i18n/locales";
import type { PhraseVerdict } from "./identity";
import type { VaultStatus } from "./vault";

/**
 * How the holder is confirmed before their phrase is checked: the PIN where
 * the wallet has one (biometrics beside it when they open the wallet too), the
 * device's own check where it has none — an iPhone's Face ID and passcode.
 */
export type PhraseConfirmation = { way: "pin"; biometrics: boolean } | { way: "device" };

export function confirmationFor(status: VaultStatus): PhraseConfirmation {
  return status.pin ? { way: "pin", biometrics: offersBiometrics(status) } : { way: "device" };
}

/**
 * What the screen says about a verdict, and whether it is a warning. A
 * refusal for want of a confirmation is not worded here: the screen asks for
 * the PIN again instead.
 */
export function verdictNote(
  t: Dictionary,
  verdict: PhraseVerdict,
): { text: string; warning: boolean } {
  switch (verdict) {
    case "match":
      return { text: t.phraseCheck.match, warning: false };
    case "mismatch":
      return { text: t.phraseCheck.mismatch, warning: true };
    case "identity_word_count":
    case "identity_checksum":
    case "identity_locked":
    case "identity_unconfirmed":
      return { text: t.onboarding.errors[verdict], warning: true };
    default:
      return { text: t.phraseCheck.errors.unknown, warning: true };
  }
}
