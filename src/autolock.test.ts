import { describe, expect, it } from "vitest";

import { defaultAutoLock, idleLeft, readAutoLock } from "./autolock";

describe("auto-lock", () => {
  const minute = 60_000;

  it("counts from the last sign of somebody, through time away", () => {
    // Used at 0, a minute's lock: half left at 30 s, due at a minute.
    expect(idleLeft(0, 30_000, minute)).toBe(30_000);
    expect(idleLeft(0, minute, minute)).toBe(0);
    // Back after an hour in the background, a frozen timer notwithstanding.
    expect(idleLeft(0, 60 * minute, minute)).toBe(0);
    // Used again: the count starts over.
    expect(idleLeft(50_000, 60_000, minute)).toBe(50_000);
  });

  it("takes only the lengths offered, and the shortest otherwise", () => {
    expect(readAutoLock("5")).toBe(5);
    expect(readAutoLock("10")).toBe(10);
    expect(readAutoLock("7")).toBe(defaultAutoLock);
    expect(readAutoLock(null)).toBe(defaultAutoLock);
    expect(readAutoLock("ten")).toBe(defaultAutoLock);
    expect(defaultAutoLock).toBe(1);
  });
});
