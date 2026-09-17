import { useEffect } from "react";

import { ChevronLeftIcon } from "../components/icons";
import { useI18n, useTranslations } from "../i18n";
import {
  messageKind,
  relationshipName,
  type Messaging,
  type Entry,
  type Relationship,
} from "../messaging";

type ConversationScreenProps = {
  messaging: Messaging;
  relationship: Relationship;
  /** Back to the inbox. */
  onBack: () => void;
};

/**
 * One relationship, and everything that came through it, oldest first.
 *
 * Opening the conversation is reading it: whatever was unread in it is
 * marked read on arrival, so the inbox stops saying so. The marking goes
 * message by message because the book records it that way, and a message
 * that arrives while the screen is open is marked with the rest.
 *
 * Above the messages, the two identifiers the relationship is made of — the
 * counterparty's, and the one this wallet is in it. Both are identifiers,
 * shown as such and not read out.
 */
export function ConversationScreen({ messaging, relationship, onBack }: ConversationScreenProps) {
  const { t, locale } = useI18n();
  const messages = messaging.book.messages
    .filter((m) => m.counterparty === relationship.counterparty)
    .sort((a, b) => (a.createdTime ?? 0) - (b.createdTime ?? 0));

  // The ids as one string, so a render with the same unread messages does
  // not mark them again.
  const unread = messages
    .filter((m) => !m.read)
    .map((m) => m.id)
    .join("\n");
  const { markRead } = messaging;
  useEffect(() => {
    for (const id of unread.split("\n").filter((id) => id.length > 0)) {
      void markRead(id);
    }
  }, [unread, markRead]);

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">{relationshipName(relationship)}</h1>
      </header>

      <section className="card">
        {relationship.label ? <p className="identifier">{relationship.counterparty}</p> : null}
        <p className="card__subtitle">{t.messages.conversation.pairwise}</p>
        <p className="identifier">{relationship.pairwise}</p>
      </section>

      {messages.map((message) => (
        <Message key={message.id} message={message} locale={locale} />
      ))}
    </div>
  );
}

type MessageProps = {
  message: Entry;
  locale: string;
};

/**
 * One message, whole: what it is, when it was written, who the envelope said
 * it was from, and what it says. The body is shown as the JSON it is: this
 * wallet does not yet speak the protocols that would render it as anything
 * else, and a shape it does not understand is better shown than hidden.
 * What this wallet sent is set apart, and says so.
 */
function Message({ message, locale }: MessageProps) {
  const t = useTranslations();
  const when =
    message.createdTime === null
      ? null
      : new Intl.DateTimeFormat(locale, {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(new Date(message.createdTime * 1000));
  const hint = [message.sent ? t.messages.inbox.sent : null, when, message.from]
    .filter((part) => part !== null)
    .join(" · ");

  return (
    <section className={message.sent ? "card message message--sent" : "card message"}>
      <h2 className="card__title">{messageKind(message.type)}</h2>
      {hint ? <p className="row__hint row__hint--identifier">{hint}</p> : null}
      <pre className="identifier message__body">{JSON.stringify(message.body, null, 2)}</pre>
    </section>
  );
}
