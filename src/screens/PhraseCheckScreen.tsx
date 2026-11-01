import { useCallback, useState } from "react";

import { biometricPrompt, biometricWords } from "../biometrics";
import { BackButton } from "../components/BackButton";
import { BrandSpinner } from "../components/BrandSpinner";
import { useTranslations } from "../i18n";
import { phraseVerdict, type PhraseVerdict } from "../identity";
import { confirmationFor, verdictNote } from "../phraseCheck";
import { useShield } from "../shield";
import { confirmVault, errorCode, type Vault } from "../vault";
import { PinScreen } from "./PinScreen";

type PhraseCheckScreenProps = {
  vault: Vault;
  onBack: () => void;
};

/**
 * Security → Check your recovery phrase.
 *
 * **The phrase cannot be shown again**, since only the seed it made is kept;
 * this is how its holder learns whether what they wrote down still opens this
 * wallet. The PIN (or the device's own check) comes first, so a wallet left
 * open cannot be used to test words against; the words then go to Rust, which
 * derives them as a restore would and answers yes or no. They are dropped from
 * this screen once they match and whenever it is left, and the screen is
 * shielded like every other screen the phrase is on.
 */
export function PhraseCheckScreen({ vault, onBack }: PhraseCheckScreenProps) {
  const t = useTranslations();
  useShield();
  const confirmation = confirmationFor(vault.status);
  const [confirmed, setConfirmed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [phrase, setPhrase] = useState("");
  const [verdict, setVerdict] = useState<PhraseVerdict | null>(null);

  const confirm = useCallback(
    async (pin?: string) => {
      setError(null);
      setBusy(true);
      try {
        await confirmVault(pin, pin === undefined ? biometricPrompt(t, "confirm") : undefined);
        setConfirmed(true);
      } catch (failure) {
        setError(t.vault.errors[errorCode(failure)]);
        // A PIN given here spends an attempt like any other.
        void vault.refresh();
      } finally {
        setBusy(false);
      }
    },
    [t, vault],
  );

  const check = useCallback(async () => {
    setBusy(true);
    const answer = await phraseVerdict(phrase);
    setBusy(false);
    if (answer === "identity_unconfirmed") {
      // The confirmation ran out while the words were typed: ask again.
      setConfirmed(false);
      setError(t.onboarding.errors.identity_unconfirmed);
      return;
    }
    setVerdict(answer);
    if (answer === "match") {
      setPhrase("");
    }
  }, [phrase, t]);

  if (!confirmed && confirmation.way === "pin") {
    return (
      <PinScreen
        title={t.phraseCheck.title}
        subtitle={t.phraseCheck.pinSubtitle}
        digits={vault.status.digits ?? 4}
        error={error}
        busy={busy}
        busyLabel={t.pin.checking}
        onBack={onBack}
        onComplete={(code) => {
          void confirm(code);
        }}
        onBiometrics={confirmation.biometrics ? () => void confirm() : undefined}
        biometricsLabel={biometricWords(t, vault.status.biometrics).use}
      />
    );
  }

  if (busy && !confirmed) {
    return <BrandSpinner label={t.phraseCheck.confirming} />;
  }

  const note = verdict ? verdictNote(t, verdict) : null;

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <BackButton onBack={onBack} />
        <h1 className="screen__title screen__title--compact">{t.phraseCheck.title}</h1>
      </header>

      {!confirmed ? (
        <section className="card">
          <p className="card__body">{t.phraseCheck.deviceSubtitle}</p>
          {error ? <p className="card__note card__note--warning">{error}</p> : null}
          <div className="button-row">
            <button
              type="button"
              className="button button--primary"
              onClick={() => {
                void confirm();
              }}
            >
              {t.phraseCheck.confirm}
            </button>
          </div>
        </section>
      ) : (
        <>
          <p className="screen__intro">{t.phraseCheck.intro}</p>

          <form
            className="card"
            onSubmit={(event) => {
              event.preventDefault();
              void check();
            }}
          >
            <label className="field">
              <span className="field__label">{t.phraseCheck.label}</span>
              <textarea
                className="field__input field__input--phrase"
                value={phrase}
                onChange={(event) => {
                  setPhrase(event.target.value);
                  setVerdict(null);
                }}
                placeholder={t.phraseCheck.placeholder}
                rows={4}
                autoCapitalize="none"
                autoCorrect="off"
                autoComplete="off"
                spellCheck={false}
              />
            </label>

            {note ? (
              <p
                className={note.warning ? "card__note card__note--warning" : "card__note"}
                role="status"
              >
                {note.text}
              </p>
            ) : null}

            <div className="button-row">
              {verdict === "match" ? (
                <button type="button" className="button button--primary" onClick={onBack}>
                  {t.phraseCheck.done}
                </button>
              ) : (
                <button
                  type="submit"
                  className="button button--primary"
                  disabled={phrase.trim().length === 0 || busy}
                  aria-label={busy ? t.phraseCheck.checking : undefined}
                >
                  {t.phraseCheck.submit}
                </button>
              )}
            </div>
          </form>
        </>
      )}
    </div>
  );
}
