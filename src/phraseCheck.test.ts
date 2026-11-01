import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const { phraseVerdict } = await import("./identity");
const { confirmationFor, verdictNote } = await import("./phraseCheck");
const { emptyVault } = await import("./vault");
const { catalogues } = await import("./i18n/locales");

const t = catalogues.en;

describe("checking the recovery phrase", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("asks for the PIN where there is one, with biometrics when they open the wallet", () => {
    expect(confirmationFor({ ...emptyVault, pin: true })).toEqual({ way: "pin", biometrics: false });
    expect(confirmationFor({ ...emptyVault, pin: true, deviceKey: true })).toEqual({
      way: "pin",
      biometrics: true,
    });
    expect(
      confirmationFor({ ...emptyVault, pin: true, deviceKey: true, deviceChanged: true }),
    ).toEqual({ way: "pin", biometrics: false });
  });

  it("asks the device where there is no PIN", () => {
    expect(confirmationFor({ ...emptyVault, pin: false })).toEqual({ way: "device" });
  });

  it("hands the words to Rust and turns its answer into a verdict", async () => {
    invoke.mockResolvedValueOnce(true);
    expect(await phraseVerdict("some words")).toBe("match");
    expect(invoke).toHaveBeenCalledWith("identity_check_phrase", { input: "some words" });

    invoke.mockResolvedValueOnce(false);
    expect(await phraseVerdict("other words")).toBe("mismatch");
  });

  it("turns its refusals into verdicts too", async () => {
    invoke.mockRejectedValueOnce("identity_checksum");
    expect(await phraseVerdict("abandon")).toBe("identity_checksum");
    invoke.mockRejectedValueOnce("identity_unconfirmed");
    expect(await phraseVerdict("abandon")).toBe("identity_unconfirmed");
    invoke.mockRejectedValueOnce(new Error("anything else"));
    expect(await phraseVerdict("abandon")).toBe("identity_unknown");
  });

  it("words a match plainly and everything else as a warning", () => {
    expect(verdictNote(t, "match")).toEqual({ text: t.phraseCheck.match, warning: false });
    expect(verdictNote(t, "mismatch")).toEqual({ text: t.phraseCheck.mismatch, warning: true });
    expect(verdictNote(t, "identity_word_count").text).toBe(t.onboarding.errors.identity_word_count);
    expect(verdictNote(t, "identity_unknown").text).toBe(t.phraseCheck.errors.unknown);
  });
});
