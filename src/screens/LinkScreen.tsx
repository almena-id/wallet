import { ChevronLeftIcon, CredentialIcon } from "../components/icons";
import { useTranslations } from "../i18n";

type LinkScreenProps = {
  /** The link the wallet was opened with. */
  url: string;
  /** Leaves the request behind and goes back to the dashboard home. */
  onBack: () => void;
};

/**
 * What an `almena://` link brought, shown and nothing more. The wallet does not
 * act on a link: see `useDeepLink`.
 */
export function LinkScreen({ url, onBack }: LinkScreenProps) {
  const t = useTranslations();

  return (
    <div className="screen">
      <header className="screen__header screen__header--scan">
        <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">{t.link.title}</h1>
      </header>

      <div className="card">
        <span className="card__icon">
          <CredentialIcon />
        </span>
        <h2 className="card__title">{t.link.received}</h2>
        <p className="card__body">{t.link.body}</p>
        <p className="card__subtitle">{t.link.urlLabel}</p>
        <p className="scan-result">{url}</p>
        <button type="button" className="button button--primary" onClick={onBack}>
          {t.nav.backToHome}
        </button>
      </div>
    </div>
  );
}
