import { invoke } from "@tauri-apps/api/core";

/**
 * The lengths a new phrase can be, in the order they are offered.
 *
 * The Rust side owns this list — see `identity::phrase::Length` for why it is
 * these two and not the six BIP-39 defines — and it is repeated here so the
 * interface can offer the choice without asking first. The first one is what a
 * phrase is unless somebody says otherwise.
 *
 * **Only creating asks.** Bringing an identity back counts the words that were
 * written rather than making somebody declare which kind they have.
 */
export const PHRASE_LENGTHS = [12, 24] as const;

/** How long a phrase is, as one of the lengths this wallet makes. */
export type PhraseLength = (typeof PHRASE_LENGTHS)[number];

/** The length a new phrase is unless somebody chooses the other one. */
export const DEFAULT_PHRASE_LENGTH: PhraseLength = PHRASE_LENGTHS[0];

export type Identity = {
  /** The identifier itself, `did:key:z…`. */
  did: string;
  /** The public key as multibase text, which the identifier is built from. */
  publicKey: string;
  /** The DID document that describes the identity. */
  document: Record<string, unknown>;
};

/**
 * A new phrase to show, of the length asked for. The backend holds it until it
 * is confirmed.
 *
 * Asking again replaces what was being held, so switching length is not two
 * phrases in flight: the one that was on screen is gone the moment this returns.
 */
export function draftPhrase(locale: string, words: PhraseLength): Promise<string[]> {
  return invoke<string[]>("identity_draft", { locale, words });
}

/** The identity the phrase being shown produces. Ends that phrase's stay in memory. */
export function createIdentity(): Promise<Identity> {
  return invoke<Identity>("identity_create");
}

/**
 * The identity a phrase somebody already has produces.
 *
 * Of either length, and in any language BIP-39 defines: the words say which
 * they are, so nothing has to be declared alongside them.
 */
export function restoreIdentity(input: string): Promise<Identity> {
  return invoke<Identity>("identity_restore", { input });
}

/** Forgets a phrase that was being shown, for somebody who left the flow. */
export function discardDraft(): Promise<void> {
  return invoke<void>("identity_discard").catch(() => undefined);
}

/**
 * Lets go of the identity that was open.
 *
 * Signing out is this: the seed every key is derived from is dropped, so
 * nothing can be signed until somebody writes their words again.
 */
export function forgetIdentity(): Promise<void> {
  return invoke<void>("identity_forget").catch(() => undefined);
}

/**
 * The error codes the identity commands answer with. They are codes and not
 * sentences, so this side says them in the language somebody is reading.
 */
export type IdentityErrorCode =
  | "identity_entropy_unavailable"
  | "identity_word_count"
  | "identity_checksum"
  | "identity_no_draft"
  | "identity_unknown";

const CODES: IdentityErrorCode[] = [
  "identity_entropy_unavailable",
  "identity_word_count",
  "identity_checksum",
  "identity_no_draft",
];

/** Whatever a rejected command threw, as a code this interface has a word for. */
export function errorCode(error: unknown): IdentityErrorCode {
  return typeof error === "string" && (CODES as string[]).includes(error)
    ? (error as IdentityErrorCode)
    : "identity_unknown";
}
