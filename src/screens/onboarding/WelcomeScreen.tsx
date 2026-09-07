import { CredentialIcon, QrIcon } from "../../components/icons";
import { useTranslations } from "../../i18n";

type WelcomeScreenProps = {
  onCreate: () => void;
  onSignIn: () => void;
};

/**
 * The first thing anybody sees: the mark and the name, centred in whatever
 * height the device gives them, and the two ways in sitting at the bottom where
 * a thumb reaches them.
 */
export function WelcomeScreen({ onCreate, onSignIn }: WelcomeScreenProps) {
  const t = useTranslations();

  return (
    <div className="screen screen--welcome">
      <div className="welcome">
        <img className="welcome__mark" src="/brand/app-icon.png" alt="" />
        <h1 className="welcome__name">{t.app.shortName}</h1>
        <p className="welcome__tagline">{t.onboarding.welcome.tagline}</p>
      </div>

      <div className="welcome__choices">
        <button type="button" className="action action--choice" onClick={onCreate}>
          <span className="action__icon">
            <CredentialIcon />
          </span>
          <span className="action__text">
            <span className="action__label">{t.onboarding.welcome.create}</span>
            <span className="action__hint">{t.onboarding.welcome.createHint}</span>
          </span>
        </button>

        <button type="button" className="action action--choice" onClick={onSignIn}>
          <span className="action__icon action__icon--quiet">
            <QrIcon />
          </span>
          <span className="action__text">
            <span className="action__label">{t.onboarding.welcome.signIn}</span>
            <span className="action__hint">{t.onboarding.welcome.signInHint}</span>
          </span>
        </button>
      </div>
    </div>
  );
}
