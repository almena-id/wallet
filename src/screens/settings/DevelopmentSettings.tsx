import { useCallback, useEffect, useState } from "react";

import {
  errorCode,
  readLogs,
  saveLogsTo,
  shareLogs,
  type DevelopErrorCode,
  type Logs,
} from "../../develop";
import { fill, useI18n } from "../../i18n";
import { platformLabel } from "../../labels";
import type { PlatformInfo } from "../../platform";

type DevelopmentSettingsProps = {
  platform: PlatformInfo;
};

/**
 * The name the save dialog offers, which is the name the copy then carries.
 *
 * Not a catalogue string: a file name is an identifier, and this codebase
 * writes identifiers in English on every platform and in every language.
 */
const FILE_NAME = "almena-wallet-log.txt";

/**
 * How much of the log there is, said in the unit that takes the fewest digits.
 *
 * The unit is chosen here and the wording of it is not: a separator is a comma
 * in one language and a full stop in another, and the abbreviation is not the
 * same either. A catalogue string with a number glued into it would get one of
 * those wrong in every language it was not written for.
 *
 * Divided by a thousand and not by 1024, because the unit `Intl` writes is the
 * decimal one — calling 1024 bytes a kilobyte is off by exactly the amount the
 * label claims it is not.
 */
function logSize(bytes: number, locale: string): string {
  const units = ["byte", "kilobyte", "megabyte", "gigabyte"];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }

  return new Intl.NumberFormat(locale, {
    style: "unit",
    unit: units[unit],
    unitDisplay: "narrow",
    maximumFractionDigits: unit === 0 ? 0 : 1,
  }).format(value);
}

/**
 * What this build is and what it can do where it is running.
 *
 * It answers from the Rust side rather than from the user agent, which is why
 * it is worth having: the same list of features is what decides whether the
 * scanner, the tray or the window state exist at all on this platform.
 *
 * The log is here for the same reason. **Somebody holding a phone cannot attach
 * a debugger to it**, so the only account of something going wrong is what the
 * wallet wrote down, and this is the screen that reads it out and hands it over.
 */
export function DevelopmentSettings({ platform }: DevelopmentSettingsProps) {
  const { locale, t } = useI18n();
  const [logs, setLogs] = useState<Logs | null>(null);
  const [saved, setSaved] = useState(false);
  const [trouble, setTrouble] = useState<DevelopErrorCode | null>(null);

  /**
   * What the log says about itself before it says anything else.
   *
   * Four sentences and not one string, because a translator should see
   * sentences. The date goes through `Intl`: a log exported "on 4/9/2026" means
   * two different days depending on who reads it.
   */
  function header(): string {
    const when = new Intl.DateTimeFormat(locale, {
      dateStyle: "long",
      timeStyle: "short",
    }).format(new Date());
    const lines = t.settings.development.logs.exportHeader;
    return [fill(lines.what, { when }), lines.safe, lines.cost, lines.care].join("\n");
  }

  const features = [
    { label: t.settings.development.device.featureScanner, on: platform.barcodeScanner },
    { label: t.settings.development.device.featureNotifications, on: platform.kind !== "unknown" },
    { label: t.settings.development.device.featureWindowState, on: platform.windowState },
    { label: t.settings.development.device.featureSingleInstance, on: platform.singleInstance },
    { label: t.settings.development.device.featureTray, on: platform.tray },
    { label: t.settings.development.device.featureDeepLink, on: platform.deepLink },
  ];

  const refresh = useCallback(async () => {
    try {
      setLogs(await readLogs());
      setSaved(false);
      setTrouble(null);
    } catch (error: unknown) {
      setTrouble(errorCode(error));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function shareLog() {
    try {
      await shareLogs(header(), t.settings.development.logs.shareTitle);
      setTrouble(null);
    } catch (error: unknown) {
      const code = errorCode(error);
      // Closing the sheet is not a failure and does not get a message. The
      // wallet saying "that did not work" to somebody who changed their mind is
      // the wallet being wrong about what happened.
      setTrouble(code === "develop_cancelled" ? null : code);
    }
  }

  async function saveCopy() {
    let destination: string | null = null;
    try {
      // Asked for here rather than at the top of the file, the way the reveal
      // already is: it is loaded when somebody presses the button and not
      // before.
      const { save } = await import("@tauri-apps/plugin-dialog");
      destination = await save({
        defaultPath: FILE_NAME,
        filters: [{ name: t.settings.development.logs.fileKind, extensions: ["txt"] }],
      });
    } catch (error: unknown) {
      setTrouble(errorCode(error));
      return;
    }
    // A dismissed dialog answers with nothing, and nothing is not an error.
    if (destination === null) {
      return;
    }
    try {
      await saveLogsTo(destination, header());
      setSaved(true);
      setTrouble(null);
    } catch (error: unknown) {
      setTrouble(errorCode(error));
    }
  }

  async function reveal(path: string) {
    try {
      // Asked for here rather than at the top of the file so that a phone build
      // never loads it: a folder to open is a thing only a computer has.
      const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
      await revealItemInDir(path);
    } catch (error: unknown) {
      setTrouble(errorCode(error));
    }
  }

  return (
    <>
      <section className="card" aria-labelledby="development-about">
        <h2 className="card__title" id="development-about">
          {t.settings.development.about.title}
        </h2>
        <dl className="detail-list">
          <div className="detail-list__row">
            <dt>{t.settings.development.about.platformLabel}</dt>
            <dd>{platformLabel(t, platform)}</dd>
          </div>
          {platform.version ? (
            <div className="detail-list__row">
              <dt>{t.settings.development.about.versionLabel}</dt>
              <dd>{platform.version}</dd>
            </div>
          ) : null}
        </dl>
      </section>

      <section className="card" aria-labelledby="development-device">
        <h2 className="card__title" id="development-device">
          {t.settings.development.device.title}
        </h2>
        {/* The platform and the version are the card above; what is left is what
            this build actually carries here. */}
        <p className="card__subtitle">{t.settings.development.device.featuresLabel}</p>
        <ul className="feature-list">
          {features.map((feature) => (
            <li key={feature.label} className={feature.on ? "feature feature--on" : "feature"}>
              <span className="feature__dot" aria-hidden="true" />
              <span className="feature__label">{feature.label}</span>
              <span className="feature__state">
                {feature.on
                  ? t.settings.development.device.featureOn
                  : t.settings.development.device.featureOff}
              </span>
            </li>
          ))}
        </ul>
      </section>

      <section className="card" aria-labelledby="development-logs">
        <h2 className="card__title" id="development-logs">
          {t.settings.development.logs.title}
        </h2>

        {logs !== null ? (
          <>
            <dl className="detail-list">
              {/* The directory is worth spelling out even when there is nothing
                  in it yet: it is where to look once something has gone wrong. */}
              <div className="detail-list__row">
                <dt>{t.settings.development.logs.locationLabel}</dt>
                <dd>{logs.directory}</dd>
              </div>
              <div className="detail-list__row">
                <dt>{t.settings.development.logs.filesLabel}</dt>
                <dd>{new Intl.NumberFormat(locale).format(logs.files.length)}</dd>
              </div>
              <div className="detail-list__row">
                <dt>{t.settings.development.logs.sizeLabel}</dt>
                <dd>{logSize(logs.totalBytes, locale)}</dd>
              </div>
            </dl>

            {logs.reach.browsable ? null : (
              <p className="card__note">{t.settings.development.logs.notBrowsable}</p>
            )}

            {logs.files.length === 0 ? (
              <p className="card__note">{t.settings.development.logs.empty}</p>
            ) : (
              <>
                <div className="button-row">
                  {/* A folder to open is a thing only a computer has, and
                      `browsable` is true on iOS as well — where Files reaches
                      the directory but no plugin call opens it. Both, then. */}
                  {platform.kind === "desktop" && logs.reach.browsable ? (
                    <button
                      type="button"
                      className="button"
                      onClick={() => {
                        void reveal(logs.directory);
                      }}
                    >
                      {t.settings.development.logs.reveal}
                    </button>
                  ) : null}
                  {logs.reach.shareable ? (
                    <button
                      type="button"
                      className="button button--primary"
                      onClick={() => {
                        void shareLog();
                      }}
                    >
                      {t.settings.development.logs.share}
                    </button>
                  ) : null}
                  {logs.reach.savable ? (
                    <button
                      type="button"
                      className={logs.reach.shareable ? "button" : "button button--primary"}
                      onClick={() => {
                        void saveCopy();
                      }}
                    >
                      {t.settings.development.logs.save}
                    </button>
                  ) : null}
                </div>
                {saved ? (
                  <p className="card__note">{t.settings.development.logs.saved}</p>
                ) : null}
              </>
            )}
          </>
        ) : null}

        {trouble !== null ? (
          <p className="card__note">{t.settings.development.logs.errors[trouble]}</p>
        ) : null}
      </section>
    </>
  );
}
