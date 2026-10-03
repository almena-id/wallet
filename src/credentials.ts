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
