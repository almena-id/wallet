import { invoke } from "@tauri-apps/api/core";

/**
 * The encrypted backup (`messaging/backup.rs`): contacts, conversations,
 * credentials, the name and the picture, sealed under a key the phrase
 * derives, in a file the person puts where they choose.
 */

/** What restoring a backup added to what the device holds. */
export type Restored = {
  contacts: number;
  messages: number;
  credentials: number;
};

export type BackupErrorCode =
  | "backup_locked"
  | "backup_file"
  | "backup_not_this_identity"
  | "backup_storage";

/** Writes a backup where the person chooses; `false` if they cancelled. */
export function exportBackup(): Promise<boolean> {
  return invoke<boolean>("backup_export");
}

/** Adds a backup file to what the device holds; `null` if they cancelled. */
export function importBackup(): Promise<Restored | null> {
  return invoke<Restored | null>("backup_import");
}

export function backupErrorCode(error: unknown): BackupErrorCode {
  const codes: BackupErrorCode[] = [
    "backup_locked",
    "backup_file",
    "backup_not_this_identity",
    "backup_storage",
  ];
  return codes.find((code) => code === error) ?? "backup_storage";
}
