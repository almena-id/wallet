import { useCallback, useEffect, useState } from "react";

import { ChevronLeftIcon, CredentialIcon } from "../components/icons";
import { useTranslations } from "../i18n";
import {
  acceptSignIn,
  declineSignIn,
  errorCode,
  readSignIn,
  type SignInErrorCode,
  type SignInRequest,
} from "../signin";

type ApprovalScreenProps = {
  /** The link that arrived, by deep link or by camera. */
  link: string;
  /** Leaves the request behind and goes back to the dashboard home. */
  onBack: () => void;
};

type Stage =
  | { at: "reading" }
  | { at: "asking"; request: SignInRequest }
  | { at: "answering"; request: SignInRequest }
  | { at: "declined" }
  | { at: "failed"; code: SignInErrorCode };

/**
 * Where somebody decides whether to sign in.
 *
 * **Nothing is accepted by arriving here.** A link can be put in front of
 * anybody, so what it asks for is read, checked against the platform's
 * signature, and then shown — and it goes no further until somebody says so.
 */
export function ApprovalScreen({ link, onBack }: ApprovalScreenProps) {
  const t = useTranslations();
  const [stage, setStage] = useState<Stage>({ at: "reading" });
  const [remaining, setRemaining] = useState<string | null>(null);

  useEffect(() => {
    let looking = true;

    readSignIn(link)
      .then((request) => {
        if (looking) {
          setStage({ at: "asking", request });
        }
      })
      .catch((error: unknown) => {
        if (looking) {
          setStage({ at: "failed", code: errorCode(error) });
        }
      });

    return () => {
      looking = false;
    };
  }, [link]);

  // Counted while the request is on screen, so somebody deciding can see how
  // long they have rather than find out by being refused.
  const expiresAt =
    stage.at === "asking" || stage.at === "answering"
      ? stage.request.expiresAt
      : null;

  useEffect(() => {
    if (expiresAt === null) {
      return;
    }

    const tick = () => {
      const seconds = Math.max(0, Math.round(expiresAt - Date.now() / 1000));
      const minutes = Math.floor(seconds / 60);
      setRemaining(`${minutes}:${String(seconds % 60).padStart(2, "0")}`);
    };

    const first = setTimeout(tick, 0);
    const timer = setInterval(tick, 1000);

    return () => {
      clearTimeout(first);
      clearInterval(timer);
    };
  }, [expiresAt]);

  const accept = useCallback(() => {
    setStage((current) =>
      current.at === "asking"
        ? { at: "answering", request: current.request }
        : current,
    );

    acceptSignIn()
      // Straight back to the wallet, with nothing to dismiss. An accepted
      // request has nothing left to tell anybody: the screen that asked is
      // already signing somebody in, and it is the one they are looking at. A
      // card saying it worked would only be a second thing to put away.
      .then(onBack)
      .catch((error: unknown) =>
        setStage({ at: "failed", code: errorCode(error) }),
      );
  }, [onBack]);

  const decline = useCallback(() => {
    // Reported before leaving, so the screen somebody walked away from can say
    // what happened instead of counting down to nothing.
    declineSignIn()
      .then(() => setStage({ at: "declined" }))
      .catch(() => setStage({ at: "declined" }));
  }, []);

  return (
    <div className="screen">
      <header className="screen__header screen__header--scan">
        <button
          type="button"
          className="icon-button"
          onClick={onBack}
          aria-label={t.nav.back}
        >
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">
          {t.signin.title}
        </h1>
      </header>

      {stage.at === "reading" ? (
        <div className="card">
          <p className="card__body">{t.signin.reading}</p>
        </div>
      ) : null}

      {stage.at === "asking" || stage.at === "answering" ? (
        <div className="card">
          <span className="card__icon">
            <CredentialIcon />
          </span>

          <h2 className="card__title">
            {stage.request.verifier.name} {t.signin.asks}
          </h2>
          <p className="scan-result">{stage.request.verifier.did}</p>

          <p className="card__body">{t.signin.proves}</p>
          {/* The property that makes this worth doing, said plainly rather
              than left for somebody to work out. */}
          <p className="card__body">{t.signin.separate}</p>

          <p className="card__subtitle">{t.signin.verifiedBy}</p>
          {remaining !== null ? (
            <p className="card__subtitle">
              {t.signin.expiresIn.replace("{time}", remaining)}
            </p>
          ) : null}

          {/* Refusing on the left, accepting on the right, the same size. The
              one that gives something away is not the one under the thumb by
              default, and neither is dressed as the obvious answer. */}
          <div className="button-row button-row--split">
            <button
              type="button"
              className="button"
              onClick={decline}
              disabled={stage.at === "answering"}
            >
              {t.signin.decline}
            </button>

            <button
              type="button"
              className="button button--primary"
              onClick={accept}
              disabled={stage.at === "answering"}
            >
              {stage.at === "answering" ? t.signin.accepting : t.signin.accept}
            </button>
          </div>
        </div>
      ) : null}

      {/* There is no card for having accepted — see `accept`. Refusing and
          failing both keep one, because each of those is something somebody
          has to be told, and neither is said anywhere else. */}
      {stage.at === "declined" ? (
        <div className="card">
          <h2 className="card__title">{t.signin.declined}</h2>
          <p className="card__body">{t.signin.declinedBody}</p>
          <button type="button" className="button button--primary" onClick={onBack}>
            {t.nav.backToHome}
          </button>
        </div>
      ) : null}

      {stage.at === "failed" ? (
        <div className="card">
          <h2 className="card__title">{t.signin.title}</h2>
          <p className="card__body">{t.signin.errors[stage.code]}</p>
          <button type="button" className="button button--primary" onClick={onBack}>
            {t.nav.backToHome}
          </button>
        </div>
      ) : null}
    </div>
  );
}
