import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";

import type { Contact } from "./contacts";

/**
 * Links and codes from outside: an `almena://` link that opened the wallet, or
 * a code the scanner read. **Nothing from outside acts on the identity.** What
 * it is decides which screen it is carried to — accepting an invitation,
 * connecting to a mediator, answering a registry — and nothing happens until
 * somebody says so there.
 */
export type LinkKind = "contact" | "mediator" | "registry" | "unknown";

/** What a link or a code is, read on the Rust side without acting on it. */
export function invitationKind(input: string): Promise<LinkKind> {
  return invoke<LinkKind>("invitation_kind", { input }).catch(() => "unknown");
}

/** What the confirmation sheet shows about a link or a code — see `link_details`. */
export type LinkDetails =
  | {
      kind: "contact";
      /** The fingerprint of the card the invitation names: it carries no name. */
      fingerprint: string;
      /** The relationship already opened with that card. */
      contact: Contact | null;
      /** This wallet's own invitation. */
      own: boolean;
    }
  | {
      kind: "mediator";
      mediator: string;
      current: string | null;
      /** Relationships routed through the current mediator. */
      contacts: number;
    }
  | { kind: "unknown" };

/** What a link or a code is, with what this wallet knows about it. Needs the wallet open. */
export function linkDetails(input: string): Promise<LinkDetails> {
  return invoke<LinkDetails>("link_details", { input });
}

/**
 * A registry portal asking this wallet to sign in, or to link to an account
 * there — read over the network, nothing sent. See `registry.rs`.
 */
export type RegistryRequest = {
  /** The portal's name, as it gives it. */
  name: string;
  /** The portal's host: what vouches for the name. */
  portal: string;
  /** The host the answer goes to. */
  answerTo: string;
  purpose: "sign_in" | "link" | "sign";
  /** The DID this wallet is known by at that portal. */
  did: string;
  /** `sign`: what is signed. */
  signing: {
    /**
     * `did_log_entry`: a version of an identity's DID; `endorsement`: an
     * organisation vouching for one of its issuers, verifiers or mediators.
     */
    kind: "did_log_entry" | "endorsement";
    /** The identity's or the item's name. */
    identity: string;
    tenant: string | null;
    /** `null` for the first entry, which makes the DID. */
    did: string | null;
    version: number | null;
    validUntil: string | null;
    /** This wallet's key may sign it. */
    signer: boolean;
  } | null;
};

export type RegistryErrorCode =
  | "registry_locked"
  | "registry_unreadable"
  | "registry_insecure"
  | "registry_unreachable"
  | "registry_expired"
  | "registry_refused"
  | "registry_not_a_signer";

export function registryRequest(input: string): Promise<RegistryRequest> {
  return invoke<RegistryRequest>("registry_request", { input });
}

/** Answers the request: what Accept does. */
export function registryAnswer(input: string): Promise<void> {
  return invoke<void>("registry_answer", { input });
}

export function registryErrorCode(error: unknown): RegistryErrorCode {
  const codes: RegistryErrorCode[] = [
    "registry_locked",
    "registry_unreadable",
    "registry_insecure",
    "registry_unreachable",
    "registry_expired",
    "registry_refused",
    "registry_not_a_signer",
  ];
  return typeof error === "string" && (codes as string[]).includes(error)
    ? (error as RegistryErrorCode)
    : "registry_unreachable";
}

/**
 * Calls `handle` with every `almena://` link the wallet is opened with: the
 * one that started it, and each one after — on a computer a link opened while
 * the wallet runs arrives through `single-instance`, on a phone through the
 * system.
 */
export function useDeepLinks(handle: (url: string) => void) {
  const latest = useRef(handle);
  latest.current = handle;

  useEffect(() => {
    let stop: (() => void) | undefined;
    let active = true;

    getCurrent()
      .then((urls) => {
        const first = urls?.[0];
        if (active && first) {
          latest.current(first);
        }
      })
      .catch(() => undefined);
    onOpenUrl((urls) => {
      const first = urls[0];
      if (first) {
        latest.current(first);
      }
    })
      .then((unlisten) => {
        if (active) {
          stop = unlisten;
        } else {
          unlisten();
        }
      })
      // No native backend behind the webview.
      .catch(() => undefined);

    return () => {
      active = false;
      stop?.();
    };
  }, []);
}
