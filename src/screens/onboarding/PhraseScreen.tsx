import { useEffect, useState } from "react";

import { ChevronLeftIcon } from "../../components/icons";
import { useTranslations } from "../../i18n";

type PhraseScreenProps = {
  words: string[];
  /** Set when these words replaced a set somebody failed to confirm. */
  restarted: boolean;
  onBack: () => void;
  onContinue: () => void;
};

/**
 * The twelve words, and a door that only opens deliberately: the checkbox is
 * the difference between having read the warning and having answered it.
 */
export function PhraseScreen({ words, restarted, onBack, onContinue }: PhraseScreenProps) {
  const t = useTranslations();
  const [copied, setCopied] = useState(false);
  const [acknowledged, setAcknowledged] = useState(false);

  // New words are a new promise: whatever was ticked was ticked about the words
  // that are now gone.
  useEffect(() => {
    setAcknowledged(false);
    setCopied(false);
  }, [words]);

  async function copyPhrase() {
    try {
      await navigator.clipboard.writeText(words.join(" "));
      setCopied(true);
    } catch {
      // A webview that refuses the clipboard leaves the words on screen, which
      // is where they were always meant to be read from.
    }
  }

  return (
    <div className="screen">
      <header className="screen__header screen__header--scan">
        <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">{t.onboarding.phrase.title}</h1>
      </header>

      {restarted ? (
        <p className="card__note card__note--warning">{t.onboarding.phrase.restarted}</p>
      ) : null}

      <p className="screen__intro">{t.onboarding.phrase.intro}</p>

      <ol className="phrase">
        {words.map((word, index) => (
          <li className="phrase__word" key={`${index}-${word}`}>
            <span className="phrase__position">{index + 1}</span>
            <span className="phrase__text">{word}</span>
          </li>
        ))}
      </ol>

      <p className="card__note card__note--warning">{t.onboarding.phrase.warning}</p>

      <label className="checkbox">
        <input
          type="checkbox"
          className="checkbox__input"
          checked={acknowledged}
          onChange={(event) => setAcknowledged(event.target.checked)}
        />
        <span className="checkbox__label">{t.onboarding.phrase.acknowledge}</span>
      </label>

      <div className="button-row">
        <button
          type="button"
          className="button button--primary"
          onClick={onContinue}
          disabled={!acknowledged}
        >
          {t.onboarding.phrase.continue}
        </button>
        <button type="button" className="button" onClick={copyPhrase}>
          {copied ? t.onboarding.phrase.copied : t.onboarding.phrase.copy}
        </button>
      </div>
    </div>
  );
}
