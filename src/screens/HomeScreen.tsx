import { CredentialCard, StatusChip } from "../components/CredentialCard";
import { BadgeCheckIcon, QrIcon, SyncIcon } from "../components/icons";
import { EmptyWallet } from "../components/illustrations/EmptyWallet";
import { credentialName, issuerLabel, standing, type HeldCredential } from "../credentials";
import { useI18n } from "../i18n";
import { fill } from "../i18n/format";

type HomeScreenProps = {
  /** The credentials held, newest first; null until they are read. */
  held: HeldCredential[] | null;
  /** They could not be read. */
  failed: boolean;
  /** Their statuses are being checked. */
  checking: boolean;
  onCheck: () => void;
  /** Opens the credential `id`. */
  onOpen: (id: string) => void;
  /** Opens the scanner; absent where there is none. */
  onScan?: () => void;
};

/**
 * Home: the greeting, then every credential this wallet holds as its card,
 * each with how it stands — one under the other, two to a row on a wide
 * window. With nothing held, a drawing and where credentials come from.
 */
export function HomeScreen({ held, failed, checking, onCheck, onOpen, onScan }: HomeScreenProps) {
  const { t, locale } = useI18n();
  const copy = t.credentials;
  const any = held !== null && held.length > 0;

  return (
    <div className="screen screen--wide">
      <header className="screen__header">
        <img className="brand-mark" src="/brand/app-icon.png" alt="" />
        <div className="screen__title--grow">
          <p className="screen__eyebrow">{t.app.name}</p>
          <h1 className="screen__title">{t.home.greeting}</h1>
        </div>
        {any ? (
          <button
            type="button"
            className="icon-button"
            onClick={onCheck}
            disabled={checking}
            aria-label={t.home.refresh}
          >
            <SyncIcon className={checking ? "icon-button__spin" : undefined} />
          </button>
        ) : null}
      </header>

      {failed ? <p className="card__note card__note--warning">{copy.unreadable}</p> : null}

      {held === null ? null : any ? (
        <section className="credential-grid" aria-label={t.home.credentials}>
          {held.map((credential) => {
            const name = credentialName(credential, locale);
            const issuer = issuerLabel(credential);
            const state = standing(credential);
            const status = copy.chips[state];
            return (
              <CredentialCard
                key={credential.id}
                typeName={name}
                issuerName={issuer}
                category={{ label: copy.category, icon: <BadgeCheckIcon /> }}
                status={<StatusChip standing={state} label={status} />}
                onPress={() => onOpen(credential.id)}
                label={fill(copy.open, { name, issuer, status })}
              />
            );
          })}
        </section>
      ) : (
        <section className="card">
          <div className="empty-state">
            <EmptyWallet className="empty-state__art" />
            <p className="empty-state__title">{t.home.emptyTitle}</p>
            <p className="empty-state__hint">{t.home.emptyHint}</p>
            {onScan ? (
              <div className="button-row button-row--centred">
                <button type="button" className="button button--primary button--icon" onClick={onScan}>
                  <QrIcon />
                  {t.home.scan}
                </button>
              </div>
            ) : null}
          </div>
        </section>
      )}
    </div>
  );
}
