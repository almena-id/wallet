import { beforeEach, describe, expect, it, vi } from "vitest";

const store = new Map<string, string>();
vi.stubGlobal("window", {
  localStorage: {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => void store.set(key, value),
  },
});

const { backupDue, dismissBackupReminder, markBackedUp, readBackupMark } = await import("./backup");

describe("the backup reminder", () => {
  beforeEach(() => store.clear());

  it("asks once something arrived that no backup carries", () => {
    const held = [{ receivedAt: 100 }];
    expect(backupDue(held, readBackupMark())).toBe(true);
    expect(backupDue([], readBackupMark())).toBe(false);

    // A backup after it: quiet. A credential after the backup: again.
    const mark = markBackedUp(150);
    expect(backupDue(held, mark)).toBe(false);
    expect(backupDue([...held, { receivedAt: 200 }], mark)).toBe(true);
  });

  it("is quiet once put away, until the next credential", () => {
    const mark = dismissBackupReminder(300);
    expect(backupDue([{ receivedAt: 250 }], mark)).toBe(false);
    expect(backupDue([{ receivedAt: 301 }], mark)).toBe(true);
    // Both marks are kept: a backup does not forget the dismissal.
    expect(markBackedUp(310)).toEqual({ backedUp: 310, dismissed: 300 });
  });

  it("reads storage it cannot make sense of as nothing marked", () => {
    store.set("almena.backup.reminder", "{not json");
    expect(readBackupMark()).toEqual({ backedUp: 0, dismissed: 0 });
  });
});
