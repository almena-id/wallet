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
};

/** The credentials held, newest first. Needs the wallet open. */
export function listCredentials(): Promise<HeldCredential[]> {
  return invoke<HeldCredential[]>("credentials_list");
}
