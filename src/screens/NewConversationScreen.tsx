import { useEffect, useState } from "react";

import { useI18n } from "../i18n";
import { fill } from "../i18n/format";
import { listContacts, onMessagesChanged, syncMessages, type Contact } from "../contacts";
import { errorCode } from "../mediator";
import { initial } from "./MessagesScreen";
import { BackButton } from "../components/BackButton";

type NewConversationScreenProps = {
  onBack: () => void;
  /** Opens the conversation with a contact. */
  onOpen: (id: string) => void;
};

/**
 * Where a conversation starts: with somebody this wallet already has a
 * relationship with.
 *
 * It syncs as it opens, so a relationship somebody opened by accepting this
 * wallet's invitation is in the list by the time it is drawn.
 */
export function NewConversationScreen({ onBack, onOpen }: NewConversationScreenProps) {
  const { t, locale } = useI18n();
  const [contacts, setContacts] = useState<Contact[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    listContacts()
      .then((listed) => current && setContacts(listed))
      .catch(() => undefined);
    syncMessages()
      .then((synced) => current && setContacts(synced.contacts))
      .catch((failure) => current && setError(t.messaging.errors[errorCode(failure)]));
    return () => {
      current = false;
    };
  }, [t]);

  // A contact who accepts this wallet's invitation while it is open appears.
  useEffect(() => onMessagesChanged((synced) => setContacts(synced.contacts)), []);

  const date = new Intl.DateTimeFormat(locale, { dateStyle: "medium" });

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <BackButton onBack={onBack} />
        <h1 className="screen__title screen__title--compact">{t.conversations.new.title}</h1>
      </header>

      {error ? <p className="card__note card__note--warning">{error}</p> : null}

      <section className="card" aria-labelledby="new-contacts">
        <h2 className="card__title" id="new-contacts">
          {t.conversations.new.contacts}
        </h2>
        {contacts === null ? null : contacts.length === 0 ? (
          <p className="card__body">{t.conversations.new.empty}</p>
        ) : (
          contacts.map((contact) => (
            <button
              type="button"
              className="row"
              key={contact.id}
              onClick={() => onOpen(contact.id)}
            >
              <span className="row__icon row__icon--initial" aria-hidden="true">
                {initial(contact.name)}
              </span>
              <span className="row__text">
                <span className="row__label">{contact.name}</span>
                <span className="row__hint">
                  {contact.pending
                    ? t.conversations.new.pending
                    : fill(t.conversations.new.since, {
                        date: date.format(new Date(contact.since * 1000)),
                      })}
                </span>
              </span>
            </button>
          ))
        )}
      </section>

    </div>
  );
}
