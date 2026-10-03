import { describe, expect, it } from "vitest";

import { popped, pushed, replaced } from "./nav";

describe("a screen stack", () => {
  it("goes back to where somebody came from, and never past the first screen", () => {
    const inbox = pushed(["inbox"], "new");
    const conversation = pushed(inbox, "conversation");
    expect(popped(conversation)).toEqual(["inbox", "new"]);
    expect(popped(popped(conversation))).toEqual(["inbox"]);
    expect(popped(["inbox"])).toEqual(["inbox"]);
  });

  it("replaces the top without changing what is under it", () => {
    expect(replaced(["inbox", "new"], "conversation")).toEqual(["inbox", "conversation"]);
  });
});
