import { useState } from "react";

import { BrandSpinner } from "../components/BrandSpinner";
import { useTranslations } from "../i18n";
import { acceptInvitation, type Contact } from "../contacts";
import { errorCode } from "../mediator";
import { BackButton } from "../components/BackButton";

type AcceptInvitationScreenProps = {
  onBack: () => void;
  onAccepted: (contact: Contact) => void;
};

/** Somebody else's invitation, pasted: accepting it says hello from a new pairwise. */
export function AcceptInvitationScreen({ onBack, onAccepted }: AcceptInvitationScreenProps) {
  const t = useTranslations();
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function accept() {
    setError(null);
    setBusy(true);
    try {
      onAccepted(await acceptInvitation(input));
    } catch (failure) {
      setError(t.messaging.errors[errorCode(failure)]);
      setBusy(false);
    }
  }

  if (busy) {
    return <BrandSpinner label={t.conversations.accept.accepting} />;
  }

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <BackButton onBack={onBack} />
        <h1 className="screen__title screen__title--compact">{t.conversations.accept.title}</h1>
      </header>

      <p className="screen__intro">{t.conversations.accept.intro}</p>

      <form
        className="card"
        onSubmit={(event) => {
          event.preventDefault();
          void accept();
        }}
      >
        <label className="field">
          <span className="field__label">{t.conversations.accept.label}</span>
          <textarea
            className="field__input field__input--phrase"
            value={input}
            onChange={(event) => setInput(event.target.value)}
            placeholder={t.conversations.accept.placeholder}
            rows={4}
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
          />
        </label>

        {error ? <p className="card__note card__note--warning">{error}</p> : null}

        <div className="button-row">
          <button
            type="submit"
            className="button button--primary"
            disabled={input.trim().length === 0}
          >
            {t.conversations.accept.submit}
          </button>
        </div>
      </form>
    </div>
  );
}
