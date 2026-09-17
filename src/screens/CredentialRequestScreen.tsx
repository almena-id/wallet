import { useEffect, useState } from "react";

import { ChevronLeftIcon } from "../components/icons";
import { fill, useTranslations } from "../i18n";
import {
  errorCode,
  readRequest,
  sendRequest,
  type CredentialRequest,
  type Messaging,
  type Sent,
} from "../messaging";

type CredentialRequestScreenProps = {
  messaging: Messaging;
  /** The mediator a relationship is opened at, if this wallet has none with the issuer. */
  mediator: string;
  /** The second code, as it was scanned. */
  code: string;
  /** Back to where the code was scanned from, whether or not it was sent. */
  onBack: () => void;
  /** Where to go once it is sent: the thread it is now part of. */
  onSent: (sent: Sent) => void;
};

/**
 * What the second code says the request would be, for the person to send
 * — or not.
 *
 * The spec's authorisation of the send: the wallet shows the summary of
 * what is about to go, and nothing goes until the button is pressed. The
 * summary is the code's own — the issuer, the credential, every answer
 * under the label it was asked with — so what is shown is exactly what
 * the portal made of what the person typed there.
 */
export function CredentialRequestScreen({
  messaging,
  mediator,
  code,
  onBack,
  onSent,
}: CredentialRequestScreenProps) {
  const t = useTranslations();
  const [request, setRequest] = useState<CredentialRequest | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    readRequest(code)
      .then((read) => {
        if (current) {
          setRequest(read);
        }
      })
      .catch((failure) => {
        if (current) {
          setReadError(t.messages.errors[errorCode(failure)]);
        }
      });
    return () => {
      current = false;
    };
  }, [code, t]);

  async function send() {
    setSending(true);
    setSendError(null);
    try {
      const sent = await sendRequest(code, mediator);
      await messaging.refresh();
      onSent(sent);
    } catch (failure) {
      setSendError(t.messages.errors[errorCode(failure)]);
      setSending(false);
    }
  }

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">{t.messages.request.title}</h1>
      </header>

      {readError ? (
        <section className="card">
          <p className="field__error">{readError}</p>
        </section>
      ) : null}

      {request ? (
        <>
          <section className="card">
            <h2 className="card__title">{request.credential}</h2>
            <p className="card__subtitle">
              {fill(t.messages.request.version, { version: request.template.version })}
            </p>
            <p className="card__subtitle">{t.messages.request.to}</p>
            <p className="identifier">
              {request.issuerName} — {request.issuer}
            </p>
          </section>

          <section className="card">
            <dl className="summary">
              {request.fields.map((field) => (
                <div key={field.key} className="summary__row">
                  <dt className="summary__label">{field.label}</dt>
                  <dd className="summary__value">{field.value}</dd>
                </div>
              ))}
            </dl>
            {sendError ? <p className="field__error">{sendError}</p> : null}
            <div className="button-row">
              <button
                type="button"
                className="button button--primary"
                disabled={sending}
                onClick={() => void send()}
              >
                {sending ? t.messages.request.sending : t.messages.request.send}
              </button>
              <button type="button" className="button" onClick={onBack} disabled={sending}>
                {t.messages.request.cancel}
              </button>
            </div>
          </section>
        </>
      ) : null}
    </div>
  );
}
