import { useEffect, useState } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";

import { CheckIcon, CopyIcon } from "../components/icons";
import { QrCode } from "../components/QrCode";
import { useTranslations } from "../i18n";
import { showInvitation } from "../contacts";
import { errorCode } from "../mediator";
import { BackButton } from "../components/BackButton";

type InviteScreenProps = {
  onBack: () => void;
};

/**
 * This wallet's invitation, as a code a camera reads and a link to send.
 *
 * **It names the contact card, never the identity or a pairwise.** Whoever
 * uses it is answered from a DID made for them alone, so the same code can be
 * shown to anybody, printed, or — one day — published in the directory.
 */
export function InviteScreen({ onBack }: InviteScreenProps) {
  const t = useTranslations();
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    showInvitation()
      .then(setUrl)
      .catch((failure) => setError(t.messaging.errors[errorCode(failure)]));
  }, [t]);

  // Said, and then not said any more, so a second press is seen to work too.
  useEffect(() => {
    if (!copied) {
      return;
    }
    const timer = window.setTimeout(() => setCopied(false), 2000);
    return () => window.clearTimeout(timer);
  }, [copied]);

  async function copy() {
    if (url === null) {
      return;
    }
    try {
      // Through the native clipboard: the webview's own is refused on iOS.
      await writeText(url);
      setCopied(true);
    } catch {
      // Nothing to copy to leaves the code on screen, which is what it is for.
    }
  }

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <BackButton onBack={onBack} />
        <h1 className="screen__title screen__title--compact">{t.conversations.invite.title}</h1>
      </header>

      <p className="screen__intro">{t.conversations.invite.intro}</p>

      <section className="card card--centered">
        {url !== null ? (
          <div className="qr-plate">
            <QrCode value={url} label={t.conversations.invite.codeLabel} />
          </div>
        ) : error !== null ? (
          <p className="field__error">{error}</p>
        ) : (
          <div
            className="qr-plate qr-plate--waiting"
            role="status"
            aria-label={t.conversations.invite.preparing}
          >
            <span className="spinner" aria-hidden="true" />
          </div>
        )}
      </section>

      {url !== null ? (
        <div className="button-row">
          <button type="button" className="button button--icon" onClick={() => void copy()}>
            {copied ? <CheckIcon /> : <CopyIcon />}
            {copied ? t.conversations.invite.copied : t.conversations.invite.copy}
          </button>
        </div>
      ) : null}
    </div>
  );
}
