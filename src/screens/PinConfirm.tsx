import { useCallback, useState } from "react";

import { useTranslations } from "../i18n";
import { errorCode, setVaultDevice, type Vault, type VaultStatus } from "../vault";
import { PinScreen } from "./PinScreen";

type PinConfirmProps = {
  vault: Vault;
  digits: number;
  onArmed: (status: VaultStatus) => void;
  onBack: () => void;
};

/**
 * The PIN, asked for once, before the device is given a key of its own.
 *
 * **Arming is handing the platform a second way in.** A wallet somebody left
 * unlocked on a table must not be able to have one added to it by whoever picks
 * it up, so it costs the digits — the same digits that would have opened it
 * anyway, which is exactly the point.
 */
export function PinConfirm({ vault, digits, onArmed, onBack }: PinConfirmProps) {
  const t = useTranslations();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const arm = useCallback(
    async (code: string) => {
      setError(null);
      setBusy(true);
      try {
        onArmed(await setVaultDevice(true, code));
      } catch (failure) {
        setError(t.vault.errors[errorCode(failure)]);
        // Arming spends an attempt like any other answer, so a record that has
        // just run out has to reach the rest of the application.
        void vault.refresh();
      } finally {
        setBusy(false);
      }
    },
    [onArmed, t, vault],
  );

  return (
    <PinScreen
      title={t.settings.security.biometricsLabel}
      subtitle={t.pin.armSubtitle}
      digits={digits}
      error={error}
      busy={busy}
      busyLabel={t.pin.checking}
      onBack={onBack}
      onComplete={(code) => {
        void arm(code);
      }}
    />
  );
}
