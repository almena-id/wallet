import { useEffect, useState } from "react";

import {
  checkCredentials,
  listCredentials,
  removeCredential,
  type HeldCredential,
} from "../../credentials";
import { useI18n } from "../../i18n";

/** A value as a person reads it: lists and groups joined. */
function shown(value: unknown): string {
  if (Array.isArray(value)) return value.map(shown).join(", ");
  if (value && typeof value === "object")
    return Object.values(value).map(shown).join(", ");
  return String(value ?? "");
}

/**
 * The credentials this wallet holds, one card each: what it is, who issued
 * it, its status, until when, and what it says. Read from the sealed store
 * each time the section opens, then checked with each issuer's status list —
 * what was known shows until the check answers.
 */
export function CredentialsSettings() {
  const { t, locale } = useI18n();
  const copy = t.settings.credentials;
  const [held, setHeld] = useState<HeldCredential[] | null>(null);
  const [failed, setFailed] = useState(false);
  // The one whose removal is being asked about, and the one being removed.
  const [asking, setAsking] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [removeFailed, setRemoveFailed] = useState<string | null>(null);

  const remove = (id: string) => {
    setRemoving(id);
    setRemoveFailed(null);
    removeCredential(id)
      .then((rest) => {
        setHeld(rest);
        setAsking(null);
      })
      .catch(() => setRemoveFailed(id))
      .finally(() => setRemoving(null));
  };

  useEffect(() => {
    let live = true;
    listCredentials()
      .then((list) => {
        if (!live) return;
        setHeld(list);
        if (list.length > 0)
          void checkCredentials()
            .then((checked) => live && setHeld(checked))
            .catch(() => {});
      })
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, []);

  if (failed)
    return <p className="card__note card__note--warning">{copy.unreadable}</p>;
  if (held === null) return null;
  if (held.length === 0)
    return (
      <section className="card">
        <p className="card__body">{copy.none}</p>
      </section>
    );

  const day = (seconds: number) =>
    new Date(seconds * 1000).toLocaleDateString(locale);
  return (
    <>
      {held.map((credential) => {
        const name =
          credential.typeLabels[locale] ??
          credential.typeLabels.en ??
          credential.typeId;
        const expired = credential.validUntil * 1000 < Date.now();
        const { status, at, problem } = credential.checked;
        return (
          <section key={credential.id} className="card" aria-label={name}>
            <h2 className="card__title">{name}</h2>
            {(status === "revoked" || status === "suspended") && (
              <p className="card__note card__note--warning">
                {copy.notice[status].replace("{issuer}", credential.issuerName)}
              </p>
            )}
            <dl className="detail-list">
              <div className="detail-list__row">
                <dt>{copy.issuer}</dt>
                <dd>{credential.issuerName}</dd>
              </div>
              <div className="detail-list__row">
                <dt>{copy.status}</dt>
                <dd>
                  {status
                    ? copy.statuses[status]
                    : copy.unchecked[problem ?? "pending"]}
                  {status && at && (
                    <small className="detail-list__aside">
                      {(problem === "unreachable"
                        ? copy.checkedOffline
                        : copy.checkedAt
                      ).replace(
                        "{when}",
                        new Date(at * 1000).toLocaleString(locale),
                      )}
                    </small>
                  )}
                </dd>
              </div>
              <div className="detail-list__row">
                <dt>{expired ? copy.expired : copy.validUntil}</dt>
                <dd>{day(credential.validUntil)}</dd>
              </div>
              {Object.entries(credential.claims).map(([claim, value]) => (
                <div key={claim} className="detail-list__row">
                  <dt>{claim}</dt>
                  <dd>{shown(value)}</dd>
                </div>
              ))}
            </dl>
            {asking === credential.id ? (
              <>
                <p className="card__note card__note--warning">
                  {copy.removeQuestion.replace("{issuer}", credential.issuerName)}
                </p>
                {removeFailed === credential.id && (
                  <p className="card__note card__note--warning">{copy.removeFailed}</p>
                )}
                <div className="button-row button-row--split">
                  <button
                    type="button"
                    className="button"
                    onClick={() => setAsking(null)}
                    disabled={removing !== null}
                  >
                    {copy.removeKeep}
                  </button>
                  <button
                    type="button"
                    className="button button--danger"
                    onClick={() => remove(credential.id)}
                    disabled={removing !== null}
                  >
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
                    setRemoveFailed(null);
                    setAsking(credential.id);
                  }}
                >
                  {copy.remove}
                </button>
              </div>
            )}
          </section>
        );
      })}
    </>
  );
}
