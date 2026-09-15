import { useEffect, useState } from "react";

import { ChevronLeftIcon, CredentialIcon } from "../components/icons";
import { SlideToConfirm } from "../components/SlideToConfirm";
import { fill, useTranslations } from "../i18n";

/** How long the page insists on being read before it will let anybody out. */
const READ_SECONDS = 10;

type LogoutScreenProps = {
  onBack: () => void;
  onConfirmed: () => void;
};

/**
 * What signing out costs, and a door that takes ten seconds and a deliberate
 * drag to open.
 *
 * Nothing about this identity is stored anywhere else, so this is not a session
 * ending — it is the identity leaving the only device that has it. The delay is
 * there because the cost of reading for ten seconds is ten seconds, and the cost
 * of not reading is everything the identity holds.
 */
export function LogoutScreen({ onBack, onConfirmed }: LogoutScreenProps) {
  const t = useTranslations();
  const [remaining, setRemaining] = useState(READ_SECONDS);

  useEffect(() => {
    const timer = window.setInterval(() => {
      setRemaining((seconds) => (seconds <= 1 ? 0 : seconds - 1));
    }, 1000);
    return () => window.clearInterval(timer);
  }, []);

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">{t.logout.title}</h1>
      </header>

      <p className="screen__intro">{t.logout.intro}</p>

      <section className="card">
        <span className="card__icon">
          <CredentialIcon />
        </span>
        <ul className="consequences">
          <li className="consequences__item">{t.logout.identity}</li>
          <li className="consequences__item">{t.logout.credentials}</li>
          <li className="consequences__item">{t.logout.phrase}</li>
        </ul>
      </section>

      <p className="card__note card__note--warning">{t.logout.question}</p>

      <SlideToConfirm
        label={t.logout.slide}
        doneLabel={t.logout.slideDone}
        waitingLabel={fill(t.logout.waiting, { seconds: remaining })}
        disabled={remaining > 0}
        onConfirm={onConfirmed}
      />

      <div className="button-row">
        <button type="button" className="button" onClick={onBack}>
          {t.logout.cancel}
        </button>
      </div>
    </div>
  );
}
