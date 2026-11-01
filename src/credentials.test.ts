import { describe, expect, it } from "vitest";

import {
  RECHECK_AFTER_MS,
  credentialName,
  dueForCheck,
  issuerLabel,
  registryOf,
  shown,
  standing,
  type HeldCredential,
} from "./credentials";

const NOW = Date.UTC(2026, 10, 1);
const seconds = (ms: number) => Math.floor(ms / 1000);

function held(change: Partial<HeldCredential> = {}): HeldCredential {
  return {
    id: "c1",
    format: "dc+sd-jwt",
    issuerDid: "did:webvh:scid:issuer.example",
    issuerName: "City Council",
    typeId: "residence",
    typeLabels: { en: "Proof of residence", es: "Certificado de residencia" },
    claims: {},
    issuedAt: seconds(NOW) - 86400,
    validUntil: seconds(NOW) + 86400,
    receivedAt: seconds(NOW) - 86400,
    scope: "https://almena.id did:webvh:scid:issuer.example",
    checked: { status: "valid", at: seconds(NOW), problem: null },
    ...change,
  };
}

describe("credentialName", () => {
  it("prefers the exact locale, then its language, then English", () => {
    const credential = held({ typeLabels: { en: "Residence", es: "Residencia", "es-MX": "Domicilio" } });
    expect(credentialName(credential, "es-MX")).toBe("Domicilio");
    expect(credentialName(credential, "es-ES")).toBe("Residencia");
    expect(credentialName(credential, "fr")).toBe("Residence");
  });

  it("falls back to any name, then to the catalogue id", () => {
    expect(credentialName(held({ typeLabels: { de: "Wohnsitz" } }), "en")).toBe("Wohnsitz");
    expect(credentialName(held({ typeLabels: {} }), "en")).toBe("residence");
  });
});

describe("issuerLabel", () => {
  it("is the issuer's name, or its DID when the name is blank", () => {
    expect(issuerLabel(held())).toBe("City Council");
    expect(issuerLabel(held({ issuerName: "  " }))).toBe("did:webvh:scid:issuer.example");
  });
});

describe("registryOf", () => {
  it("reads the registry's host from the scope", () => {
    expect(registryOf(held())).toBe("almena.id");
    expect(registryOf(held({ scope: "http://localhost:8000 did:x" }))).toBe("localhost:8000");
    expect(registryOf(held({ scope: "" }))).toBeNull();
  });
});

describe("standing", () => {
  it("says what the issuer's list said", () => {
    expect(standing(held(), NOW)).toBe("valid");
    expect(standing(held({ checked: { status: "revoked", at: 1, problem: null } }), NOW)).toBe("revoked");
    expect(standing(held({ checked: { status: "suspended", at: 1, problem: "unreachable" } }), NOW)).toBe(
      "suspended",
    );
  });

  it("keeps the last status known while the list cannot be reached", () => {
    expect(standing(held({ checked: { status: "valid", at: 1, problem: "unreachable" } }), NOW)).toBe("valid");
  });

  it("is not checked while nothing is known", () => {
    for (const problem of [null, "noList", "untrusted", "unreachable"] as const) {
      expect(standing(held({ checked: { status: null, at: null, problem } }), NOW)).toBe("unchecked");
    }
  });

  it("is not valid once its issuer no longer lists the key that signed it", () => {
    expect(standing(held({ checked: { status: null, at: 1, problem: "signature" } }), NOW)).toBe("invalid");
  });

  it("is expired past its end date, unless the issuer said worse", () => {
    const past = seconds(NOW) - 1;
    expect(standing(held({ validUntil: past }), NOW)).toBe("expired");
    expect(
      standing(held({ validUntil: past, checked: { status: "revoked", at: 1, problem: null } }), NOW),
    ).toBe("revoked");
  });
});

describe("dueForCheck", () => {
  it("is due when one was never checked or not for a while", () => {
    expect(dueForCheck([held()], NOW)).toBe(false);
    expect(dueForCheck([held(), held({ checked: { status: null, at: null, problem: null } })], NOW)).toBe(true);
    const stale = held({ checked: { status: "valid", at: seconds(NOW - RECHECK_AFTER_MS), problem: null } });
    expect(dueForCheck([stale], NOW)).toBe(true);
    expect(dueForCheck([], NOW)).toBe(false);
  });
});

describe("shown", () => {
  it("joins lists and groups", () => {
    expect(shown(["a", "b"])).toBe("a, b");
    expect(shown({ street: "Main", city: { name: "Madrid" } })).toBe("Main, Madrid");
    expect(shown(null)).toBe("");
    expect(shown(42)).toBe("42");
  });
});
