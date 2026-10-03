import { useEffect, useState } from "react";

import { useI18n } from "../i18n";
import { fill } from "../i18n/format";
import { NAME_CHARS, readConversation, renameContact, type Contact } from "../contacts";
import { errorCode } from "../mediator";
import { BackButton } from "../components/BackButton";

type ContactScreenProps = {
  id: string;
  onBack: () => void;
};

/**
 * A contact: the name they gave, the fingerprint that tells two of the same
 * name apart, and the name this wallet calls them by — which stays on this
 * device and wins over theirs.
 */
export function ContactScreen({ id, onBack }: ContactScreenProps) {
  const { t, locale } = useI18n();
  const [contact, setContact] = useState<Contact | null>(null);
  const [alias, setAlias] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    readConversation(id)
      .then((read) => {
        setContact(read.contact);
        setAlias(read.contact.alias ?? "");
      })
      .catch((failure) => setError(t.messaging.errors[errorCode(failure)]));
  }, [id, t]);

  async function save(next: string | null) {
    setBusy(true);
    setError(null);
    try {
      const renamed = await renameContact(id, next);
      setContact(renamed);
      setAlias(renamed.alias ?? "");
    } catch (failure) {
      setError(t.messaging.errors[errorCode(failure)]);
    } finally {
      setBusy(false);
    }
  }

  const date = new Intl.DateTimeFormat(locale, { dateStyle: "medium" });
  const changed = contact !== null && alias.trim() !== (contact.alias ?? "");

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <BackButton onBack={onBack} />
        <h1 className="screen__title screen__title--compact">{contact?.name ?? ""}</h1>
      </header>

      {error ? <p className="card__note card__note--warning">{error}</p> : null}

      {contact ? (
        <>
          <section className="card" aria-labelledby="contact-details">
            <h2 className="card__title" id="contact-details">
              {t.conversations.contact.title}
            </h2>
            <dl className="detail-list">
              <div className="detail-list__row">
                <dt>{t.conversations.contact.theirName}</dt>
                <dd>{contact.theirName ?? t.conversations.contact.noName}</dd>
              </div>
              <div className="detail-list__row">
                <dt>{t.conversations.contact.fingerprint}</dt>
                <dd className="detail-list__mono">{contact.fingerprint}</dd>
              </div>
              <div className="detail-list__row">
                <dt>{t.conversations.contact.since}</dt>
                <dd>
                  {contact.pending
                    ? t.conversations.new.pending
                    : fill(t.conversations.new.since, {
                        date: date.format(new Date(contact.since * 1000)),
                      })}
                </dd>
              </div>
            </dl>
          </section>

          <form
            className="card"
            aria-labelledby="contact-alias"
            onSubmit={(event) => {
              event.preventDefault();
              void save(alias.trim() || null);
            }}
          >
            <h2 className="card__title" id="contact-alias">
              {t.conversations.contact.aliasTitle}
            </h2>
            <p className="card__body">{t.conversations.contact.aliasHint}</p>
            <label className="field">
              <span className="field__label">{t.conversations.contact.aliasLabel}</span>
              <input
                className="field__input"
                value={alias}
                onChange={(event) => setAlias(event.target.value)}
                placeholder={contact.theirName ?? contact.fingerprint}
                maxLength={NAME_CHARS}
              />
            </label>
            <div className="button-row">
              <button type="submit" className="button button--primary" disabled={busy || !changed}>
                {t.conversations.contact.save}
              </button>
              {contact.alias ? (
                <button
                  type="button"
                  className="button"
                  onClick={() => void save(null)}
                  disabled={busy}
                >
                  {t.conversations.contact.clear}
                </button>
              ) : null}
            </div>
          </form>
        </>
      ) : null}
    </div>
  );
}
