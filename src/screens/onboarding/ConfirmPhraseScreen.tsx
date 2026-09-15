import { useMemo, useState } from "react";

import { ChevronLeftIcon } from "../../components/icons";
import { fill, plural, useI18n } from "../../i18n";

type ConfirmPhraseScreenProps = {
  words: string[];
  onBack: () => void;
  /** Every round answered right: the identity can be made. */
  onPassed: () => void;
  /** One wrong answer: the whole thing starts again, with new words. */
  onFailed: () => void;
};

/** How many words are asked for. */
const ROUNDS = 3;

/** How many words are offered each time, the right one among them. */
const CHOICES = 6;

type Round = {
  /** Zero-based index into the phrase. */
  position: number;
  /** The words offered, in the order they are shown. */
  options: string[];
};

function shuffled<T>(items: T[]): T[] {
  const copy = [...items];
  for (let i = copy.length - 1; i > 0; i -= 1) {
    const j = Math.floor(Math.random() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }
  return copy;
}

/**
 * Three rounds, six words each, whatever the phrase is long.
 *
 * The five wrong options are other words from the same phrase, which is what
 * makes this a test of the copy somebody keeps rather than of their memory of
 * what the screen looked like: every option is a word they wrote down, and only
 * the paper says which one is number seven.
 *
 * **Three rounds either way.** A longer phrase is not a longer quiz: the check
 * is that a copy exists and can be read from, and three positions out of
 * twenty-four settle that as well as three out of twelve.
 */
function rounds(words: string[]): Round[] {
  return shuffled(words.map((_, index) => index))
    .slice(0, ROUNDS)
    .sort((a, b) => a - b)
    .map((position) => {
      const decoys = shuffled(words.filter((_, index) => index !== position)).slice(0, CHOICES - 1);
      return { position, options: shuffled([words[position], ...decoys]) };
    });
}

export function ConfirmPhraseScreen({
  words,
  onBack,
  onPassed,
  onFailed,
}: ConfirmPhraseScreenProps) {
  const { t, locale } = useI18n();
  const asked = useMemo(() => rounds(words), [words]);
  const [round, setRound] = useState(0);

  const current = asked[round];

  function choose(word: string) {
    if (word !== words[current.position]) {
      onFailed();
      return;
    }
    if (round + 1 >= asked.length) {
      onPassed();
      return;
    }
    setRound(round + 1);
  }

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">{t.onboarding.confirm.title}</h1>
      </header>

      <p className="screen__intro">{t.onboarding.confirm.intro}</p>

      <section className="card" aria-labelledby="confirm-question">
        <p className="card__subtitle">
          {fill(t.onboarding.confirm.progress, { current: round + 1, total: asked.length })}
        </p>
        <h2 className="card__title" id="confirm-question">
          {fill(t.onboarding.confirm.question, { position: current.position + 1 })}
        </h2>

        <div className="choices">
          {current.options.map((word) => (
            <button
              key={word}
              type="button"
              className="choices__option"
              onClick={() => choose(word)}
            >
              {word}
            </button>
          ))}
        </div>
      </section>

      {/* What a wrong answer costs, counted in the words actually being
          checked: a phrase of twenty-four is twenty-four to write down again. */}
      <p className="card__note card__note--warning">
        {plural(t.onboarding.confirm.warning, words.length, locale)}
      </p>
    </div>
  );
}
