import { describe, expect, it } from "vitest";

import { presentedOf, recipientLabel, type Presentation } from "./presented";

function presentation(id: string, at: number, credentials: [string, string[]][]): Presentation {
  return {
    id,
    at,
    purpose: "verify",
    to: "Door",
    toDid: "did:webvh:Qm:almena.id:ids:idn_door",
    verified: true,
    domains: ["club.example"],
    portal: "registry.almena.id",
    credentials: credentials.map(([credential, claims]) => ({
      credential,
      typeLabels: {},
      issuerName: "Club",
      claims,
    })),
  };
}

describe("what was presented", () => {
  it("lists one credential's presentations, newest first, with its own claims", () => {
    const log = [
      presentation("a", 1, [["x", ["member_number"]]]),
      presentation("b", 3, [["y", ["birthdate"]], ["x", ["given_name"]]]),
      presentation("c", 2, [["y", ["birthdate"]]]),
    ];
    const ofX = presentedOf(log, "x");
    expect(ofX.map((one) => one.presentation.id)).toEqual(["b", "a"]);
    expect(ofX[0].claims).toEqual(["given_name"]);
    expect(presentedOf(log, "z")).toEqual([]);
  });

  it("says the domains only when the verifier was checked", () => {
    const checked = presentation("a", 1, []);
    expect(recipientLabel(checked)).toBe("Door · club.example");
    expect(recipientLabel({ ...checked, verified: false })).toBe("Door");
    expect(recipientLabel({ ...checked, domains: [] })).toBe("Door");
  });
});
