import { useEffect, useRef } from "react";

import { BrandSpinner } from "../components/BrandSpinner";
import { useTranslations } from "../i18n";
import { useBackInSight } from "../lock";

type DeviceLockScreenProps = {
  /** Shown under the line, already in the right language. */
  error?: string | null;
  /** Whether the iPhone is being asked right now. */
  busy: boolean;
  /** Asks the iPhone: the prompt is the system's, raised by reading the key. */
  onUnlock: () => void;
  /** The way out for somebody the phone will not let in. */
  footer?: React.ReactNode;
};

/**
 * The lock of a wallet that has no PIN: the iPhone's own, Face ID first and the
 * passcode after it.
 *
 * **It asks on its own.** The prompt is raised the moment this screen appears
 * and again whenever the wallet comes back on screen, the way a bank's
 * application does; the button is only for somebody who dismissed it. There is
 * nothing to type here — the passcode, when it is asked for, is typed into the
 * system's sheet, never into the wallet.
 */
export function DeviceLockScreen({ error, busy, onUnlock, footer }: DeviceLockScreenProps) {
  const t = useTranslations();

  const latest = useRef({ onUnlock, busy });
  latest.current = { onUnlock, busy };

  // Once per appearance. The guard is for development, where the effect runs
  // twice and would put two prompts on the screen one after the other.
  const asked = useRef(false);
  useEffect(() => {
    if (!asked.current) {
      asked.current = true;
      latest.current.onUnlock();
    }
  }, []);
  useBackInSight(() => {
    if (!latest.current.busy) {
      latest.current.onUnlock();
    }
  });

  if (busy) {
    return <BrandSpinner label={t.vault.deviceLock.checking} />;
  }

  return (
    <div className="screen screen--pin">
      <h1 className="screen__title screen__title--centred">{t.vault.deviceLock.title}</h1>
      <p className="pin__subtitle">{t.vault.deviceLock.subtitle}</p>
      <p className={error ? "pin__error" : "pin__error pin__error--empty"}>{error ?? " "}</p>

      <div className="button-row">
        <button type="button" className="button button--primary" onClick={onUnlock}>
          {t.vault.deviceLock.unlock}
        </button>
      </div>

      {footer ? <div className="pin__footer">{footer}</div> : null}
    </div>
  );
}
