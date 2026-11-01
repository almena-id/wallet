import { invoke } from "@tauri-apps/api/core";

/**
 * A credential this wallet holds: an SD-JWT VC an issuer of an Almena Registry
 * granted it, kept sealed on the device (see `credentials.rs`).
 */
export type HeldCredential = {
  id: string;
  format: string;
  issuerDid: string;
  issuerName: string;
  /** Its type in Almena's catalogue, and its names by language. */
  typeId: string;
  typeLabels: Record<string, string>;
  /** What it says, by claim. */
  claims: Record<string, unknown>;
  /** Seconds since the epoch. */
  issuedAt: number;
  validUntil: number;
  receivedAt: number;
  /** What its key is derived from: the registry's origin, a space, the issuer's DID. */
  scope: string;
  /** Its status, as its issuer's status list last said it (`status.rs`). */
  checked: {
    status: "valid" | "suspended" | "revoked" | null;
    /** Seconds since the epoch: when the status was read. */
    at: number | null;
    /** Why the last check gave no status, or kept an older one. */
    problem: "noList" | "unreachable" | "untrusted" | "signature" | null;
  };
};

/** The credentials held, newest first. Needs the wallet open. */
export function listCredentials(): Promise<HeldCredential[]> {
  return invoke<HeldCredential[]>("credentials_list");
}

/**
 * Checks each credential with its issuer's status list, keeps what was
 * learnt, and returns them as `listCredentials` does.
 */
export function checkCredentials(): Promise<HeldCredential[]> {
  return invoke<HeldCredential[]>("credentials_check");
}

/**
 * Removes a credential from this wallet for good — only its issuer can grant
 * it again — and returns the rest, as `listCredentials` does.
 */
export function removeCredential(id: string): Promise<HeldCredential[]> {
  return invoke<HeldCredential[]>("credentials_remove", { id });
}

/**
 * Its name in `locale`: the exact tag, then its language alone (`es` for
 * `es-ES`), then English, then any name it has, and its catalogue id when it
 * has none.
 */
export function credentialName(credential: HeldCredential, locale: string): string {
  const labels = credential.typeLabels;
  const language = locale.split("-")[0];
  return (
    labels[locale] ??
    labels[language] ??
    labels.en ??
    Object.values(labels).find((label) => label.trim() !== "") ??
    credential.typeId
  );
}

/** Who issued it: the name it came with, or its DID when that is blank. */
export function issuerLabel(credential: HeldCredential): string {
  return credential.issuerName.trim() || credential.issuerDid;
}

/**
 * The host of the registry it came through, read from its `scope` (the
 * registry's origin, a space, the issuer's DID), or null.
 */
export function registryOf(credential: HeldCredential): string | null {
  const origin = credential.scope.split(" ")[0];
  try {
    return new URL(origin).host || null;
  } catch {
    return null;
  }
}

/**
 * The one word a credential's card wears for how it stands. What the issuer
 * said first — revoked, suspended —, then a signature its issuer no longer
 * lists, then its own end date; valid only when the issuer's list said so,
 * and "not checked" while nothing is known.
 */
export type Standing = "valid" | "suspended" | "revoked" | "expired" | "invalid" | "unchecked";

export function standing(credential: HeldCredential, now: number = Date.now()): Standing {
  const { status, problem } = credential.checked;
  if (status === "revoked" || status === "suspended") return status;
  if (problem === "signature") return "invalid";
  if (credential.validUntil * 1000 < now) return "expired";
  return status === "valid" ? "valid" : "unchecked";
}

/** How long a status read from a list is taken as current when Home opens. */
export const RECHECK_AFTER_MS = 10 * 60 * 1000;

/**
 * Whether opening Home should check the statuses again: some credential was
 * never read from its list, or not for `RECHECK_AFTER_MS`. The sync button
 * checks whatever this says.
 */
export function dueForCheck(held: readonly HeldCredential[], now: number = Date.now()): boolean {
  return held.some(({ checked }) => checked.at === null || checked.at * 1000 + RECHECK_AFTER_MS <= now);
}

/** A claim's value as a person reads it: lists and groups joined. */
export function shown(value: unknown): string {
  if (Array.isArray(value)) return value.map(shown).join(", ");
  if (value && typeof value === "object") return Object.values(value).map(shown).join(", ");
  return String(value ?? "");
}
