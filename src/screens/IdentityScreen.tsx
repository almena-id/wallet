import { useEffect, useState } from "react";

import { CheckIcon, ChevronLeftIcon, CopyIcon } from "../components/icons";
import { QrCode } from "../components/QrCode";
import { useTranslations } from "../i18n";
import type { Identity } from "../identity";

type IdentityScreenProps = {
  /** The identity this session is running as. */
  identity: Identity;
  /** Back to the dashboard home. */
  onBack: () => void;
};

/**
 * The identifier, as a code to point a camera at and as text to take away.
 *
 * **The identifier and nothing else.** It is the public half — the thing that
 * is given out — so there is nothing here to warn anybody about, and nothing
 * that would be a mistake to photograph.
 */
export function IdentityScreen({ identity, onBack }: IdentityScreenProps) {
  const t = useTranslations();
  const [copied, setCopied] = useState(false);

  // Said, and then not said any more: a button that stays on "Copied" cannot
  // tell somebody that the second press worked too.
  useEffect(() => {
    if (!copied) {
      return;
    }
    const timer = window.setTimeout(() => setCopied(false), 2000);
    return () => window.clearTimeout(timer);
  }, [copied]);

  async function copyDid() {
    try {
      await navigator.clipboard.writeText(identity.did);
      setCopied(true);
    } catch {
      // A webview that refuses the clipboard leaves the identifier on screen,
      // selectable, which is where it can be read from either way.
    }
  }

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <button type="button" className="icon-button" onClick={onBack} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">{t.identity.title}</h1>
      </header>

      <section className="card card--centered">
        <div className="qr-plate">
          <QrCode value={identity.did} label={t.identity.codeLabel} />
        </div>
      </section>

      <section className="card">
        <p className="card__subtitle">{t.identity.didLabel}</p>
        {/* The identifier is the control: pressing what is written is what
            copies it. Named for a reader rather than read out — a screen reader
            working through fifty-six characters of base58 is being given noise,
            and the identifier is on the home screen as text as well. */}
        <button
          type="button"
          className="copy-field"
          onClick={() => void copyDid()}
          aria-label={copied ? t.identity.copied : t.identity.copy}
        >
          <span className="copy-field__value">{identity.did}</span>
          <span className="copy-field__action">
            {copied ? <CheckIcon /> : <CopyIcon />}
            {copied ? t.identity.copied : t.identity.copy}
          </span>
        </button>
      </section>
    </div>
  );
}
