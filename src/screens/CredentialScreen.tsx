import { useState } from "react";

import { BackButton } from "../components/BackButton";
import { CredentialCard, StatusChip } from "../components/CredentialCard";
import { BadgeCheckIcon, SyncIcon } from "../components/icons";
import {
  credentialName,
  issuerLabel,
  registryOf,
  shown,
  standing,
  type HeldCredential,
} from "../credentials";
import { useI18n } from "../i18n";

type CredentialScreenProps = {
  credential: HeldCredential;
  /** Its status is being checked. */
  checking: boolean;
  onCheck: () => void;
  /** Removes it for good; rejects when it could not be. */
  onRemove: () => Promise<void>;
  onBack: () => void;
};

/**
 * One credential: its card, then who issued it, what type it is, until when,
 * how it stands — checked again on request with its issuer's status list —,
 * what it says, and removing it, which asks a second time: nothing keeps a
 * copy, and only its issuer can grant it again.
 */
export function CredentialScreen({ credential, checking, onCheck, onRemove, onBack }: CredentialScreenProps) {
  const { t, locale } = useI18n();
  const copy = t.credentials;
  const [asking, setAsking] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [removeFailed, setRemoveFailed] = useState(false);

  const name = credentialName(credential, locale);
  const issuer = issuerLabel(credential);
  const registry = registryOf(credential);
  const state = standing(credential);
  const { status, at, problem } = credential.checked;
  const expired = credential.validUntil * 1000 < Date.now();
  const claims = Object.entries(credential.claims);
  const day = (seconds: number) => new Date(seconds * 1000).toLocaleDateString(locale);

  const remove = () => {
    setRemoving(true);
    setRemoveFailed(false);
    onRemove()
      .catch(() => setRemoveFailed(true))
      .finally(() => setRemoving(false));
  };

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <BackButton onBack={onBack} />
        <h1 className="screen__title screen__title--compact screen__title--grow">{name}</h1>
      </header>

      <CredentialCard
        typeName={name}
        issuerName={issuer}
        category={{ label: copy.category, icon: <BadgeCheckIcon /> }}
        status={<StatusChip standing={state} label={copy.chips[state]} />}
      />

      {(status === "revoked" || status === "suspended") && (
        <p className="card__note card__note--warning">{copy.notice[status].replace("{issuer}", issuer)}</p>
      )}

      <section className="card" aria-label={copy.about}>
        <h2 className="card__title">{copy.about}</h2>
        <dl className="detail-list">
          <div className="detail-list__row">
            <dt>{copy.issuer}</dt>
            <dd>{issuer}</dd>
          </div>
          <div className="detail-list__row">
            <dt>{copy.issuerDid}</dt>
            <dd className="detail-list__mono">{credential.issuerDid}</dd>
          </div>
          <div className="detail-list__row">
            <dt>{copy.type}</dt>
            <dd className="detail-list__mono">{credential.typeId}</dd>
          </div>
          {registry ? (
            <div className="detail-list__row">
              <dt>{copy.registry}</dt>
              <dd>{registry}</dd>
            </div>
          ) : null}
          <div className="detail-list__row">
            <dt>{copy.issuedAt}</dt>
            <dd>{day(credential.issuedAt)}</dd>
          </div>
          <div className="detail-list__row">
            <dt>{expired ? copy.expired : copy.validUntil}</dt>
            <dd>{day(credential.validUntil)}</dd>
          </div>
          <div className="detail-list__row">
            <dt>{copy.status}</dt>
            <dd>
              {checking
                ? copy.unchecked.pending
                : status
                  ? copy.statuses[status]
                  : copy.unchecked[problem ?? "pending"]}
              {!checking && status && at ? (
                <small className="detail-list__aside">
                  {(problem === "unreachable" ? copy.checkedOffline : copy.checkedAt).replace(
                    "{when}",
                    new Date(at * 1000).toLocaleString(locale),
                  )}
                </small>
              ) : null}
            </dd>
          </div>
        </dl>
        <div className="button-row">
          <button type="button" className="button button--icon" onClick={onCheck} disabled={checking}>
            <SyncIcon className={checking ? "icon-button__spin" : undefined} />
            {copy.check}
          </button>
        </div>
      </section>

      <section className="card" aria-label={copy.claims}>
        <h2 className="card__title">{copy.claims}</h2>
        {claims.length === 0 ? (
          <p className="card__body">{copy.noClaims}</p>
        ) : (
          <dl className="detail-list">
            {claims.map(([claim, value]) => (
              <div key={claim} className="detail-list__row">
                <dt>{claim}</dt>
                <dd>{shown(value)}</dd>
              </div>
            ))}
          </dl>
        )}
      </section>

      <section className="card">
        {asking ? (
          <>
            <p className="card__note card__note--warning">{copy.removeQuestion.replace("{issuer}", issuer)}</p>
            {removeFailed ? <p className="card__note card__note--warning">{copy.removeFailed}</p> : null}
            <div className="button-row button-row--split">
              <button type="button" className="button" onClick={() => setAsking(false)} disabled={removing}>
                {copy.removeKeep}
              </button>
              <button type="button" className="button button--danger" onClick={remove} disabled={removing}>
                {copy.removeConfirm}
              </button>
            </div>
          </>
        ) : (
          <div className="button-row">
            <button
              type="button"
              className="button button--danger"
              onClick={() => {
                setRemoveFailed(false);
                setAsking(true);
              }}
            >
              {copy.remove}
            </button>
          </div>
        )}
      </section>
    </div>
  );
}
