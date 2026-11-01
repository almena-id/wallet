import { useCallback, useState } from "react";

import { useTranslations } from "../i18n";
import { changeVaultPin, errorCode, openVault, type Vault, type VaultStatus } from "../vault";
import { PinScreen } from "./PinScreen";
import { PinSetup } from "./PinSetup";

type PinChangeProps = {
  vault: Vault;
  /** How long the current PIN is, so the keypad matches what is being asked for. */
  digits: number;
  /** Where the new status goes once it has been replaced. */
  onChanged: (status: VaultStatus) => void;
  /** Absent where the change cannot be put off: a PIN too short to keep. */
  onBack?: () => void;
  /** The current PIN when it was typed a moment ago to unlock, so it is not asked twice. */
  known?: string | null;
  /** What the screen choosing the new one is called and says, when not a plain change. */
  title?: string;
  intro?: string;
};

/**
 * Replacing the PIN, which is two questions in a fixed order.
 *
 * **The current one is asked for first, and checked before the new one.** A
 * wallet left open on a table must not be able to have its PIN quietly
 * replaced by somebody walking past, and finding out at the end that the first
 * answer was wrong would mean typing the new one twice for nothing.
 */
export function PinChange({ vault, digits, onChanged, onBack, known, title, intro }: PinChangeProps) {
  const t = useTranslations();
  const [current, setCurrent] = useState<string | null>(known ?? null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const check = useCallback(
    async (code: string) => {
      setError(null);
      setBusy(true);
      try {
        // Opening it is the check. There is nothing stored that verifies a PIN
        // — it is right exactly when what it derives opens the record — so this
        // is the only way to ask, and the identity it hands back is the one
        // already open.
        await openVault(code);
        setCurrent(code);
      } catch (failure) {
        setError(t.vault.errors[errorCode(failure)]);
        // The same digits are counted here as on the lock screen, and the tenth
        // wrong one destroys the record. Asking again is what takes the whole
        // application back to the way in rather than leaving somebody typing at
        // a wallet that no longer has anything to open.
        void vault.refresh();
      } finally {
        setBusy(false);
      }
    },
    [t, vault],
  );

  const replace = useCallback(
    async (next: string) => {
      if (current === null) {
        return;
      }
      setError(null);
      setBusy(true);
      try {
        onChanged(await changeVaultPin(current, next));
      } catch (failure) {
        const code = errorCode(failure);
        setError(t.vault.errors[code]);
        // A current PIN that turned out wrong — one handed over from the lock
        // that the record no longer takes — is asked for again.
        if (code === "vault_wrong_pin") {
          setCurrent(null);
        }
        void vault.refresh();
      } finally {
        setBusy(false);
      }
    },
    [current, onChanged, t, vault],
  );

  if (current === null) {
    return (
      <PinScreen
        title={t.pin.currentTitle}
        subtitle={t.pin.currentSubtitle}
        digits={digits}
        error={error}
        busy={busy}
        busyLabel={t.pin.checking}
        onBack={onBack}
        onComplete={(code) => {
          void check(code);
        }}
      />
    );
  }

  return (
    <PinSetup
      title={title ?? t.settings.security.pinChange}
      intro={intro ?? t.pin.replaceIntro}
      error={error}
      busy={busy}
      busyLabel={t.pin.checking}
      onBack={onBack}
      minDigits={vault.status.minDigits}
      onChosen={replace}
    />
  );
}
