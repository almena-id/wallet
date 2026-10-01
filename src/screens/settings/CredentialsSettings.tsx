import { useEffect, useState } from "react";

import { listCredentials, type HeldCredential } from "../../credentials";
import { useI18n } from "../../i18n";

/** A value as a person reads it: lists and groups joined. */
function shown(value: unknown): string {
  if (Array.isArray(value)) return value.map(shown).join(", ");
  if (value && typeof value === "object") return Object.values(value).map(shown).join(", ");
  return String(value ?? "");
}

/**
 * The credentials this wallet holds, one card each: what it is, who issued
 * it, until when, and what it says. Read from the sealed store each time the
 * section opens.
 */
export function CredentialsSettings() {
  const { t, locale } = useI18n();
  const copy = t.settings.credentials;
  const [held, setHeld] = useState<HeldCredential[] | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    listCredentials()
      .then(setHeld)
      .catch(() => setFailed(true));
  }, []);

  if (failed) return <p className="card__note card__note--warning">{copy.unreadable}</p>;
  if (held === null) return null;
  if (held.length === 0)
    return (
      <section className="card">
        <p className="card__body">{copy.none}</p>
      </section>
    );

  const day = (seconds: number) => new Date(seconds * 1000).toLocaleDateString(locale);
  return (
    <>
      {held.map((credential) => {
        const name =
          credential.typeLabels[locale] ?? credential.typeLabels.en ?? credential.typeId;
        const expired = credential.validUntil * 1000 < Date.now();
        return (
          <section key={credential.id} className="card" aria-label={name}>
            <h2 className="card__title">{name}</h2>
            <dl className="detail-list">
              <div className="detail-list__row">
                <dt>{copy.issuer}</dt>
                <dd>{credential.issuerName}</dd>
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
          </section>
        );
      })}
    </>
  );
}
