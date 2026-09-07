import { useCallback, useEffect, useState } from "react";

import { BrandSpinner } from "../../components/BrandSpinner";
import { useI18n } from "../../i18n";
import {
  createIdentity,
  discardDraft,
  draftPhrase,
  errorCode,
  restoreIdentity,
  type Identity,
} from "../../identity";
import { createVault, errorCode as vaultErrorCode } from "../../vault";
import { PinSetup } from "../PinSetup";
import { ConfirmPhraseScreen } from "./ConfirmPhraseScreen";
import { PhraseScreen } from "./PhraseScreen";
import { RestoreScreen } from "./RestoreScreen";
import { WelcomeScreen } from "./WelcomeScreen";

type Step =
  | { name: "welcome" }
  | { name: "phrase"; words: string[]; restarted: boolean }
  | { name: "confirm"; words: string[] }
  | { name: "restore" }
  /** The mark turning while the identity is derived. */
  | { name: "creating"; identity: Identity | null }
  /** The last step, and the one that makes it a wallet: a PIN to keep it behind. */
  | { name: "protect"; identity: Identity };

/** How long the mark turns at the least, so the wait reads as a step and not a flicker. */
const SPINNER_MS = 900;

type OnboardingProps = {
  /** Hands over the identity: the wallet opens on the dashboard with it. */
  onReady: (identity: Identity) => void;
};

/**
 * The way in: make an identity from twelve new words, or bring one back from
 * twelve somebody already has.
 */
export function Onboarding({ onReady }: OnboardingProps) {
  const { t, locale } = useI18n();
  const [step, setStep] = useState<Step>({ name: "welcome" });
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const say = useCallback(
    (failure: unknown) => setError(t.onboarding.errors[errorCode(failure)]),
    [t],
  );

  const toWelcome = useCallback(() => {
    setError(null);
    discardDraft();
    setStep({ name: "welcome" });
  }, []);

  /** A fresh phrase, either the first one or the one a wrong answer earned. */
  const startCreating = useCallback(
    async (restarted: boolean) => {
      setError(null);
      setBusy(true);
      try {
        // The wordlist follows the interface, so the words are ones this person
        // reads rather than ones they transcribe.
        const words = await draftPhrase(locale);
        setStep({ name: "phrase", words, restarted });
      } catch (failure) {
        say(failure);
        setStep({ name: "welcome" });
      } finally {
        setBusy(false);
      }
    },
    [locale, say],
  );

  // The identity is derived while the mark turns, and both have to be done
  // before the next screen: a spinner that vanishes in 40ms is a flicker nobody
  // can read, and one that outlasts the work is a lie about waiting.
  useEffect(() => {
    if (step.name !== "creating" || step.identity === null) {
      return;
    }
    const identity = step.identity;
    const timer = window.setTimeout(() => setStep({ name: "protect", identity }), SPINNER_MS);
    return () => window.clearTimeout(timer);
  }, [step]);

  /**
   * The PIN, and with it the only moment the identity is written down.
   *
   * It is asked for here rather than offered later in Settings because it is one
   * of the two things that open the record: without it there is nothing to write
   * the seed behind, and a wallet that kept the seed unprotected until somebody
   * wandered into Settings would be a wallet that was never safe.
   */
  const protect = useCallback(
    async (identity: Identity, pin: string) => {
      setError(null);
      setBusy(true);
      try {
        await createVault(pin);
        onReady(identity);
      } catch (failure) {
        setError(t.vault.errors[vaultErrorCode(failure)]);
      } finally {
        setBusy(false);
      }
    },
    [onReady, t],
  );

  /** Derives while the mark turns, and goes back to `onFailure` if it cannot. */
  const derive = useCallback(
    async (make: () => Promise<Identity>, onFailure: Step) => {
      setError(null);
      setStep({ name: "creating", identity: null });
      try {
        setStep({ name: "creating", identity: await make() });
      } catch (failure) {
        say(failure);
        setStep(onFailure);
      }
    },
    [say],
  );

  switch (step.name) {
    case "phrase":
      return (
        <PhraseScreen
          words={step.words}
          restarted={step.restarted}
          onBack={toWelcome}
          onContinue={() => setStep({ name: "confirm", words: step.words })}
        />
      );
    case "confirm":
      return (
        <ConfirmPhraseScreen
          words={step.words}
          onBack={() => setStep({ name: "phrase", words: step.words, restarted: false })}
          onPassed={() => {
            void derive(createIdentity, { name: "welcome" });
          }}
          // A wrong answer ends this phrase. The one being shown next is new,
          // and has to be written down like the first.
          onFailed={() => {
            discardDraft();
            void startCreating(true);
          }}
        />
      );
    case "restore":
      return (
        <RestoreScreen
          error={error}
          busy={busy}
          onBack={toWelcome}
          // A phrase that is refused leaves somebody on the screen they wrote
          // it on, with the reason under it, rather than back at the start.
          onSubmit={(phrase) => {
            void derive(() => restoreIdentity(phrase), { name: "restore" });
          }}
        />
      );
    case "creating":
      return (
        <BrandSpinner label={t.onboarding.creating.title} />
      );
    case "protect":
      return (
        <PinSetup
          title={t.onboarding.protect.title}
          intro={t.onboarding.protect.intro}
          error={error}
          busy={busy}
          busyLabel={t.onboarding.protect.saving}
          onChosen={(pin) => protect(step.identity, pin)}
        />
      );
    default:
      return (
        <>
          <WelcomeScreen
            onCreate={() => {
              void startCreating(false);
            }}
            onSignIn={() => {
              setError(null);
              setStep({ name: "restore" });
            }}
          />
          {error ? <p className="card__note card__note--warning">{error}</p> : null}
        </>
      );
  }
}
