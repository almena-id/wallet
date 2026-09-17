import { useEffect, useState } from "react";

import { BellIcon, CheckIcon } from "../components/icons";
import { plural, useI18n } from "../i18n";
import {
  errorCode,
  looksLikeInvitation,
  messageKind,
  openRelationship,
  readInvitation,
  relationshipName,
  type Invitation,
  type Messaging,
  type Received,
  type Relationship,
} from "../messaging";

type MessagesScreenProps = {
  messaging: Messaging;
  /** The mediator a new relationship's mailbox is opened at. */
  mediator: string;
  /** Something scanned on the way here, to be read as an invitation. */
  initialInvitation: string | null;
};

/**
 * The relationships, and what came through them.
 *
 * Three things on one screen, in the order somebody arrives at them: a place
 * to put an invitation and see who it is from before opening it; the button
 * that empties every mailbox; and the relationships themselves, each with
 * its messages under it.
 *
 * **Nothing here opens anything on its own.** An invitation that was scanned
 * arrives in the field, read and named, and it is the person who presses the
 * button — the screen says who the relationship would be with, and that is
 * the whole of what it says about it.
 */
export function MessagesScreen({ messaging, mediator, initialInvitation }: MessagesScreenProps) {
  const { t, locale } = useI18n();
  const [invitation, setInvitation] = useState(initialInvitation ?? "");
  const [preview, setPreview] = useState<Invitation | null>(null);
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState<string | null>(null);
  const [collectNote, setCollectNote] = useState<string | null>(null);
  const [collectError, setCollectError] = useState<string | null>(null);

  // Read as it is typed, so the card can say who it is from before anybody
  // opens it. What does not read as an invitation is shown as nobody, and the
  // button stays off; the error is for the press, not the keystroke.
  useEffect(() => {
    let current = true;
    if (!looksLikeInvitation(invitation)) {
      setPreview(null);
      return;
    }
    readInvitation(invitation)
      .then((read) => {
        if (current) {
          setPreview(read);
        }
      })
      .catch(() => {
        if (current) {
          setPreview(null);
        }
      });
    return () => {
      current = false;
    };
  }, [invitation]);

  async function open() {
    setOpening(true);
    setOpenError(null);
    try {
      await openRelationship(invitation, mediator);
      setInvitation("");
      setPreview(null);
      await messaging.refresh();
    } catch (failure) {
      setOpenError(t.messages.errors[errorCode(failure)]);
    } finally {
      setOpening(false);
    }
  }

  async function collect() {
    setCollectNote(null);
    setCollectError(null);
    try {
      const collected = await messaging.collect();
      const parts = [
        collected.received === 0
          ? t.messages.collect.none
          : plural(t.messages.collect.received, collected.received, locale),
      ];
      if (collected.unreachable.length > 0) {
        parts.push(plural(t.messages.collect.unreachable, collected.unreachable.length, locale));
      }
      setCollectNote(parts.join(" · "));
    } catch (failure) {
      setCollectError(t.messages.errors[errorCode(failure)]);
    }
  }

  const { relationships, messages } = messaging.book;

  return (
    <div className="screen">
      <header className="screen__header">
        <h1 className="screen__title">{t.messages.title}</h1>
      </header>

      <section className="card" aria-labelledby="messages-open">
        <h2 className="card__title" id="messages-open">
          {t.messages.open.title}
        </h2>
        <label className="field">
          <span className="field__label">{t.messages.open.invitation}</span>
          <textarea
            className="field__input field__input--phrase"
            rows={3}
            value={invitation}
            onChange={(event) => setInvitation(event.target.value)}
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
          />
        </label>
        {preview ? (
          <>
            <p className="card__subtitle">{t.messages.open.with}</p>
            <p className="identifier">
              {preview.label ? `${preview.label} — ` : ""}
              {preview.counterparty}
            </p>
          </>
        ) : null}
        {openError ? <p className="field__error">{openError}</p> : null}
        <div className="button-row">
          <button
            type="button"
            className="button button--primary"
            disabled={preview === null || opening}
            onClick={() => void open()}
          >
            {opening ? t.messages.open.opening : t.messages.open.action}
          </button>
        </div>
      </section>

      <section className="card" aria-labelledby="messages-collect">
        <h2 className="card__title" id="messages-collect">
          {t.messages.collect.title}
        </h2>
        {collectNote ? <p className="card__note">{collectNote}</p> : null}
        {collectError ? <p className="field__error">{collectError}</p> : null}
        <div className="button-row">
          <button
            type="button"
            className="button button--primary button--icon"
            disabled={messaging.collecting || relationships.length === 0}
            onClick={() => void collect()}
          >
            <BellIcon />
            {messaging.collecting ? t.messages.collect.checking : t.messages.collect.action}
          </button>
        </div>
      </section>

      {messaging.read && relationships.length === 0 ? (
        <section className="card">
          <div className="empty-state">
            <p className="empty-state__title">{t.messages.relationships.empty}</p>
          </div>
        </section>
      ) : null}

      {relationships.map((relationship) => (
        <RelationshipCard
          key={relationship.counterparty}
          relationship={relationship}
          messages={messages.filter((m) => m.counterparty === relationship.counterparty)}
          onOpen={(id) => void messaging.markRead(id)}
        />
      ))}
    </div>
  );
}

type RelationshipCardProps = {
  relationship: Relationship;
  messages: Received[];
  onOpen: (id: string) => void;
};

function RelationshipCard({ relationship, messages, onOpen }: RelationshipCardProps) {
  const { t, locale } = useI18n();
  const heading = `relationship-${relationship.pairwise}`;
  // Newest first: what came last is what somebody came to see.
  const ordered = [...messages].sort((a, b) => (b.createdTime ?? 0) - (a.createdTime ?? 0));

  return (
    <section className="card" aria-labelledby={heading}>
      <h2 className="card__title" id={heading}>
        {relationshipName(relationship)}
      </h2>
      {relationship.label ? (
        <p className="row__hint row__hint--identifier">{relationship.counterparty}</p>
      ) : null}
      <p className="card__subtitle">{t.messages.relationships.pairwise}</p>
      <p className="identifier">{relationship.pairwise}</p>

      {ordered.length === 0 ? (
        <p className="card__subtitle">{t.messages.list.empty}</p>
      ) : (
        <div className="options">
          {ordered.map((message) => (
            <MessageRow key={message.id} message={message} locale={locale} onOpen={onOpen} />
          ))}
        </div>
      )}
    </section>
  );
}

type MessageRowProps = {
  message: Received;
  locale: string;
  onOpen: (id: string) => void;
};

/**
 * One message: what it is, when it was written, and — once opened — what it
 * says. The body is shown as the JSON it is: this wallet does not yet speak
 * the protocols that would render it as anything else, and a shape it does
 * not understand is better shown than hidden.
 */
function MessageRow({ message, locale, onOpen }: MessageRowProps) {
  const [shown, setShown] = useState(false);
  const when =
    message.createdTime === null
      ? null
      : new Intl.DateTimeFormat(locale, {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(new Date(message.createdTime * 1000));
  const hint = [when, message.from].filter((part) => part !== null).join(" · ");

  return (
    <div className="message">
      <button
        type="button"
        className="row"
        aria-expanded={shown}
        onClick={() => {
          setShown((open) => !open);
          if (!message.read) {
            onOpen(message.id);
          }
        }}
      >
        <span className="row__text">
          <span className={message.read ? "row__label" : "row__label message__label--unread"}>
            {messageKind(message.type)}
          </span>
          {hint ? <span className="row__hint row__hint--identifier">{hint}</span> : null}
        </span>
        {message.read ? <CheckIcon className="row__check" /> : null}
      </button>
      {shown ? (
        <pre className="identifier message__body">{JSON.stringify(message.body, null, 2)}</pre>
      ) : null}
    </div>
  );
}
