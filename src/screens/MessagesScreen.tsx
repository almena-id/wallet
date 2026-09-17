import { useState } from "react";

import { ChevronLeftIcon, PlusIcon, SyncIcon } from "../components/icons";
import { plural, useI18n } from "../i18n";
import {
  errorCode,
  relationshipName,
  requestsOf,
  threadName,
  type Messaging,
  type Relationship,
  type Thread,
} from "../messaging";

type MessagesScreenProps = {
  messaging: Messaging;
  /** Opens a thread, by its key. */
  onOpenThread: (key: string) => void;
  /** Where a new relationship is opened from an invitation. */
  onNewRelationship: () => void;
};

/**
 * The inbox: the credentials this wallet has asked for, one row each —
 * a request and everything said about it — across every relationship,
 * newest activity first.
 *
 * **A mailbox, not a chat.** What arrives here is what an issuer sent
 * back about a request, and the holder reads it; the wallet does not
 * write from this screen — what it sent, it sent from the screen that
 * asked. A row says which credential, from whom and when it last moved,
 * and leads to the request itself. Unread is said on the row, and stops
 * being said once the request has been opened.
 *
 * The mailboxes are emptied on a clock while the wallet is open, and on
 * demand from the button at the top: a wallet on a phone is not listening,
 * it asks, and the note under the title says what the asking brought.
 */
export function MessagesScreen({
  messaging,
  onOpenThread,
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

  const { relationships } = messaging.book;
  const byCounterparty = new Map(relationships.map((r) => [r.counterparty, r]));
  const threads = requestsOf(messaging.book);

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

      {messaging.read && threads.length === 0 ? (
        <section className="card">
          <div className="empty-state">
            <p className="empty-state__title">{t.messages.inbox.empty}</p>
          </div>
        </section>
      ) : null}

      {threads.length > 0 ? (
        <div className="options">
          {threads.map((thread) => (
            <ThreadRow
              key={thread.key}
              thread={thread}
              relationship={byCounterparty.get(thread.counterparty) ?? null}
              onOpen={() => onOpenThread(thread.key)}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

type ThreadRowProps = {
  thread: Thread;
  /** Null for a thread whose relationship is no longer in the book. */
  relationship: Relationship | null;
  onOpen: () => void;
};

/**
 * One request in the inbox: the credential it asked for, whom it asked,
 * and when it last moved.
 *
 * The messages are not here. A row is for deciding whether to open the
 * request, and the credential's name — kept with the request as the
 * person authorised it — is what there is to decide on.
 */
function ThreadRow({ thread, relationship, onOpen }: ThreadRowProps) {
  const { t, locale } = useI18n();
  const when =
    thread.lastTime === null
      ? null
      : new Intl.DateTimeFormat(locale, {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(new Date(thread.lastTime * 1000));
  const hint = [relationship ? relationshipName(relationship) : thread.counterparty, when]
    .filter((part) => part !== null)
    .join(" · ");

  return (
    <button
      type="button"
      className={thread.unread === 0 ? "row" : "row message-row--unread"}
      onClick={onOpen}
    >
      <span className="row__text">
        <span className="row__label">{threadName(thread)}</span>
        <span className="row__hint">{hint}</span>
      </span>
      {thread.unread === 0 ? null : (
        <span className="message-row__dot" role="img" aria-label={t.messages.inbox.unread} />
      )}
      <ChevronLeftIcon className="row__chevron" />
    </button>
  );
}
