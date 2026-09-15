import { useEffect, useRef, useState } from "react";

import { BrandSpinner } from "../components/BrandSpinner";
import { BackspaceIcon, BiometricIcon, ChevronLeftIcon } from "../components/icons";
import { useTranslations } from "../i18n";

type PinScreenProps = {
  title: string;
  subtitle: string;
  /** How many digits this PIN has. */
  digits: number;
  /** Shown under the dots, already in the right language. */
  error?: string | null;
  /** Called with the digits once as many as `digits` have been typed. */
  onComplete: (code: string) => void;
  /** When given, a back arrow appears. The lock screen has none. */
  onBack?: () => void;
  /** Offered next to the keypad, for a wallet that unlocks with a face. */
  onBiometrics?: () => void;
  /** The way out for somebody who does not remember the code. */
  footer?: React.ReactNode;
  /**
   * Whether the digits are being checked right now.
   *
   * **The wait is real and it is not short.** Deriving the key a PIN opens the
   * record with is deliberately expensive — that cost is the whole of what
   * stands between four digits and somebody who has the record — so a keypad
   * that simply sat there after the last digit would read as a wallet that had
   * not noticed. It becomes the same mark turning that the rest of the flow
   * uses, so the wait reads as one step rather than as nothing happening.
   */
  busy?: boolean;
  /** What is being waited for, read out to anybody who cannot see the mark. */
  busyLabel?: string;
};

const KEYS = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

/**
 * How long the finished code stays on screen before it is acted on.
 *
 * **So the last digit can be seen.** Acting the instant it is typed means the
 * dot it fills is painted and replaced in the same breath, and the screen
 * changes under a thumb that has not lifted yet — which reads as the wallet
 * having reacted to the digit before it, and leaves nobody sure the last one
 * registered. Long enough to notice, short enough not to feel like waiting.
 */
const SETTLE_MS = 220;

/**
 * The keypad, used to choose a PIN, to repeat it, and to get back in.
 *
 * Its own keys rather than the system keyboard: a PIN is digits and only
 * digits, and a numeric keyboard on a phone is still a keyboard with everything
 * else one tap away.
 */
export function PinScreen({
  title,
  subtitle,
  digits,
  error,
  onComplete,
  onBack,
  onBiometrics,
  footer,
  busy = false,
  busyLabel,
}: PinScreenProps) {
  const t = useTranslations();
  const [code, setCode] = useState("");

  // Held in a ref rather than watched: the callback is rebuilt on every render
  // by most of the screens that use this one, and an effect that depended on it
  // would restart the pause below each time anything re-rendered.
  const complete = useRef(onComplete);
  complete.current = onComplete;

  // A complete code leaves this screen, and the screen forgets it: whatever
  // happens next, it does not happen with somebody's digits still on it.
  //
  // It waits a moment first, so the dot the last digit filled is actually seen
  // — see `SETTLE_MS`. The code is left on screen for that moment rather than
  // cleared early, because clearing it is what would blank the dots. Deleting a
  // digit inside the pause cancels the whole thing, which falls out of the
  // effect depending on the code: a hand that changed its mind in time is
  // answered by nothing happening.
  useEffect(() => {
    if (code.length !== digits) {
      return;
    }

    const typed = code;
    const settle = setTimeout(() => {
      setCode("");
      complete.current(typed);
    }, SETTLE_MS);

    return () => clearTimeout(settle);
  }, [code, digits]);

  // A new question — repeat it, try again — starts empty.
  useEffect(() => {
    setCode("");
  }, [title, error]);

  // The same centred mark the rest of the flow waits behind, so deriving an
  // identity and then sealing it read as one step and not two screens.
  if (busy) {
    return (
      <BrandSpinner label={busyLabel ?? t.pin.checking} />
    );
  }

  return (
    <div className="screen screen--pin">
      {onBack ? (
        <header className="screen__header screen__header--compact">
          <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
            <ChevronLeftIcon />
          </button>
          <h1 className="screen__title screen__title--compact">{title}</h1>
        </header>
      ) : (
        <h1 className="screen__title screen__title--centred">{title}</h1>
      )}

      <p className="pin__subtitle">{subtitle}</p>

      <div className="pin__dots" role="status" aria-live="polite">
        {Array.from({ length: digits }, (_, index) => (
          <span
            key={index}
            className={index < code.length ? "pin__dot pin__dot--filled" : "pin__dot"}
          />
        ))}
      </div>

      <p className={error ? "pin__error" : "pin__error pin__error--empty"}>{error ?? " "}</p>

      <div className="pin__keys">
        {KEYS.map((key) => (
          <button
            key={key}
            type="button"
            className="pin__key"
            onClick={() => setCode((current) => (current + key).slice(0, digits))}
          >
            {key}
          </button>
        ))}

        {onBiometrics ? (
          <button
            type="button"
            className="pin__key pin__key--quiet"
            onClick={onBiometrics}
            aria-label={t.pin.useBiometrics}
          >
            <BiometricIcon />
          </button>
        ) : (
          <span className="pin__key pin__key--empty" aria-hidden="true" />
        )}

        <button
          type="button"
          className="pin__key"
          onClick={() => setCode((current) => (current + "0").slice(0, digits))}
        >
          0
        </button>

        <button
          type="button"
          className="pin__key pin__key--quiet"
          onClick={() => setCode((current) => current.slice(0, -1))}
          aria-label={t.pin.delete}
        >
          <BackspaceIcon />
        </button>
      </div>

      {footer ? <div className="pin__footer">{footer}</div> : null}
    </div>
  );
}
