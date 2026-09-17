import { useEffect, useState } from "react";

import { ChevronLeftIcon } from "../components/icons";
import { useTranslations } from "../i18n";
import {
  asksForCredential,
  errorCode,
  openRelationship,
  readInvitation,
  type Invitation,
  type Messaging,
} from "../messaging";

type RequestStartScreenProps = {
  messaging: Messaging;
  /** The mediator the relationship with the issuer is opened at. */
  mediator: string;
  /** The marketplace's first code, as it was scanned. */
  code: string;
  /** Out without starting anything. */
  onCancel: () => void;
  /** Where to go once the start is authorised: back to the camera, for the second code. */
  onContinued: () => void;
};

/**
 * The marketplace's first code, put to the person as what it is: the start
 * of a request for a credential, from the issuer it names.
 *
 * The spec's "authorises the start of the request". What the screen shows is
 * what that authorisation is about — which issuer the request would go to —
 * and nothing of how it travels: not the invitation the code carries, not
 * the issuer's identifier, not the relationship the wallet opens underneath.
 * Those are the wallet's business, and `messaging_open` does them when the
 * button is pressed; the page that showed the code moves on to the form, and
 * the person comes back to the camera for the second one.
 *
 * A code that reads as an invitation but not as the marketplace's is not
 * this screen's: the scanner sends it to the relationship screen instead.
 */
export function RequestStartScreen({
  messaging,
  mediator,
  code,
  onCancel,
  onContinued,
}: RequestStartScreenProps) {
  const t = useTranslations();
  const [invitation, setInvitation] = useState<Invitation | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    readInvitation(code)
      .then((read) => {
        if (!current) {
          return;
        }
        if (asksForCredential(read)) {
          setInvitation(read);
        } else {
          setReadError(t.messages.errors.messaging_request_unreadable);
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

  async function start() {
    setOpening(true);
    setOpenError(null);
    try {
      await openRelationship(code, mediator);
      await messaging.refresh();
      onContinued();
    } catch (failure) {
      setOpenError(t.messages.errors[errorCode(failure)]);
      setOpening(false);
    }
  }

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <button type="button" className="icon-button" onClick={onCancel} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">{t.messages.request.title}</h1>
      </header>

      {readError ? (
        <section className="card">
          <p className="field__error">{readError}</p>
          <div className="button-row">
            <button type="button" className="button" onClick={onCancel}>
              {t.messages.request.cancel}
            </button>
          </div>
        </section>
      ) : null}

      {invitation ? (
        <section className="card">
          <dl className="summary">
            <div className="summary__row">
              <dt className="summary__label">{t.messages.request.issuer}</dt>
              {/* The platform's invitation always names the issuer; one that
                  does not is shown by the only thing that says who it is. */}
              <dd className="summary__value">{invitation.label ?? invitation.counterparty}</dd>
            </div>
          </dl>
          {openError ? <p className="field__error">{openError}</p> : null}
          <div className="button-row">
            <button
              type="button"
              className="button button--primary"
              disabled={opening}
              onClick={() => void start()}
            >
              {opening ? t.messages.request.continuing : t.messages.request.continue}
            </button>
            <button type="button" className="button" onClick={onCancel} disabled={opening}>
              {t.messages.request.cancel}
            </button>
          </div>
        </section>
      ) : null}
    </div>
  );
}
