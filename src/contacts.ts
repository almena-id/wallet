import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * The relationships this wallet has and what is said in them. Everything
 * behind these calls happens on the Rust side — see
 * `src-tauri/src/messaging` — and no DID crosses to this side: a contact is
 * named by an alias, the name they gave, or a short fingerprint.
 */
export type Contact = {
  /** The conversation's id: a stable key, and what the commands take. */
  id: string;
  /** What it is called: the alias, else their name, else the fingerprint. */
  name: string;
  /** The name this wallet gave it. */
  alias: string | null;
  /** The name they gave themselves. */
  theirName: string | null;
  /** A short fingerprint of the DID the relationship was opened with. */
  fingerprint: string;
  /** Waiting for the other side's first answer. */
  pending: boolean;
  /** When it was opened, in seconds since the epoch. */
  since: number;
  unread: number;
  last: Last | null;
  /** Its history was deleted; the inbox leaves it out until it has news. */
  cleared: boolean;
};

/** The latest message of a conversation. */
export type Last = {
  content: string;
  at: number;
  mine: boolean;
  /** An issuer's notice: its status, worded here. */
  notice?: NoticeStatus;
};

/** How an application stands, as its issuer tells it (`messaging/notice.rs`). */
export type NoticeStatus = "accepted" | "rejected" | "issued";

/** An issuer's notice about an application this wallet sent it. */
export type Notice = {
  application: string;
  status: NoticeStatus;
  credentialType: string;
  /** The credential's name, by language. */
  credentialName: Record<string, string>;
  issuer: string;
  note?: string;
  /** Issued: where this wallet asks to receive it. */
  collect?: string;
};

/** One message, either way. */
export type Entry = {
  id: string;
  mine: boolean;
  content: string;
  /** When it was written, in seconds since the epoch. */
  at: number;
  /** Sent by this wallet and not delivered; it can be retried. */
  failed: boolean;
  /** An issuer's notice, drawn from this rather than from `content`. */
  notice?: Notice;
};

export type Conversation = {
  contact: Contact;
  /** Oldest first. */
  entries: Entry[];
};

export type Synced = {
  /** Relationships and messages changed by what arrived. */
  changed: number;
  contacts: Contact[];
};

export type Profile = {
  /** The name this wallet goes by. */
  name: string | null;
  /** Contacts a changed name could not be sent to. */
  unreached: number;
};

/** The longest message, in characters, as the Rust side counts them. */
export const MESSAGE_CHARS = 4000;
/** The longest name. */
export const NAME_CHARS = 64;

export function listContacts(): Promise<Contact[]> {
  return invoke<Contact[]>("contacts_list");
}

/** Picks up what the mediator is holding: new contacts, names and messages. */
export function syncMessages(): Promise<Synced> {
  return invoke<Synced>("messages_sync");
}

/**
 * Calls `handler` whenever something arrived live and changed the contacts or
 * a conversation. Returns the function that stops listening.
 */
export function onMessagesChanged(handler: (synced: Synced) => void): () => void {
  const stopped = listen<Synced>("messaging-changed", (event) => handler(event.payload));
  return () => {
    void stopped.then((stop) => stop()).catch(() => undefined);
  };
}

/** This wallet's own invitation link, with its contact card registered. */
export function showInvitation(): Promise<string> {
  return invoke<{ url: string }>("invitation_show").then((shown) => shown.url);
}

/** Accepts somebody's invitation link. */
export function acceptInvitation(input: string): Promise<Contact> {
  return invoke<Contact>("contact_accept", { input });
}

export function readConversation(id: string): Promise<Conversation> {
  return invoke<Conversation>("conversation_read", { id });
}

/** Marks a conversation as read. */
export function markSeen(id: string): Promise<void> {
  return invoke<void>("conversation_seen", { id });
}

/** Deletes a conversation's history. The contact stays. */
export function clearConversation(id: string): Promise<void> {
  return invoke<void>("conversation_clear", { id });
}

/** Sends a message; one that did not go comes back marked `failed`. */
export function sendMessage(id: string, content: string): Promise<Entry> {
  return invoke<Entry>("message_send", { id, content });
}

/**
 * Asks the issuer of an `issued` notice for the credential: the answer is an
 * `almena://` link to its `receive` request, put to the person like any other.
 */
export function collectCredential(contact: string, entry: string): Promise<string> {
  return invoke<string>("registry_collect", { contact, entry });
}

export function retryMessage(id: string, message: string): Promise<Entry> {
  return invoke<Entry>("message_retry", { id, message });
}

/** Gives a contact a name of this wallet's own; `null` takes it away. */
export function renameContact(id: string, alias: string | null): Promise<Contact> {
  return invoke<Contact>("contact_rename", { id, alias });
}

export function readProfile(): Promise<Profile> {
  return invoke<Profile>("profile_read");
}

/** Changes the name this wallet goes by and sends it to every contact. */
export function writeProfile(name: string | null): Promise<Profile> {
  return invoke<Profile>("profile_write", { name });
}

/** The picture this wallet shows for its own identity, as a `data:` URL. */
export function readPhoto(): Promise<string | null> {
  return invoke<string | null>("profile_photo_read");
}

/** Keeps a picture made by `shrinkPhoto`, or removes it with `null`. */
export function writePhoto(photo: string | null): Promise<void> {
  return invoke<void>("profile_photo_write", { photo });
}
