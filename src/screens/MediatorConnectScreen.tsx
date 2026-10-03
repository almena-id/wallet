import { useState } from "react";

import { BrandSpinner } from "../components/BrandSpinner";
import { useTranslations } from "../i18n";
import {
  connectMediator,
  errorCode,
  suggestedMediator,
  type MediatorStatus,
} from "../mediator";
import { BackButton } from "../components/BackButton";

type MediatorConnectScreenProps = {
  onBack: () => void;
  onConnected: (status: MediatorStatus) => void;
};

/**
 * Choosing a mediator: its invitation link, its address or its DID. The one
 * offered is filled in, so connecting to it is one tap.
 */
export function MediatorConnectScreen({ onBack, onConnected }: MediatorConnectScreenProps) {
  const t = useTranslations();
  const [input, setInput] = useState(suggestedMediator);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function connect() {
    setError(null);
    setBusy(true);
    try {
      onConnected(await connectMediator(input));
    } catch (failure) {
      setError(t.messaging.errors[errorCode(failure)]);
      setBusy(false);
    }
  }

  if (busy) {
    return <BrandSpinner label={t.mediator.connecting} />;
  }

  return (
    <div className="screen">
      <header className="screen__header screen__header--compact">
        <BackButton onBack={onBack} />
        <h1 className="screen__title screen__title--compact">{t.mediator.title}</h1>
      </header>

      <p className="screen__intro">{t.mediator.intro}</p>

      <form
        className="card"
        onSubmit={(event) => {
          event.preventDefault();
          void connect();
        }}
      >
        <label className="field">
          <span className="field__label">{t.mediator.label}</span>
          <textarea
            className="field__input field__input--phrase"
            value={input}
            onChange={(event) => setInput(event.target.value)}
            placeholder={t.mediator.placeholder}
            rows={3}
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
            {t.mediator.submit}
          </button>
        </div>
      </form>
    </div>
  );
}
