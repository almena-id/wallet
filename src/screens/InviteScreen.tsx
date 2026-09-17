import { useCallback, useEffect, useState } from "react";

import { CheckIcon, ChevronLeftIcon, CopyIcon, SyncIcon } from "../components/icons";
import { QrCode } from "../components/QrCode";
import { useI18n } from "../i18n";
import { errorCode, showInvitation } from "../messaging";

/** How long a code is on show before the next one takes its place. */
const LIFETIME_SECONDS = 30;

type InviteScreenProps = {
  /** The mediator the invitation's mailbox is opened at. */
  mediator: string;
  /** Back to the dashboard home. */
  onBack: () => void;
};

/**
 * The code somebody else's camera reads: an invitation to a relationship
 * with this wallet.
 *
 * **Never the identity.** What the code carries is a key made for this one
 * showing — see `messaging::invite` on the Rust side — from which nothing
 * else about the wallet follows. Whoever scans it writes to that key; the
 * wallet learns who they are when it next syncs, and answers from the
 * pairwise it has for them. So the screen has nothing to say about the
 * identity, because the code has nothing to do with it.
 *
 * A fresh code every time the screen opens, every thirty seconds after
 * that, and sooner on request: showing one is what makes the previous one
 * dead, and a code that was on a screen for a while is a code that may have
 * been photographed. The bar and the count say how long this one has left,
 * so that somebody lining up a camera knows whether to wait for the next.
 * The link is offered as text for the one case a camera does not cover — a
 * counterparty at a keyboard.
 */
export function InviteScreen({ mediator, onBack }: InviteScreenProps) {
  const { t, locale } = useI18n();
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [secondsLeft, setSecondsLeft] = useState(LIFETIME_SECONDS);

  const draw = useCallback(async () => {
    setUrl(null);
    setError(null);
    setCopied(false);
    try {
      const shown = await showInvitation(mediator);
      setUrl(shown.url);
      setSecondsLeft(LIFETIME_SECONDS);
    } catch (failure) {
      setError(t.messages.errors[errorCode(failure)]);
    }
  }, [mediator, t]);

  useEffect(() => {
    void draw();
  }, [draw]);

  // The clock runs only while a code is on show: nothing counts down over a
  // spinner or an error. At zero the next code is asked for, and the count
  // starts again when it arrives.
  useEffect(() => {
    if (url === null) {
      return;
    }
    const timer = window.setInterval(() => {
      setSecondsLeft((left) => Math.max(0, left - 1));
    }, 1000);
    return () => window.clearInterval(timer);
  }, [url]);

  useEffect(() => {
    if (url !== null && secondsLeft === 0) {
      void draw();
    }
  }, [url, secondsLeft, draw]);

  const seconds = new Intl.NumberFormat(locale, {
    style: "unit",
    unit: "second",
    unitDisplay: "narrow",
  }).format(secondsLeft);

  // Said, and then not said any more: a button that stays on "Copied" cannot
  // tell somebody that the second press worked too.
  useEffect(() => {
    if (!copied) {
      return;
    }
    const timer = window.setTimeout(() => setCopied(false), 2000);
    return () => window.clearTimeout(timer);
  }, [copied]);

  async function copyLink() {
    if (url === null) {
      return;
    }
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
    } catch {
      // A webview that refuses the clipboard leaves the code on screen,
      // which is what it is for.
    }
  }

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact screen__title--grow">
          {t.invite.title}
        </h1>
        <button
          type="button"
          className="icon-button"
          onClick={() => void draw()}
          disabled={url === null && error === null}
          aria-label={t.invite.rotate}
        >
          <SyncIcon />
        </button>
      </header>

      <section className="card card--centered">
        {url !== null ? (
          <div className="qr-plate">
            <QrCode value={url} label={t.invite.codeLabel} />
          </div>
        ) : error !== null ? (
          <p className="field__error">{error}</p>
        ) : (
          <div className="qr-plate qr-plate--waiting" role="status" aria-label={t.invite.preparing}>
            <span className="spinner" aria-hidden="true" />
          </div>
        )}
      </section>

      {url !== null ? (
        <div className="countdown" role="timer" aria-live="off">
          <span className="countdown__track" aria-hidden="true">
            <span
              className="countdown__bar"
              style={{ ["--fraction" as string]: secondsLeft / LIFETIME_SECONDS }}
            />
          </span>
          <span className="countdown__seconds">{seconds}</span>
        </div>
      ) : null}

      {url !== null ? (
        <div className="button-row">
          <button
            type="button"
            className="button button--icon"
            onClick={() => void copyLink()}
            aria-label={copied ? t.invite.copied : t.invite.copy}
          >
            {copied ? <CheckIcon /> : <CopyIcon />}
            {copied ? t.invite.copied : t.invite.copy}
          </button>
        </div>
      ) : null}
    </div>
  );
}
