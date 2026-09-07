import { accents, defaultAccent, type Accent } from "../../appearance";
import { CheckIcon } from "../../components/icons";
import { useTranslations } from "../../i18n";
import { themes, type Theme } from "../../theme";

type AppearanceSettingsProps = {
  accent: Accent;
  onAccentChange: (accent: Accent) => void;
  theme: Theme;
  onThemeChange: (theme: Theme) => void;
};

/** The identity colour and the light or dark the wallet is read in. */
export function AppearanceSettings({
  accent,
  onAccentChange,
  theme,
  onThemeChange,
}: AppearanceSettingsProps) {
  const t = useTranslations();

  return (
    <>
      <section className="card" aria-labelledby="appearance-colour">
        <h2 className="card__title" id="appearance-colour">
          {t.settings.appearance.colour.title}
        </h2>
        <div className="swatches" role="radiogroup" aria-labelledby="appearance-colour">
          {accents.map((option) => {
            const name = t.settings.appearance.colours[option];
            return (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={option === accent}
                // The colour is the whole control, so the name it would have
                // carried is said here instead.
                aria-label={
                  option === defaultAccent ? `${name} (${t.settings.appearance.default})` : name
                }
                title={name}
                // The swatch wears the palette it offers, which is what makes
                // the row of colours the row of colours.
                data-accent={option}
                className={option === accent ? "swatch swatch--chosen" : "swatch"}
                onClick={() => onAccentChange(option)}
              >
                {option === accent ? <CheckIcon className="swatch__tick" /> : null}
              </button>
            );
          })}
        </div>
      </section>

      <section className="card" aria-labelledby="appearance-theme">
        <h2 className="card__title" id="appearance-theme">
          {t.settings.appearance.theme.title}
        </h2>
        <div className="segmented" role="radiogroup" aria-labelledby="appearance-theme">
          {themes.map((option) => (
            <button
              key={option}
              type="button"
              role="radio"
              aria-checked={option === theme}
              className={
                option === theme ? "segmented__option is-active" : "segmented__option"
              }
              onClick={() => onThemeChange(option)}
            >
              {t.settings.appearance.theme[option]}
            </button>
          ))}
        </div>
      </section>
    </>
  );
}
