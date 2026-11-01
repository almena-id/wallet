import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const { errorCode, pinLengths } = await import("./vault");

describe("the vault", () => {
  it("offers four digits only where the device takes them", () => {
    expect(pinLengths(4)).toEqual([4, 6]);
    expect(pinLengths(6)).toEqual([6]);
  });

  it("knows the codes for a PIN too short and an unconfirmed sign-out", () => {
    expect(errorCode("vault_pin_too_short")).toBe("vault_pin_too_short");
    expect(errorCode("vault_unconfirmed")).toBe("vault_unconfirmed");
    expect(errorCode("something else")).toBe("vault_unknown");
  });
});
