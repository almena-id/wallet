import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-deep-link", () => ({ getCurrent: vi.fn(), onOpenUrl: vi.fn() }));

const { choiceToSend, pickedOption } = await import("./links");
type Presenting = import("./links").Presenting;

const option = (id: string) => ({
  id,
  credential: { en: "Membership" },
  issuer: "Club",
  issuedAt: 1,
  claims: [{ name: "member_number", value: "0042" }],
});

describe("what the sheet presents", () => {
  const one: Presenting = { set: 0, options: [option("x")] };
  const two: Presenting = { set: 1, options: [option("a"), option("b")] };

  it("sends the first of each set until another is picked", () => {
    expect(pickedOption(two, {}).id).toBe("a");
    expect(pickedOption(two, { 1: "b" }).id).toBe("b");
    // A choice that is not among the options leaves the first.
    expect(pickedOption(two, { 1: "z" }).id).toBe("a");
    // Another set's choice does not reach this one.
    expect(pickedOption(two, { 0: "b" }).id).toBe("a");
  });

  it("names on Accept only the sets that had a choice to make", () => {
    expect(choiceToSend([one, two], {})).toEqual({ 1: "a" });
    expect(choiceToSend([one, two], { 0: "x", 1: "b" })).toEqual({ 1: "b" });
    expect(choiceToSend([], {})).toEqual({});
  });
});
