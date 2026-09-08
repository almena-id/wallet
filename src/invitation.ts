import { invoke } from "@tauri-apps/api/core";

/**
 * Answering an invitation to an organization.
 *
 * The interface never sees the nonce, the address the answer goes to, or either
 * of the two keys it is signed with: it asks to read an invitation, shows what
 * came back, and says whether somebody accepted. Everything else stays on the
 * Rust side.
 *
 * There is no decline. A sign-in leaves a screen counting down and telling it
 * "no" is a kindness; an invitation leaves a row that expires by itself, and an
 * answer refusing it would be a fact about somebody that nobody asked to record.
 */

/** The organization doing the inviting, as the approval screen shows it. */
export type Inviter = {
  did: string;
  name: string;
  logoUrl: string | null;
};

/** The place being offered. */
export type Offered = {
  /** `owner` or `member`, said in the reader's language by the interface. */
  role: string;
  /** What whoever invited them wrote this place down as, if anything. */
  name: string | null;
};

export type Invitation = {
  entity: Inviter;
  offered: Offered;
  /** When the invitation stops being answerable, in seconds since the epoch. */
  expiresAt: number;
};

/** Whether a link or a scanned code is an invitation at all. */
export function isInvitationLink(value: string): boolean {
  return value.startsWith("almena://invitation?");
}

/** Read an invitation and hold it, so it can be answered without passing it back. */
export function readInvitation(link: string): Promise<Invitation> {
  return invoke<Invitation>("invitation_read", { link });
}

/**
 * Accept the invitation being shown.
 *
 * Two identifiers are derived and proved in one answer: one for the
 * organization, which is what it will deal with from here, and one for the
 * console, which is what an account on the platform is. Nothing but this device
 * knows they belong together.
 */
export function acceptInvitation(): Promise<void> {
  return invoke<void>("invitation_accept");
}

/** Let go of an invitation nobody answered. */
export function forgetInvitation(): Promise<void> {
  return invoke<void>("invitation_forget").catch(() => undefined);
}

/**
 * The error codes the invitation commands answer with. They are codes and not
 * sentences, so this side says them in the language somebody is reading.
 */
export type InvitationErrorCode =
  | "invitation_unreadable"
  | "invitation_not_ours"
  | "invitation_unverified"
  | "invitation_expired"
  | "invitation_unreachable"
  | "invitation_no_identity"
  | "invitation_nothing"
  | "invitation_already_joined"
  | "invitation_refused"
  | "invitation_unknown";

const CODES: InvitationErrorCode[] = [
  "invitation_unreadable",
  "invitation_not_ours",
  "invitation_unverified",
  "invitation_expired",
  "invitation_unreachable",
  "invitation_no_identity",
  "invitation_nothing",
  "invitation_already_joined",
  "invitation_refused",
];

/** Whatever a rejected command threw, as a code this interface has a word for. */
export function errorCode(error: unknown): InvitationErrorCode {
  return typeof error === "string" && (CODES as string[]).includes(error)
    ? (error as InvitationErrorCode)
    : "invitation_unknown";
}
