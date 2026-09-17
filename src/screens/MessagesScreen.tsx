import { useState } from "react";

import { ChevronLeftIcon, PlusIcon, SyncIcon } from "../components/icons";
import { plural, useI18n } from "../i18n";
import {
  errorCode,
  messageKind,
  relationshipName,
  type Messaging,
  type Entry,
  type Relationship,
} from "../messaging";

type MessagesScreenProps = {
  messaging: Messaging;
  /** Opens the conversation a message belongs to. */
  onOpenConversation: (counterparty: string) => void;
  /** Where a new relationship is opened from an invitation. */
  onNewRelationship: () => void;
};

/**
 * The inbox: every message of every relationship, in one list, newest
 * first — what came through, and what this wallet sent.
 *
 * **A mailbox, not a chat.** What arrives here is what an entity sent to
 * this holder, and the holder reads it; the wallet does not write back from
 * this screen — what it sent, it sent from the screen that asked. So the
 * list is flat — one row per message, with who it is with and when — and a
 * row leads to the conversation it is part of. Unread is said on the row,
 * and stops being said once the conversation has been opened.
 *
 * The mailboxes are emptied on a clock while the wallet is open, and on
 * demand from the button at the top: a wallet on a phone is not listening,
 * it asks, and the note under the title says what the asking brought.
 */
export function MessagesScreen({
  messaging,
  onOpenConversation,
  onNewRelationship,
}: MessagesScreenProps) {
  const { t, locale } = useI18n();
  const [syncNote, setSyncNote] = useState<string | null>(null);
  const [syncError, setSyncError] = useState<string | null>(null);

  async function sync() {
    setSyncNote(null);
    setSyncError(null);
    try {
      const collected = await messaging.collect();
      const parts = [
        collected.received === 0
          ? t.messages.sync.none
          : plural(t.messages.sync.received, collected.received, locale),
      ];
      if (collected.unreachable.length > 0) {
        parts.push(plural(t.messages.sync.unreachable, collected.unreachable.length, locale));
      }
      setSyncNote(parts.join(" · "));
    } catch (failure) {
      setSyncError(t.messages.errors[errorCode(failure)]);
    }
  }

  const { relationships, messages } = messaging.book;
  const byCounterparty = new Map(relationships.map((r) => [r.counterparty, r]));
  // Newest first: what came last is what somebody came to see.
  const ordered = [...messages].sort((a, b) => (b.createdTime ?? 0) - (a.createdTime ?? 0));

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <h1 className="screen__title screen__title--compact screen__title--grow">
          {t.messages.title}
        </h1>
        <button
          type="button"
          className="icon-button"
          onClick={() => void sync()}
          disabled={messaging.collecting || relationships.length === 0}
          aria-label={messaging.collecting ? t.messages.sync.syncing : t.messages.sync.action}
          aria-busy={messaging.collecting}
        >
          <SyncIcon className={messaging.collecting ? "icon-button__spin" : undefined} />
        </button>
        <button
          type="button"
          className="icon-button"
          onClick={onNewRelationship}
          aria-label={t.messages.open.title}
        >
          <PlusIcon />
        </button>
      </header>

      {syncNote ? <p className="card__note">{syncNote}</p> : null}
      {syncError ? <p className="field__error">{syncError}</p> : null}

      {messaging.read && ordered.length === 0 ? (
        <section className="card">
          <div className="empty-state">
            <p className="empty-state__title">{t.messages.inbox.empty}</p>
          </div>
        </section>
      ) : null}

      {ordered.length > 0 ? (
        <div className="options">
          {ordered.map((message) => (
            <InboxRow
              key={message.id}
              message={message}
              relationship={byCounterparty.get(message.counterparty) ?? null}
              onOpen={() => onOpenConversation(message.counterparty)}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

type InboxRowProps = {
  message: Entry;
  /** Null for a message whose relationship is no longer in the book. */
  relationship: Relationship | null;
  onOpen: () => void;
};

/**
 * One message in the inbox: who it came through, what kind it is, and when
 * — and, for one this wallet sent, that it did.
 *
 * The body is not here. A row is for deciding whether to open the
 * conversation, and what the protocol calls the message is what there is to
 * decide on until this wallet renders the protocols themselves.
 */
function InboxRow({ message, relationship, onOpen }: InboxRowProps) {
  const { t, locale } = useI18n();
  const when =
    message.createdTime === null
      ? null
      : new Intl.DateTimeFormat(locale, {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(new Date(message.createdTime * 1000));
  const hint = [message.sent ? t.messages.inbox.sent : null, messageKind(message.type), when]
    .filter((part) => part !== null)
    .join(" · ");

  return (
    <button
      type="button"
      className={message.read ? "row" : "row message-row--unread"}
      onClick={onOpen}
    >
      <span className="row__text">
        <span className="row__label">
          {relationship ? relationshipName(relationship) : message.counterparty}
        </span>
        <span className="row__hint">{hint}</span>
      </span>
      {message.read ? null : (
        <span className="message-row__dot" role="img" aria-label={t.messages.inbox.unread} />
      )}
      <ChevronLeftIcon className="row__chevron" />
    </button>
  );
}
