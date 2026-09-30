import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * The wallet's mediation: which mediator holds its messages while it is not
 * listening. Everything behind these calls happens on the Rust side, with keys
 * derived from the seed — see `src-tauri/src/messaging`.
 */
export type MediatorStatus = {
  /** The mediator's DID, when there is a mediation. */
  mediator: string | null;
  /** The inbox DID registered with it. */
  inbox: string | null;
};

/** What is waiting at the mediator. */
export type Waiting = {
  messageCount: number;
};

/**
 * The mediator offered when somebody connects, which they can replace with any
 * other mediator's invitation link, address or DID.
 *
 * A development build may offer its own: `VITE_MEDIATOR` in `.env.local`, e.g.
 * `http://localhost:8080` for the mediator repository's
 * `ALMENA_PUBLIC_URL=http://localhost:8080 task dev:memory`. It is read only in
 * development, so a release offers the public one and nothing else.
 */
export const suggestedMediator: string =
  (import.meta.env.DEV ? import.meta.env.VITE_MEDIATOR : undefined) ??
  "https://mediator.almena.id";

export function readMediator(): Promise<MediatorStatus> {
  return invoke<MediatorStatus>("mediator_status");
}

/** Asks the mediator `input` names for mediation and registers the inbox with it. */
export function connectMediator(input: string): Promise<MediatorStatus> {
  return invoke<MediatorStatus>("mediator_connect", { input });
}

/** Asks the mediator how many messages it is holding. */
export function checkMediator(): Promise<Waiting> {
  return invoke<Waiting>("mediator_check");
}

/** Leaves the mediator, and forgets it even if it does not answer. */
export function disconnectMediator(): Promise<MediatorStatus> {
  return invoke<MediatorStatus>("mediator_disconnect");
}

/**
 * The error codes the messaging commands answer with. They are codes and not
 * sentences, so this side says them in the language somebody is reading.
 */
export type MessagingErrorCode =
  | "messaging_locked"
  | "messaging_invitation_unreadable"
  | "messaging_invitation_own"
  | "messaging_insecure"
  | "messaging_mediator_unreachable"
  | "messaging_mediator_refused"
  | "messaging_counterparty_unreachable"
  | "messaging_not_connected"
  | "messaging_pending"
  | "messaging_contact_unknown"
  | "messaging_message_invalid"
  | "messaging_photo_invalid"
  | "messaging_keys"
  | "messaging_unreadable"
  | "messaging_storage"
  | "messaging_entropy"
  | "messaging_calls_unavailable"
  | "messaging_unknown";

const CODES: MessagingErrorCode[] = [
  "messaging_locked",
  "messaging_invitation_unreadable",
  "messaging_invitation_own",
  "messaging_insecure",
  "messaging_mediator_unreachable",
  "messaging_mediator_refused",
  "messaging_counterparty_unreachable",
  "messaging_not_connected",
  "messaging_pending",
  "messaging_contact_unknown",
  "messaging_message_invalid",
  "messaging_photo_invalid",
  "messaging_keys",
  "messaging_unreadable",
  "messaging_storage",
  "messaging_entropy",
  "messaging_calls_unavailable",
];

/** Whatever a rejected command threw, as a code this interface has a word for. */
export function errorCode(error: unknown): MessagingErrorCode {
  return typeof error === "string" && (CODES as string[]).includes(error)
    ? (error as MessagingErrorCode)
    : "messaging_unknown";
}

/**
 * The name a mediator is shown under: the host its `did:web` is served from,
 * with any path after it. `did:web:localhost%3A8080` is *localhost:8080*.
 */
export function mediatorName(did: string): string {
  const prefix = "did:web:";
  if (!did.startsWith(prefix)) {
    return did;
  }
  return did
    .slice(prefix.length)
    .split(":")
    .map((segment) => decodeURIComponent(segment))
    .join("/");
}

export type Mediation = {
  status: MediatorStatus | null;
  /** Why the status could not be read, as a code. */
  error: MessagingErrorCode | null;
  adopt: (status: MediatorStatus) => void;
};

/** The mediation, read from the device when the screen that shows it opens. */
export function useMediation(): Mediation {
  const [status, setStatus] = useState<MediatorStatus | null>(null);
  const [error, setError] = useState<MessagingErrorCode | null>(null);

  useEffect(() => {
    readMediator()
      .then(setStatus)
      .catch((failure) => setError(errorCode(failure)));
  }, []);

  return {
    status,
    error,
    adopt: useCallback((next: MediatorStatus) => {
      setError(null);
      setStatus(next);
    }, []),
  };
}
