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

/**
 * **The backup reminder.** When credentials arrive that no backup carries
 * yet, Home suggests making one — once: dismissing it, or making a backup
 * from anywhere, quiets it until the next credential arrives. A note about
 * this device, holding nothing about the identity, kept beside the other
 * device preferences.
 */
const reminderKey = "almena.backup.reminder";

/** When a backup was last made, and when the reminder was last put away; seconds. */
export type BackupMark = { backedUp: number; dismissed: number };

const noMark: BackupMark = { backedUp: 0, dismissed: 0 };

export function readBackupMark(): BackupMark {
  try {
    const value = JSON.parse(window.localStorage.getItem(reminderKey) ?? "null") as unknown;
    if (
      typeof value === "object" &&
      value !== null &&
      typeof (value as BackupMark).backedUp === "number" &&
      typeof (value as BackupMark).dismissed === "number"
    ) {
      return value as BackupMark;
    }
  } catch {
    // Unreadable or refused storage: as if nothing had been marked.
  }
  return noMark;
}

function writeBackupMark(mark: BackupMark) {
  try {
    window.localStorage.setItem(reminderKey, JSON.stringify(mark));
  } catch {
    // Without storage the reminder comes back after a restart; nothing worse.
  }
}

/** A backup was just made: everything held now is in it. */
export function markBackedUp(now = Date.now() / 1000): BackupMark {
  const mark = { ...readBackupMark(), backedUp: Math.floor(now) };
  writeBackupMark(mark);
  return mark;
}

/** The reminder was put away: quiet until another credential arrives. */
export function dismissBackupReminder(now = Date.now() / 1000): BackupMark {
  const mark = { ...readBackupMark(), dismissed: Math.floor(now) };
  writeBackupMark(mark);
  return mark;
}

/** Whether a credential arrived after the last backup and the last dismissal. */
export function backupDue(held: { receivedAt: number }[], mark: BackupMark): boolean {
  const quiet = Math.max(mark.backedUp, mark.dismissed);
  return held.some((credential) => credential.receivedAt > quiet);
}
