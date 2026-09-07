import { invoke } from "@tauri-apps/api/core";

/** How many words a phrase has. The Rust side owns this number; it is repeated
 *  here only so the interface can lay out a grid without asking. */
export const PHRASE_WORDS = 12;

export type Identity = {
  /** The identifier itself, `did:key:z…`. */
  did: string;
  /** The public key as multibase text, which the identifier is built from. */
  publicKey: string;
  /** The DID document that describes the identity. */
  document: Record<string, unknown>;
};

/** A new phrase to show. The backend holds it until it is confirmed. */
export function draftPhrase(locale: string): Promise<string[]> {
  return invoke<string[]>("identity_draft", { locale });
}

/** The identity the phrase being shown produces. Ends that phrase's stay in memory. */
export function createIdentity(): Promise<Identity> {
  return invoke<Identity>("identity_create");
}

/** The identity a phrase somebody already has produces. */
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
