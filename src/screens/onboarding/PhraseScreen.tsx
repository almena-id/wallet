import { useEffect, useState } from "react";

import { ChevronLeftIcon } from "../../components/icons";
import { plural, useI18n } from "../../i18n";
import { PHRASE_LENGTHS, type PhraseLength } from "../../identity";

type PhraseScreenProps = {
  words: string[];
  /** The length being asked for, which is the words on screen unless one is on its way. */
  length: PhraseLength;
  /** Set when these words replaced a set somebody failed to confirm. */
  restarted: boolean;
  /** True while a phrase of a newly chosen length is being made. */
  busy: boolean;
  onLength: (length: PhraseLength) => void;
  onBack: () => void;
  onContinue: () => void;
};

/**
 * The words, and a door that only opens deliberately: the checkbox is the
 * difference between having read the warning and having answered it.
 *
 * **Everything on this screen counts the words in front of somebody**, not the
 * length they last tapped. The two are the same except for the moment a new
 * phrase is being made, and during that moment the wording that describes the
 * words has to describe the ones still on the screen.
 */
export function PhraseScreen({
  words,
  length,
  restarted,
  busy,
  onLength,
  onBack,
  onContinue,
}: PhraseScreenProps) {
  const { t, locale } = useI18n();
  const [copied, setCopied] = useState(false);
  const [acknowledged, setAcknowledged] = useState(false);

  // New words are a new promise: whatever was ticked was ticked about the words
  // that are now gone. Choosing the other length lands here too, which is the
  // point — a longer phrase is not the one that was already written down.
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

  const shown = words.length;

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">
          {plural(t.onboarding.phrase.title, shown, locale)}
        </h1>
      </header>

      {restarted ? (
        <p className="card__note card__note--warning">{t.onboarding.phrase.restarted}</p>
      ) : null}

      {/* The choice comes before the words, because taking it after they are
          written down is what it costs: a different length is a different
          phrase, and the one that was on the paper opens nothing. */}
      <section className="card" aria-labelledby="phrase-length">
        <h2 className="card__title" id="phrase-length">
          {t.onboarding.phrase.length}
        </h2>
        <div className="segmented" role="radiogroup" aria-labelledby="phrase-length">
          {PHRASE_LENGTHS.map((option) => (
            <button
              key={option}
              type="button"
              role="radio"
              aria-checked={option === length}
              disabled={busy}
              className={option === length ? "segmented__option is-active" : "segmented__option"}
              onClick={() => onLength(option)}
            >
              {plural(t.onboarding.phrase.lengthOption, option, locale)}
            </button>
          ))}
        </div>
      </section>

      <p className="screen__intro">{plural(t.onboarding.phrase.intro, shown, locale)}</p>

      <ol className="phrase" aria-busy={busy}>
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
          disabled={busy}
          onChange={(event) => setAcknowledged(event.target.checked)}
        />
        <span className="checkbox__label">
          {plural(t.onboarding.phrase.acknowledge, shown, locale)}
        </span>
      </label>

      <div className="button-row">
        <button
          type="button"
          className="button button--primary"
          onClick={onContinue}
          disabled={!acknowledged || busy}
        >
          {t.onboarding.phrase.continue}
        </button>
        <button type="button" className="button" onClick={copyPhrase} disabled={busy}>
          {copied ? t.onboarding.phrase.copied : t.onboarding.phrase.copy}
        </button>
      </div>
    </div>
  );
}
