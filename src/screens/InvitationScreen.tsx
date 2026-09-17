import { useEffect, useState } from "react";

import { ChevronLeftIcon } from "../components/icons";
import { useTranslations } from "../i18n";
import {
  errorCode,
  looksLikeInvitation,
  openRelationship,
  readInvitation,
  type Invitation,
  type Messaging,
} from "../messaging";

type InvitationScreenProps = {
  messaging: Messaging;
  /** The mediator the new relationship's mailbox is opened at. */
  mediator: string;
  /** Something scanned on the way here, to be read as an invitation. */
  initialInvitation: string | null;
  /** Back to the inbox, whether or not a relationship was opened. */
  onBack: () => void;
};

/**
 * A place to put an invitation and see who it is from before opening it.
 *
 * **Nothing here opens anything on its own.** An invitation that was scanned
 * arrives in the field, read and named, and it is the person who presses the
 * button — the screen says who the relationship would be with, and that is
 * the whole of what it says about it. Once it is open, the inbox is where
 * what comes through it will be.
 *
 * The marketplace's invitation is the start of a request, and the button
 * says so: pressing it is the spec's "authorises the start", and the wallet
 * answers the issuer as it opens the relationship — see `messaging_open`.
 */
export function InvitationScreen({
  messaging,
  mediator,
  initialInvitation,
  onBack,
}: InvitationScreenProps) {
  const t = useTranslations();
  const [invitation, setInvitation] = useState(initialInvitation ?? "");
  const [preview, setPreview] = useState<Invitation | null>(null);
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState<string | null>(null);

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
      await messaging.refresh();
      onBack();
    } catch (failure) {
      setOpenError(t.messages.errors[errorCode(failure)]);
      setOpening(false);
    }
  }

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">{t.messages.open.title}</h1>
      </header>

      <section className="card">
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
            {opening
              ? t.messages.open.opening
              : preview?.goalCode === "issue-vc"
                ? t.messages.open.accept
                : t.messages.open.action}
          </button>
        </div>
      </section>
    </div>
  );
}
