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
  /** The thread the message names, and the one above it — the run, for a request. */
  thread: string | null;
  parentThread: string | null;
  /**
   * For a credential request this wallet sent, what the person saw and
   * authorised: the credential by name and the answers under their labels.
   * The body carries the answers by key only. Null for anything else, and
   * for a request written before the wallet kept this.
   */
  summary: RequestSummary | null;
};

export type RequestSummary = {
  credential: string;
  fields: { key: string; label: string; value: string }[];
};

/**
 * One thing said between this wallet and a counterparty, however many
 * messages it took: a credential request is the `request` this wallet
 * sent — the form, with what the person filled in — and whatever the
 * issuer answers. The `accept` that moved the page on to the form is not
 * among them: it was a signal, not something said, and it is not kept.
 *
 * Read off the book rather than written to it: a message belongs to the
 * run it names, else the thread it names, else itself, and the protocol
 * is the first message's type without its last word.
 */
export type Thread = {
  key: string;
  counterparty: string;
  protocol: string;
  /** Oldest first. */
  messages: Entry[];
  /** The newest message's time, for ordering the inbox. */
  lastTime: number | null;
  unread: number;
  /**
   * The request this wallet sent, for a credential request: what the
   * thread is about. Null for a thread of another protocol, or one whose
   * request this wallet does not hold.
   */
  request: Entry | null;
};

/** The platform's protocol for asking an issuer for a credential, and the
 * type the second code declares itself with and the request is sent as. */
const CREDENTIAL_REQUEST = "https://almena.id/credential-request/1.0";
const REQUEST_TYPE = `${CREDENTIAL_REQUEST}/request`;

/** The protocol a message type belongs to: the type without its last word. */
export function protocolOf(type: string): string {
  const cut = type.lastIndexOf("/");
  return cut > 0 ? type.slice(0, cut) : type;
}

/**
 * The name of a protocol, out of its URI: `…/credential-request/1.0` is
 * *credential-request*. The version at the end is not the name.
 */
export function protocolKind(protocol: string): string {
  const parts = protocol.split("/").filter((part) => part.length > 0);
  if (parts.length > 1 && /^\d+(\.\d+)*$/.test(parts[parts.length - 1])) {
    parts.pop();
  }
  return parts[parts.length - 1] ?? protocol;
}

/** The book's messages, gathered into threads, newest activity first. */
export function threadsOf(book: Book): Thread[] {
  const byKey = new Map<string, Entry[]>();
  for (const entry of book.messages) {
    const key = entry.parentThread ?? entry.thread ?? entry.id;
    byKey.set(key, [...(byKey.get(key) ?? []), entry]);
  }
  const threads: Thread[] = [];
  for (const [key, entries] of byKey) {
    const messages = [...entries].sort(
      (a, b) => (a.createdTime ?? 0) - (b.createdTime ?? 0) || a.id.localeCompare(b.id),
    );
    const last = messages[messages.length - 1];
    threads.push({
      key,
      counterparty: messages[0].counterparty,
      protocol: protocolOf(messages[0].type),
      messages,
      lastTime: last.createdTime,
      unread: messages.filter((m) => !m.read).length,
      request: messages.find((m) => m.sent && m.type === REQUEST_TYPE) ?? null,
    });
  }
  return threads.sort((a, b) => (b.lastTime ?? 0) - (a.lastTime ?? 0));
}

/**
 * The credentials this wallet has asked for, newest activity first: the
 * threads of the platform's credential request protocol. What the inbox
 * lists — a thread of any other protocol is not something the person
 * asked for, and is not put in front of them.
 */
export function requestsOf(book: Book): Thread[] {
  return threadsOf(book).filter((thread) => thread.protocol === CREDENTIAL_REQUEST);
}

/** What a thread is called on screen: the credential it asked for, when known. */
export function threadName(thread: Thread): string {
  return thread.request?.summary?.credential ?? protocolKind(thread.protocol);
}

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

/**
 * The goal code the DIDComm community uses for "I will issue you a
 * credential": what the marketplace's invitation carries. The Rust side
 * reads it the same way, in `Invitation::asks_for_credential`.
 */
const ISSUE_GOAL = "issue-vc";

/**
 * Whether opening this invitation starts a request for a credential — the
 * marketplace's first code — rather than just a relationship. The two are
 * shown differently: a request is put to the person as the request it is,
 * with the issuer named, and never as the invitation that carries it.
 */
export function asksForCredential(invitation: Invitation): boolean {
  return invitation.id !== null && invitation.goalCode === ISSUE_GOAL;
}

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

/** What sending came to: whom it went to, and the thread it is now part of. */
export type Sent = {
  counterparty: string;
  thread: string;
};

/**
 * Sends the request the second code describes to the issuer it names, and
 * keeps it as sent. The relationship is opened first if this wallet has
 * none, at `mediator`.
 */
export function sendRequest(input: string, mediator: string): Promise<Sent> {
  return invoke<Sent>("messaging_send_request", { input, mediator });
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
