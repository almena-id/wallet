import { invoke } from "@tauri-apps/api/core";

/**
 * One presentation this wallet made: to whom, when, which credentials and the
 * names of the claims that went. Kept sealed on the device (`presented.rs`).
 */
export type Presentation = {
  id: string;
  /** Seconds since the epoch. */
  at: number;
  /** `present` (to an issuer, applying) or `verify` (to a verifier). */
  purpose: "present" | "verify";
  /** Who it went to, as the sheet named them. */
  to: string;
  toDid: string;
  /** `verify`: whether the wallet checked the verifier itself; `null` where nothing was. */
  verified: boolean | null;
  /** `verify`: the domains its organisation's document links to it. */
  domains: string[];
  /** The registry portal it went through. */
  portal: string;
  credentials: {
    credential: string;
    typeLabels: Record<string, string>;
    issuerName: string;
    claims: string[];
  }[];
};

/** Everything presented, newest first. Needs the wallet open. */
export function listPresented(): Promise<Presentation[]> {
  return invoke<Presentation[]>("presented_list");
}

/**
 * The times one credential was presented, newest first, each with the claims
 * of it that went.
 */
export function presentedOf(
  log: Presentation[],
  credential: string,
): { presentation: Presentation; claims: string[] }[] {
  return log
    .flatMap((presentation) =>
      presentation.credentials
        .filter((shared) => shared.credential === credential)
        .map((shared) => ({ presentation, claims: shared.claims })),
    )
    .sort((a, b) => b.presentation.at - a.presentation.at);
}

/** Who a presentation went to, as a line: the name, and the domains when they were checked. */
export function recipientLabel(presentation: Presentation): string {
  return presentation.verified && presentation.domains.length > 0
    ? `${presentation.to} · ${presentation.domains.join(", ")}`
    : presentation.to;
}
