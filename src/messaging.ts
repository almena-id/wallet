import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";

/**
 * The relationships this wallet has, and what came through them.
 *
 * Everything here is read back from the sealed book on the Rust side; nothing
 * is kept on this side that outlives the screen. The pairwise of a
 * relationship is shown for what it is — the identifier the wallet is in that
 * relationship — and the identity itself is never among these.
 */

export type Relationship = {
  /** The DID the relationship is with. */
  counterparty: string;
  /** The `did:key` this wallet is in this relationship. */
  pairwise: string;
  /** The mediator this relationship's mailbox is at. */
  mediator: string;
  /** What the invitation called the counterparty, when it said. */
  label: string | null;
};

/**
 * A message of a relationship: one that came through it, or one this wallet
 * sent — the receipt the spec has the wallet write for itself.
 */
export type Entry = {
  id: string;
  counterparty: string;
  /** The protocol message type, a URI. */
  type: string;
  from: string | null;
  body: Record<string, unknown>;
  /** Seconds since the epoch, as the sender wrote it. */
  createdTime: number | null;
  read: boolean;
  /** Whether this wallet is the one that sent it. */
  sent: boolean;
};

export type Book = {
  relationships: Relationship[];
  messages: Entry[];
};

export type Collected = {
  received: number;
  /** Counterparties whose mediator did not answer. */
  unreachable: string[];
};

export type Invitation = {
  counterparty: string;
  label: string | null;
  id: string | null;
  goalCode: string | null;
};

/**
 * The second code the marketplace shows, read: what the request would be.
 * `acquisition` is the run it answers, `issuer` the DID it goes to.
 */
export type CredentialRequest = {
  acquisition: string;
  issuer: string;
  issuerName: string;
  credential: string;
  template: { slug: string; version: number };
  fields: { key: string; label: string; value: string }[];
};

/** The type the second code declares itself with. */
const REQUEST_TYPE = "https://almena.id/credential-request/1.0/request";

const CODES = [
  "messaging_locked",
  "messaging_invitation_unreadable",
  "messaging_mediator_unreachable",
  "messaging_mediator_refused",
  "messaging_unreadable",
  "messaging_too_new",
  "messaging_storage",
  "messaging_entropy",
  "messaging_request_unreadable",
  "messaging_counterparty_unreachable",
] as const;

export type MessagingErrorCode = (typeof CODES)[number] | "messaging_unknown";

export function errorCode(error: unknown): MessagingErrorCode {
  return typeof error === "string" && (CODES as readonly string[]).includes(error)
    ? (error as MessagingErrorCode)
    : "messaging_unknown";
}

/** Who an invitation is from, before anybody opens it. */
export function readInvitation(input: string): Promise<Invitation> {
  return invoke<Invitation>("messaging_read_invitation", { input });
}

/** Whether a scanned or pasted string is something `readInvitation` reads. */
export function looksLikeInvitation(input: string): boolean {
  const trimmed = input.trim();
  return (
    trimmed.startsWith("did:") ||
    trimmed.includes("_oob=") ||
    (trimmed.startsWith("{") && !looksLikeRequest(trimmed))
  );
}

/** Whether a scanned string is the second code, the one `readRequest` reads. */
export function looksLikeRequest(input: string): boolean {
  const trimmed = input.trim();
  return trimmed.startsWith("{") && trimmed.includes(REQUEST_TYPE);
}

/**
 * Opens the relationship an invitation is from. The marketplace's is also
 * answered: the wallet says `accept`, which is what moves the page that
 * showed the code on to the form.
 */
export function openRelationship(invitation: string, mediator: string): Promise<Relationship> {
  return invoke<Relationship>("messaging_open", { invitation, mediator });
}

/** What the second code says the request would be, before anybody sends it. */
export function readRequest(input: string): Promise<CredentialRequest> {
  return invoke<CredentialRequest>("messaging_read_request", { input });
}

/**
 * Sends the request the second code describes to the issuer it names, and
 * keeps it as sent. The relationship is opened first if this wallet has
 * none, at `mediator`.
 */
export function sendRequest(input: string, mediator: string): Promise<Relationship> {
  return invoke<Relationship>("messaging_send_request", { input, mediator });
}

export type Shown = {
  /** The out-of-band invitation as a URL, which is what the code carries. */
  url: string;
};

/**
 * A fresh invitation for whoever is in front of the wallet, its mailbox
 * opened at `mediator`. Each call replaces the invitation before it: what is
 * shown is always the last one asked for, and only that one is collected.
 */
export function showInvitation(mediator: string): Promise<Shown> {
  return invoke<Shown>("messaging_invite", { mediator });
}

/**
 * The last part of a protocol message type, which is the word the protocol
 * uses for it: `…/messagepickup/3.0/delivery` is *delivery*. Shown as it is,
 * because it is an identifier and not a sentence.
 */
export function messageKind(type: string): string {
  const parts = type.split("/").filter((part) => part.length > 0);
  return parts[parts.length - 1] ?? type;
}

/** What a relationship is called on screen: its label, or who it is with. */
export function relationshipName(relationship: Relationship): string {
  return relationship.label ?? relationship.counterparty;
}

export type Messaging = {
  book: Book;
  /** False until the book has been read, so nothing is drawn on a guess. */
  read: boolean;
  /** Whether a collection is under way. */
  collecting: boolean;
  /** Reads the book again. */
  refresh: () => Promise<void>;
  /** Empties every mailbox and reads the book again. */
  collect: () => Promise<Collected>;
  /** Marks a message as opened, and shows it so. */
  markRead: (id: string) => Promise<void>;
};

const emptyBook: Book = { relationships: [], messages: [] };

/**
 * How often the mailboxes are emptied while the wallet is open, in
 * milliseconds. The mediator is passage, not storage: what it holds for
 * this wallet is meant to be gone from it as soon as it can be, and the
 * spec has the mailboxes read on opening the wallet and refreshed in the
 * background after that. Short enough that what an issuer sends back is
 * on the screen before anybody thinks to ask for it; long enough that a
 * wallet with dozens of relationships is not a wallet that never stops
 * talking to its mediator.
 */
export const COLLECT_EVERY_MS = 15_000;

/**
 * The book, as the screens see it. `open` is the flag that says an identity is
 * open: the book is readable exactly then, and asking earlier is asking for
 * `messaging_locked`.
 *
 * While it is open the mailboxes are emptied on a clock — once on opening,
 * then every `COLLECT_EVERY_MS`, and again whenever the wallet comes back
 * into view after being put away, because a webview's timers do not run
 * while nobody can see it. What a background collection brings is read
 * into the book and nothing is said about it; the button on the inbox is
 * the one that reports.
 */
export function useMessaging(open: boolean): Messaging {
  const [book, setBook] = useState<Book>(emptyBook);
  const [read, setRead] = useState(false);
  const [collecting, setCollecting] = useState(false);
  // One collection at a time, whichever clock asked: a second one started
  // while the first is out would be handed the same messages twice.
  const busy = useRef(false);

  const refresh = useCallback(async () => {
    try {
      setBook(await invoke<Book>("messaging_book"));
    } catch {
      // A book that cannot be read is shown empty; the commands that write
      // it say what is wrong when they are asked to.
      setBook(emptyBook);
    } finally {
      setRead(true);
    }
  }, []);

  useEffect(() => {
    if (open) {
      void refresh();
    } else {
      setBook(emptyBook);
      setRead(false);
    }
  }, [open, refresh]);

  const collect = useCallback(async () => {
    if (busy.current) {
      return { received: 0, unreachable: [] };
    }
    busy.current = true;
    setCollecting(true);
    try {
      const collected = await invoke<Collected>("messaging_collect");
      await refresh();
      return collected;
    } finally {
      busy.current = false;
      setCollecting(false);
    }
  }, [refresh]);

  useEffect(() => {
    if (!open) {
      return;
    }
    const quietly = () => {
      collect().catch(() => {
        // Not reported: a mediator that did not answer in the background is
        // asked again on the next tick, and the inbox's own button says so
        // when somebody asks.
      });
    };
    quietly();
    const timer = window.setInterval(quietly, COLLECT_EVERY_MS);
    const onVisible = () => {
      if (document.visibilityState === "visible") {
        quietly();
      }
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [open, collect]);

  const markRead = useCallback(async (id: string) => {
    await invoke<void>("messaging_mark_read", { id });
    setBook((current) => ({
      ...current,
      messages: current.messages.map((m) => (m.id === id ? { ...m, read: true } : m)),
    }));
  }, []);

  return { book, read, collecting, refresh, collect, markRead };
}
