import { useState } from "react";

import { useTranslations } from "../i18n";
import { fill } from "../i18n/format";
import { pinLengths } from "../vault";
import { PinScreen } from "./PinScreen";
import { BackButton } from "../components/BackButton";

type PinSetupProps = {
  /** What the screen is called, since it is used to choose a first PIN and to replace one. */
  title: string;
  /** Said once above the keypad, before any digits are asked for. */
  intro?: string;
  /** The chosen digits, once they have been typed the same way twice. */
  onChosen: (pin: string) => Promise<void>;
  /** Shown under the dots when choosing them failed. Already in the right language. */
  error?: string | null;
  /** When given, a back arrow appears. Onboarding has none: there is nowhere back to. */
  onBack?: () => void;
  /** Whether the chosen digits are being acted on. */
  busy?: boolean;
  /** What is being waited for, read out to anybody who cannot see the mark. */
  busyLabel?: string;
  /**
   * The shortest PIN this device takes (`VaultStatus.minDigits`). Where only
   * six digits are offered, the length is said rather than asked, with why.
   */
  minDigits?: number;
};

/** Choosing a PIN is two questions: how long, then twice what it is. */
type Step =
  | { name: "length" }
  | { name: "choose"; digits: number }
  | { name: "repeat"; digits: number; first: string };

/**
 * Setting a PIN, on a screen of its own.
 *
 * It takes the whole screen for the same reason the lock does: a keypad with a
 * menu under it is a keypad somebody taps past by accident.
 */
export function PinSetup({
  title,
  intro,
  onChosen,
  error,
  onBack,
  busy,
  busyLabel,
  minDigits = 4,
}: PinSetupProps) {
  const t = useTranslations();
  const lengths = pinLengths(minDigits);
  const [step, setStep] = useState<Step>({ name: "length" });
  const [mismatch, setMismatch] = useState<string | null>(null);

  if (step.name === "length") {
    return (
      <div className="screen">
        {onBack ? (
          <header className="screen__header screen__header--compact">
            <BackButton onBack={onBack} />
            <h1 className="screen__title screen__title--compact">{title}</h1>
          </header>
        ) : (
          <header className="screen__header">
            <h1 className="screen__title">{title}</h1>
          </header>
        )}

        {intro ? <p className="screen__intro">{intro}</p> : null}

        {lengths.length === 1 ? (
          // One length is no question. Where the record could be copied off
          // the device, four digits are too few to stand between it and a
          // patient guesser, and the person is told so.
          <section className="card">
            <h2 className="card__title">{t.settings.security.pinLengthSix}</h2>
            <p className="card__body">{t.pin.sixOnly}</p>
            <div className="button-row">
              <button
                type="button"
                className="button button--primary"
                onClick={() => setStep({ name: "choose", digits: lengths[0] })}
              >
                {t.pin.continue}
              </button>
            </div>
          </section>
        ) : (
          <section className="card">
            <h2 className="card__title">{t.settings.security.pinLength}</h2>
            <div className="segmented" role="group" aria-label={t.settings.security.pinLength}>
              {lengths.map((digits) => (
                <button
                  key={digits}
                  type="button"
                  className="segmented__option"
                  onClick={() => setStep({ name: "choose", digits })}
                >
                  {digits === 4
                    ? t.settings.security.pinLengthFour
                    : t.settings.security.pinLengthSix}
                </button>
              ))}
            </div>
          </section>
        )}

        {error ? <p className="card__note card__note--warning">{error}</p> : null}
      </div>
    );
  }

  const choosing = step.name === "choose";

  return (
    <PinScreen
      title={choosing ? t.pin.chooseTitle : t.pin.repeatTitle}
      subtitle={
        choosing ? fill(t.pin.chooseSubtitle, { digits: step.digits }) : t.pin.repeatSubtitle
      }
      digits={step.digits}
      error={mismatch ?? error ?? null}
      busy={busy}
      busyLabel={busyLabel}
      onBack={() => {
        setMismatch(null);
        setStep({ name: "length" });
      }}
      onComplete={(code) => {
        setMismatch(null);
        if (step.name === "choose") {
          setStep({ name: "repeat", digits: step.digits, first: code });
          return;
        }
        if (code === step.first) {
          void onChosen(code);
          return;
        }
        // A mismatch goes back to the first question rather than leaving
        // somebody guessing which of the two was the slip.
        setMismatch(t.pin.mismatch);
        setStep({ name: "choose", digits: step.digits });
      }}
    />
  );
}
