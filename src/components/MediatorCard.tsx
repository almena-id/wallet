import { useState } from "react";

import { useI18n } from "../i18n";
import { fill, plural } from "../i18n/format";
import { checkMediator, disconnectMediator, errorCode, mediatorName, type Mediation } from "../mediator";

type MediatorCardProps = {
  mediation: Mediation;
  /** Opens the screen where a mediator is chosen. */
  onConnect: () => void;
};

/**
 * Which mediator holds this wallet's messages, how many it is holding, and the
 * way to change it or leave it. After a change the previous one is still
 * collected for a while, and the card says until when.
 */
export function MediatorCard({ mediation, onConnect }: MediatorCardProps) {
  const { t, locale } = useI18n();
  const [waiting, setWaiting] = useState<number | null>(null);
  const [busy, setBusy] = useState<"check" | "disconnect" | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const status = mediation.status;
  const texts = t.settings.mediator;
  const shownError = error ?? (mediation.error ? t.messaging.errors[mediation.error] : null);
  const current = status?.mediators.find((one) => one.leaving === null) ?? null;
  const leaving = status?.mediators.find((one) => one.leaving !== null) ?? null;
  const date = new Intl.DateTimeFormat(locale, { dateStyle: "medium" });

  async function check() {
    setError(null);
    setBusy("check");
    try {
      setWaiting((await checkMediator()).messageCount);
    } catch (failure) {
      setError(t.messaging.errors[errorCode(failure)]);
    } finally {
      setBusy(null);
    }
  }

  async function disconnect() {
    setError(null);
    setBusy("disconnect");
    try {
      mediation.adopt(await disconnectMediator());
      setConfirming(false);
      setWaiting(null);
    } catch (failure) {
      setError(t.messaging.errors[errorCode(failure)]);
    } finally {
      setBusy(null);
    }
  }

  const previous =
    leaving?.leaving != null ? (
      <p className="card__note">
        {fill(texts.previous, {
          name: mediatorName(leaving.mediator),
          date: date.format(new Date(leaving.leaving * 1000)),
        })}
      </p>
    ) : null;

  return (
    <section className="card" aria-labelledby="mediator-title">
      <h2 className="card__title" id="mediator-title">
        {texts.title}
      </h2>

      {current ? (
        <>
          <dl className="detail-list">
            <div className="detail-list__row">
              <dt>{texts.nameLabel}</dt>
              <dd>{mediatorName(current.mediator)}</dd>
            </div>
            <div className="detail-list__row">
              <dt>{texts.addressesLabel}</dt>
              <dd>
                {current.maxDids === null
                  ? current.dids
                  : fill(texts.addressesOf, { count: current.dids, max: current.maxDids })}
                <small className="detail-list__aside">{texts.addressesHint}</small>
              </dd>
            </div>
            <div className="detail-list__row">
              <dt>{texts.waitingLabel}</dt>
              <dd>
                {waiting === null ? texts.waitingUnknown : plural(texts.waiting, waiting, locale)}
              </dd>
            </div>
          </dl>
          {previous}
          {current.maxDids !== null && current.dids * 10 >= current.maxDids * 9 ? (
            <p className="card__note card__note--warning">
              {current.dids >= current.maxDids
                ? texts.contactsFull
                : plural(texts.contactsNear, current.maxDids - current.dids, locale)}
            </p>
          ) : null}
          {confirming ? (
            <>
              <p className="card__note card__note--warning">
                {current.contacts > 0
                  ? plural(texts.disconnectContacts, current.contacts, locale)
                  : texts.disconnectEmpty}
              </p>
              <div className="button-row button-row--split">
                <button
                  type="button"
                  className="button"
                  onClick={() => setConfirming(false)}
                  disabled={busy !== null}
                >
                  {t.confirm.cancel}
                </button>
                <button
                  type="button"
                  className="button button--danger"
                  onClick={() => void disconnect()}
                  disabled={busy !== null}
                >
                  {texts.disconnect}
                </button>
              </div>
            </>
          ) : (
            <div className="button-row">
              <button
                type="button"
                className="button button--primary"
                onClick={() => void check()}
                disabled={busy !== null}
              >
                {busy === "check" ? texts.checking : texts.check}
              </button>
              <button type="button" className="button" onClick={onConnect} disabled={busy !== null}>
                {texts.change}
              </button>
              <button
                type="button"
                className="button"
                onClick={() => setConfirming(true)}
                disabled={busy !== null}
              >
                {texts.disconnect}
              </button>
            </div>
          )}
        </>
      ) : status ? (
        <>
          <p className="card__body">{texts.none}</p>
          {previous}
          <div className="button-row">
            <button type="button" className="button button--primary" onClick={onConnect}>
              {texts.connect}
            </button>
          </div>
        </>
      ) : null}

      {shownError ? <p className="card__note card__note--warning">{shownError}</p> : null}
    </section>
  );
}
