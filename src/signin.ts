import { invoke } from "@tauri-apps/api/core";

/**
 * Answering a sign-in request.
 *
 * The interface never sees the nonce, the address the answer goes to, or the
 * key it is signed with: it asks to read a request, shows what came back, and
 * says whether somebody accepted. Everything else stays on the Rust side.
 */

/** Who is asking, as the approval screen shows them. */
export type Asker = {
  did: string;
  name: string;
  logoUrl: string | null;
};

export type SignInRequest = {
  verifier: Asker;
  /** When the request stops being answerable, in seconds since the epoch. */
  expiresAt: number;
};

/** Whether a link or a scanned code is a sign-in request at all. */
export function isSignInLink(value: string): boolean {
  return value.startsWith("almena://signin?");
}

/** Read a request and hold it, so it can be answered without passing it back. */
export function readSignIn(link: string): Promise<SignInRequest> {
  return invoke<SignInRequest>("signin_read", { link });
}

/** Accept the request being shown, signing with this verifier's own key. */
export function acceptSignIn(): Promise<void> {
  return invoke<void>("signin_accept");
}

/** Refuse it, so the screen it came from can say so rather than time out. */
export function declineSignIn(): Promise<void> {
  return invoke<void>("signin_decline");
}

/** Let go of a request nobody answered. */
export function forgetSignIn(): Promise<void> {
  return invoke<void>("signin_forget").catch(() => undefined);
}

/**
 * The error codes the sign-in commands answer with. They are codes and not
 * sentences, so this side says them in the language somebody is reading.
 */
export type SignInErrorCode =
  | "signin_unreadable"
  | "signin_not_ours"
  | "signin_unverified"
  | "signin_expired"
  | "signin_unreachable"
  | "signin_no_identity"
  | "signin_nothing"
  | "signin_refused"
  | "signin_unknown";

const CODES: SignInErrorCode[] = [
  "signin_unreadable",
  "signin_not_ours",
  "signin_unverified",
  "signin_expired",
  "signin_unreachable",
  "signin_no_identity",
  "signin_nothing",
  "signin_refused",
];

/** Whatever a rejected command threw, as a code this interface has a word for. */
export function errorCode(error: unknown): SignInErrorCode {
  return typeof error === "string" && (CODES as string[]).includes(error)
    ? (error as SignInErrorCode)
    : "signin_unknown";
}
