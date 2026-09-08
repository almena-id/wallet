import { useCallback, useEffect, useState } from "react";

import { ChevronLeftIcon, CredentialIcon } from "../components/icons";
import { fill, useTranslations } from "../i18n";
import {
  acceptInvitation,
  errorCode,
  readInvitation,
  type Invitation,
  type InvitationErrorCode,
} from "../invitation";

type InvitationScreenProps = {
  /** The link that arrived, by deep link or by camera. */
  link: string;
  /** Leaves the invitation behind and goes back to the dashboard home. */
  onBack: () => void;
};

type Stage =
  | { at: "reading" }
  | { at: "asking"; invitation: Invitation }
  | { at: "answering"; invitation: Invitation }
  | { at: "sent"; invitation: Invitation }
  | { at: "failed"; code: InvitationErrorCode };

/**
 * Where somebody decides whether to join an organization.
 *
 * **Nothing is accepted by arriving here.** An invitation reaches a mailbox and
 * a mailbox can be anybody's, so what it offers is read, checked against the
 * platform's signature, and then shown — and it goes no further until somebody
 * says so.
 *
 * Unlike a sign-in, accepting keeps a card. A sign-in that succeeds is already
 * being shown on the screen the person was standing at; an invitation that
 * succeeds is waiting on somebody else entirely, and nothing else would ever
 * tell them so.
 */
export function InvitationScreen({ link, onBack }: InvitationScreenProps) {
  const t = useTranslations();
  const [stage, setStage] = useState<Stage>({ at: "reading" });
  const [remaining, setRemaining] = useState<string | null>(null);

  useEffect(() => {
    let looking = true;

    readInvitation(link)
      .then((invitation) => {
        if (looking) {
          setStage({ at: "asking", invitation });
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

  // Counted while the invitation is on screen. In days rather than in seconds:
  // an invitation lasts a week, and a clock ticking down from 167 hours is a
  // clock nobody reads.
  const expiresAt =
    stage.at === "asking" || stage.at === "answering"
      ? stage.invitation.expiresAt
      : null;

  useEffect(() => {
    if (expiresAt === null) {
      return;
    }

    const tick = () => {
      const seconds = Math.max(0, Math.round(expiresAt - Date.now() / 1000));
      const hours = Math.ceil(seconds / 3600);
      setRemaining(
        hours > 48
          ? fill(t.invitation.expiresInDays, { days: Math.ceil(hours / 24) })
          : fill(t.invitation.expiresInHours, { hours }),
      );
    };

    const first = setTimeout(tick, 0);
    // A minute, not a second: nothing on this screen changes faster than that.
    const timer = setInterval(tick, 60_000);

    return () => {
      clearTimeout(first);
      clearInterval(timer);
    };
  }, [expiresAt, t]);

  const accept = useCallback(() => {
    setStage((current) =>
      current.at === "asking"
        ? { at: "answering", invitation: current.invitation }
        : current,
    );

    acceptInvitation()
      .then(() =>
        setStage((current) =>
          current.at === "answering"
            ? { at: "sent", invitation: current.invitation }
            : current,
        ),
      )
      .catch((error: unknown) =>
        setStage({ at: "failed", code: errorCode(error) }),
      );
  }, []);

  const shown = stage.at === "asking" || stage.at === "answering";

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
          {t.invitation.title}
        </h1>
      </header>

      {stage.at === "reading" ? (
        <div className="card">
          <p className="card__body">{t.invitation.reading}</p>
        </div>
      ) : null}

      {shown ? (
        <div className="card">
          <span className="card__icon">
            <CredentialIcon />
          </span>

          <h2 className="card__title">
            {fill(t.invitation.invites, {
              entity: stage.invitation.entity.name,
            })}
          </h2>
          <p className="scan-result">{stage.invitation.entity.did}</p>

          {/* The role comes from the platform's own closed list, so an
              unfamiliar word is shown as it arrived rather than dropped. */}
          <p className="card__body">
            {t.invitation.roles[
              stage.invitation.offered.role as keyof typeof t.invitation.roles
            ] ?? stage.invitation.offered.role}
          </p>
          {stage.invitation.offered.name !== null ? (
            <p className="card__body">
              {fill(t.invitation.listedAs, {
                name: stage.invitation.offered.name,
              })}
            </p>
          ) : null}

          {/* What accepting gives away, and what it does not settle. Both are
              consequences somebody is entitled to know before they decide. */}
          <p className="card__body">{t.invitation.separate}</p>
          <p className="card__body">{t.invitation.thenConfirmed}</p>

          <p className="card__subtitle">{t.invitation.verifiedBy}</p>
          {remaining !== null ? (
            <p className="card__subtitle">{remaining}</p>
          ) : null}

          {/* Leaving on the left, accepting on the right, the same size. The
              one that gives something away is not the one under the thumb by
              default. */}
          <div className="button-row button-row--split">
            <button
              type="button"
              className="button"
              onClick={onBack}
              disabled={stage.at === "answering"}
            >
              {t.invitation.cancel}
            </button>

            <button
              type="button"
              className="button button--primary"
              onClick={accept}
              disabled={stage.at === "answering"}
            >
              {stage.at === "answering"
                ? t.invitation.accepting
                : t.invitation.accept}
            </button>
          </div>
        </div>
      ) : null}

      {stage.at === "sent" ? (
        <div className="card">
          <h2 className="card__title">{t.invitation.sent}</h2>
          <p className="card__body">
            {fill(t.invitation.sentBody, {
              entity: stage.invitation.entity.name,
            })}
          </p>
          <button
            type="button"
            className="button button--primary"
            onClick={onBack}
          >
            {t.nav.backToHome}
          </button>
        </div>
      ) : null}

      {stage.at === "failed" ? (
        <div className="card">
          <h2 className="card__title">{t.invitation.title}</h2>
          <p className="card__body">{t.invitation.errors[stage.code]}</p>
          <button
            type="button"
            className="button button--primary"
            onClick={onBack}
          >
            {t.nav.backToHome}
          </button>
        </div>
      ) : null}
    </div>
  );
}
