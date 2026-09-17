import { useEffect } from "react";

import { ChevronLeftIcon } from "../components/icons";
import { useI18n } from "../i18n";
import {
  relationshipName,
  threadName,
  type Entry,
  type Messaging,
  type Relationship,
  type Thread,
} from "../messaging";

type ThreadScreenProps = {
  messaging: Messaging;
  thread: Thread;
  relationship: Relationship;
  /** Back to the inbox. */
  onBack: () => void;
};

/**
 * One request, whole: whom it went to and when, and then each turn of it,
 * oldest first — what this wallet sent, with the answers as the person
 * authorised them, and whatever the issuer said back.
 *
 * Opening the request is reading it: whatever was unread in it is marked
 * read on arrival, so the inbox stops saying so. The marking goes message
 * by message because the book records it that way, and a message that
 * arrives while the screen is open is marked with the rest.
 *
 * The header names the issuer the request went to and when it was sent;
 * nothing of how it travelled is here — not the pairwise, not the
 * thread's ids — because none of it is what the person asked, or was
 * answered.
 */
export function ThreadScreen({ messaging, thread, relationship, onBack }: ThreadScreenProps) {
  const { t, locale } = useI18n();

  // The ids as one string, so a render with the same unread messages does
  // not mark them again.
  const unread = thread.messages
    .filter((m) => !m.read)
    .map((m) => m.id)
    .join("\n");
  const { markRead } = messaging;
  useEffect(() => {
    for (const id of unread.split("\n").filter((id) => id.length > 0)) {
      void markRead(id);
    }
  }, [unread, markRead]);

  const sentAt = thread.request?.createdTime ?? thread.messages[0]?.createdTime ?? null;

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">{threadName(thread)}</h1>
      </header>

      <section className="card">
        <p className="card__subtitle">{t.messages.thread.to}</p>
        <p className="card__title">{relationshipName(relationship)}</p>
        {relationship.label ? <p className="identifier">{relationship.counterparty}</p> : null}
        {sentAt === null ? null : (
          <>
            <p className="card__subtitle">{t.messages.thread.sentAt}</p>
            <p className="card__body">{when(sentAt, locale)}</p>
          </>
        )}
      </section>

      {thread.messages.map((message) => (
        <Turn key={message.id} message={message} locale={locale} />
      ))}
    </div>
  );
}

/** A message's time, for the active locale. */
function when(seconds: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(seconds * 1000));
}

type TurnProps = {
  message: Entry;
  locale: string;
};

/**
 * One turn of the request: what this wallet sent, or what the issuer
 * answered, with when it was written.
 *
 * What was sent is shown as the person authorised it — each answer under
 * the label it was asked with — and, for a request written before the
 * wallet kept that, by key. What came back is shown as the JSON it is,
 * because this wallet does not yet speak the protocols that would render
 * it as anything else, and a shape it does not understand is better shown
 * than hidden.
 */
function Turn({ message, locale }: TurnProps) {
  const { t } = useI18n();
  const fields = message.sent ? answersOf(message) : null;

  return (
    <section className={message.sent ? "card message message--sent" : "card message"}>
      <h2 className="card__title">
        {message.sent ? t.messages.thread.request : t.messages.thread.reply}
      </h2>
      {message.createdTime === null ? null : (
        <p className="row__hint">{when(message.createdTime, locale)}</p>
      )}
      {fields ? (
        <dl className="summary">
          {fields.map(([label, value]) => (
            <div key={label} className="summary__row">
              <dt className="summary__label">{label}</dt>
              <dd className="summary__value">{value}</dd>
            </div>
          ))}
        </dl>
      ) : (
        <pre className="identifier message__body">{JSON.stringify(message.body, null, 2)}</pre>
      )}
    </section>
  );
}

/**
 * The answers a sent request carries, labelled: from the summary the
 * person authorised when there is one, else from the wire body by key.
 */
function answersOf(message: Entry): [string, string][] | null {
  if (message.summary) {
    return message.summary.fields.map((field) => [field.label, field.value]);
  }
  const fields = message.body.fields;
  if (typeof fields !== "object" || fields === null) {
    return null;
  }
  return Object.entries(fields as Record<string, unknown>).map(([key, value]) => [
    key,
    typeof value === "string" ? value : JSON.stringify(value),
  ]);
}
