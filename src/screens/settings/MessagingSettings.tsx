import { CheckIcon } from "../../components/icons";
import { useTranslations } from "../../i18n";
import { mediatorName, mediators, type Mediator } from "../../mediator";

type MessagingSettingsProps = {
  mediator: Mediator;
  onMediatorChange: (mediator: Mediator) => void;
};

/**
 * Which mediator the wallet leaves its messages with.
 *
 * One list and one tick: every mediator this wallet knows, and the one the
 * next relationship will be opened with. The row says what a mediator is
 * called and what it is named — the host, and the DID under it — and nothing
 * about what a mediator does: somebody who came here knows.
 */
export function MessagingSettings({ mediator, onMediatorChange }: MessagingSettingsProps) {
  const t = useTranslations();

  return (
    <section className="card" aria-labelledby="messaging-mediator">
      <h2 className="card__title" id="messaging-mediator">
        {t.settings.messaging.mediator.title}
      </h2>
      <div className="options" role="radiogroup" aria-labelledby="messaging-mediator">
        {mediators.map((option) => (
          <button
            key={option}
            type="button"
            role="radio"
            aria-checked={option === mediator}
            className="row"
            onClick={() => onMediatorChange(option)}
          >
            <span className="row__text">
              <span className="row__label">{mediatorName(option)}</span>
              <span className="row__hint row__hint--identifier">{option}</span>
            </span>
            {option === mediator ? <CheckIcon className="row__check" /> : null}
          </button>
        ))}
      </div>
    </section>
  );
}
