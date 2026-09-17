import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";

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

export type Received = {
  id: string;
  counterparty: string;
  /** The protocol message type, a URI. */
  type: string;
  from: string | null;
  body: Record<string, unknown>;
  /** Seconds since the epoch, as the sender wrote it. */
  createdTime: number | null;
  read: boolean;
};

export type Book = {
  relationships: Relationship[];
  messages: Received[];
};

export type Collected = {
  received: number;
  /** Counterparties whose mediator did not answer. */
  unreachable: string[];
};

export type Invitation = {
  counterparty: string;
  label: string | null;
};

const CODES = [
  "messaging_locked",
  "messaging_invitation_unreadable",
  "messaging_mediator_unreachable",
  "messaging_mediator_refused",
  "messaging_unreadable",
  "messaging_too_new",
  "messaging_storage",
  "messaging_entropy",
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
  return trimmed.startsWith("did:") || trimmed.includes("_oob=") || trimmed.startsWith("{");
}

export function openRelationship(invitation: string, mediator: string): Promise<Relationship> {
  return invoke<Relationship>("messaging_open", { invitation, mediator });
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
 * The book, as the screens see it. `open` is the flag that says an identity is
 * open: the book is readable exactly then, and asking earlier is asking for
 * `messaging_locked`.
 */
export function useMessaging(open: boolean): Messaging {
  const [book, setBook] = useState<Book>(emptyBook);
  const [read, setRead] = useState(false);
  const [collecting, setCollecting] = useState(false);

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
    setCollecting(true);
    try {
      const collected = await invoke<Collected>("messaging_collect");
      await refresh();
      return collected;
    } finally {
      setCollecting(false);
    }
  }, [refresh]);

  const markRead = useCallback(async (id: string) => {
    await invoke<void>("messaging_mark_read", { id });
    setBook((current) => ({
      ...current,
      messages: current.messages.map((m) => (m.id === id ? { ...m, read: true } : m)),
    }));
  }, []);

  return { book, read, collecting, refresh, collect, markRead };
}
