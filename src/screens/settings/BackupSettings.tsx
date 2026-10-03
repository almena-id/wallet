import { useState } from "react";

import { backupErrorCode, exportBackup, importBackup } from "../../backup";
import { useTranslations } from "../../i18n";
import { fill } from "../../i18n/format";

/**
 * The encrypted backup: one file with what the phrase does not bring back —
 * contacts, conversations, credentials, the name and the picture — that only
 * this identity's words open. Restoring adds what is missing and changes
 * nothing that is there.
 */
export function BackupSettings() {
  const t = useTranslations();
  const copy = t.settings.backup;
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = async (action: () => Promise<string | null>) => {
    setBusy(true);
    setNote(null);
    setError(null);
    try {
      setNote(await action());
    } catch (failure) {
      setError(copy.errors[backupErrorCode(failure)]);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <section className="card">
        <h2 className="card__title">{copy.exportTitle}</h2>
        <p className="card__body">{copy.exportBody}</p>
        <div className="button-row">
          <button
            type="button"
            className="button button--primary"
            disabled={busy}
            onClick={() => void run(async () => ((await exportBackup()) ? copy.exported : null))}
          >
            {copy.export}
          </button>
        </div>
      </section>

      <section className="card">
        <h2 className="card__title">{copy.importTitle}</h2>
        <p className="card__body">{copy.importBody}</p>
        <div className="button-row">
          <button
            type="button"
            className="button"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const restored = await importBackup();
                return restored === null
                  ? null
                  : fill(copy.imported, {
                      contacts: restored.contacts,
                      messages: restored.messages,
                      credentials: restored.credentials,
                    });
              })
            }
          >
            {copy.import}
          </button>
        </div>
      </section>

      {note ? <p className="card__note">{note}</p> : null}
      {error ? <p className="card__note card__note--warning">{error}</p> : null}
    </>
  );
}
