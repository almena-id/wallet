import { useState } from "react";

import { autoLockMinutes, type AutoLock } from "../../autolock";
import { BiometricIcon, KeypadIcon } from "../../components/icons";
import { ChevronRow, ToggleRow } from "../../components/rows";
import { fill, plural, useI18n } from "../../i18n";
import { errorCode, setVaultDevice, type Vault } from "../../vault";

type SecuritySettingsProps = {
  vault: Vault;
  /** How long the wallet stays open with nobody using it. */
  autoLock: AutoLock;
  onAutoLockChange: (minutes: AutoLock) => void;
  /** Opens the screen where the PIN is replaced. */
  onChangePin: () => void;
  /** Opens the screen that asks for the PIN before the device is given a key. */
  onArmDevice: () => void;
  /** Opens the screen that explains what signing out costs. */
  onSignOut: () => void;
};

export function SecuritySettings({
  vault,
  autoLock,
  onAutoLockChange,
  onChangePin,
  onArmDevice,
  onSignOut,
}: SecuritySettingsProps) {
  const { locale, t } = useI18n();
  const [error, setError] = useState<string | null>(null);

  const { status } = vault;

  const disarm = async () => {
    setError(null);
    try {
      vault.adopt(await setVaultDevice(false));
    } catch (failure) {
      setError(t.vault.errors[errorCode(failure)]);
    }
  };

  return (
    <>
      <section className="card" aria-labelledby="security-lock">
        <h2 className="card__title" id="security-lock">
          {t.settings.security.lockTitle}
        </h2>

        {/* The PIN cannot be turned off — it is one of the two things that open
            the record — so it keeps the row and trades the switch for the arrow
            that leads to where it is replaced. */}
        <ChevronRow
          icon={<KeypadIcon />}
          label={t.settings.security.pinLabel}
          hint={
            status.digits
              ? fill(t.settings.security.pinDigits, { digits: status.digits })
              : undefined
          }
          onClick={onChangePin}
        />

        <ToggleRow
          icon={<BiometricIcon />}
          label={t.settings.security.biometricsLabel}
          hint={
            // One answer for every way this can be off — no sensor on this Mac,
            // a build the system will not give a protected keychain to, a
            // platform whose store would hand the key to anybody. They are the
            // same fact to the person reading the row: this device will not do
            // it. Which of the three it is belongs in the log, not here.
            status.deviceUnlock ? undefined : t.settings.security.biometricsUnavailable
          }
          on={status.deviceKey}
          disabled={!status.deviceUnlock}
          // Arming hands the platform a key, so it costs the PIN and happens on
          // a screen of its own. Disarming only takes one away.
          onChange={(on) => {
            if (on) {
              onArmDevice();
            } else {
              void disarm();
            }
          }}
        />

        {/* The one length there is: the wallet lets go this long after the last
            thing somebody did with it, on the screen or off it — see `useIdle`.
            The label says what it is; nothing here explains it. */}
        <p className="card__subtitle" id="security-autolock">
          {t.settings.security.autoLockLabel}
        </p>
        <div className="segmented" role="radiogroup" aria-labelledby="security-autolock">
          {autoLockMinutes.map((minutes) => (
            <button
              key={minutes}
              type="button"
              role="radio"
              aria-checked={minutes === autoLock}
              className={
                minutes === autoLock ? "segmented__option is-active" : "segmented__option"
              }
              onClick={() => onAutoLockChange(minutes)}
            >
              {plural(t.settings.security.autoLockMinutes, minutes, locale)}
            </button>
          ))}
        </div>

        <dl className="detail-list">
          <div className="detail-list__row">
            <dt>{t.settings.security.keptLabel}</dt>
            <dd>
              {status.home === "store"
                ? t.settings.security.keptInStore
                : t.settings.security.keptInFile}
            </dd>
          </div>
        </dl>

        {error ? <p className="card__note card__note--warning">{error}</p> : null}
      </section>

      <section className="card" aria-labelledby="security-sign-out">
        <h2 className="card__title" id="security-sign-out">
          {t.settings.security.signOutTitle}
        </h2>
        <div className="button-row">
          <button type="button" className="button button--danger" onClick={onSignOut}>
            {t.settings.security.signOut}
          </button>
        </div>
      </section>
    </>
  );
}
