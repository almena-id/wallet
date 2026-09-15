import { useState } from "react";

import { ChevronLeftIcon } from "../../components/icons";
import { useTranslations } from "../../i18n";

type RestoreScreenProps = {
  /** Shown when the phrase was refused, already in the right language. */
  error: string | null;
  busy: boolean;
  onBack: () => void;
  onSubmit: (phrase: string) => void;
};

/** Signing in: there is no account to look up, only a phrase to derive from. */
export function RestoreScreen({ error, busy, onBack, onSubmit }: RestoreScreenProps) {
  const t = useTranslations();
  const [phrase, setPhrase] = useState("");

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">{t.onboarding.restore.title}</h1>
      </header>

      <p className="screen__intro">{t.onboarding.restore.intro}</p>

      <form
        className="card"
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit(phrase);
        }}
      >
        <label className="field">
          <span className="field__label">{t.onboarding.restore.title}</span>
          <textarea
            className="field__input field__input--phrase"
            value={phrase}
            onChange={(event) => setPhrase(event.target.value)}
            placeholder={t.onboarding.restore.placeholder}
            rows={4}
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
          />
        </label>

        {error ? <p className="card__note card__note--warning">{error}</p> : null}

        <div className="button-row">
          <button
            type="submit"
            className="button button--primary"
            disabled={phrase.trim().length === 0 || busy}
          >
            {t.onboarding.restore.submit}
          </button>
        </div>
      </form>
    </div>
  );
}
