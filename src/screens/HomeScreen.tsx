import { CredentialIcon, QrIcon } from "../components/icons";
import { useTranslations } from "../i18n";
import type { Identity } from "../identity";

type HomeScreenProps = {
  /** The identity this session is running as. */
  identity: Identity;
  /** Opens the identifier as a code somebody else's camera can read. */
  onShowCode: () => void;
};

export function HomeScreen({ identity, onShowCode }: HomeScreenProps) {
  const t = useTranslations();

  return (
    <div className="screen">
      <header className="screen__header">
        <img className="brand-mark" src="/brand/app-icon.png" alt="" />
        <div>
          <p className="screen__eyebrow">{t.app.name}</p>
          <h1 className="screen__title">{t.home.greeting}</h1>
        </div>
      </header>

      <section className="card" aria-labelledby="home-credentials">
        <h2 className="card__title" id="home-credentials">
          {t.home.credentials.title}
        </h2>
        <div className="empty-state">
          <span className="empty-state__icon">
            <CredentialIcon />
          </span>
          <p className="empty-state__title">{t.home.credentials.empty}</p>
          <p className="empty-state__hint">{t.home.credentials.emptyHint}</p>
        </div>
      </section>

      <section className="card" aria-labelledby="home-identity">
        <h2 className="card__title" id="home-identity">
          {t.home.identity.title}
        </h2>
        <p className="card__subtitle">{t.home.identity.didLabel}</p>
        <p className="scan-result">{identity.did}</p>
        <button type="button" className="button button--primary button--icon" onClick={onShowCode}>
          <QrIcon />
          {t.home.identity.showCode}
        </button>
      </section>

    </div>
  );
}
