import { useCallback, useEffect, useState } from "react";

import {
  checkCredentials,
  dueForCheck,
  listCredentials,
  removeCredential,
  type HeldCredential,
} from "../credentials";
import {
  backupDue,
  backupErrorCode,
  dismissBackupReminder,
  exportBackup,
  markBackedUp,
  readBackupMark,
} from "../backup";
import { useTranslations } from "../i18n";
import { useStack } from "../nav";
import { CredentialScreen } from "./CredentialScreen";
import { HomeScreen } from "./HomeScreen";

/** Where the Home tab is: the credentials held, or one of them. */
type View = { name: "home" } | { name: "credential"; id: string };

type HomeTabProps = {
  /** Opens the Scan QR tab, where the build has one. */
  onScan?: () => void;
};

/**
 * The Home tab: the credentials this wallet holds, and each one's detail
 * behind it on a stack (`nav.ts`). The list is read here, once, from the
 * sealed store, and both screens draw from it, so a status checked or a
 * credential removed in the detail is what the list shows on the way back.
 *
 * Opening the tab checks the statuses with each issuer's status list only
 * when one was never read or not for a while (`dueForCheck`) — the tab is
 * opened at every unlock — and the sync button checks them whenever asked.
 */
export function HomeTab({ onScan }: HomeTabProps) {
  const { top, push, pop } = useStack<View>([{ name: "home" }]);
  const [held, setHeld] = useState<HeldCredential[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [checking, setChecking] = useState(false);
  const t = useTranslations();
  // A credential that no backup carries yet: suggested once, from here.
  const [mark, setMark] = useState(readBackupMark);
  const [backingUp, setBackingUp] = useState(false);
  const [backupError, setBackupError] = useState<string | null>(null);
  const backUp = useCallback(async () => {
    setBackingUp(true);
    setBackupError(null);
    try {
      if (await exportBackup()) {
        setMark(markBackedUp());
      }
    } catch (failure) {
      setBackupError(t.settings.backup.errors[backupErrorCode(failure)]);
    } finally {
      setBackingUp(false);
    }
  }, [t]);

  const check = useCallback(() => {
    setChecking(true);
    return checkCredentials()
      .then(setHeld)
      .catch(() => undefined)
      .finally(() => setChecking(false));
  }, []);

  useEffect(() => {
    let live = true;
    listCredentials()
      .then((list) => {
        if (!live) return;
        setHeld(list);
        if (dueForCheck(list)) void check();
      })
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [check]);

  const remove = useCallback(async (id: string) => {
    setHeld(await removeCredential(id));
    pop();
  }, [pop]);

  if (top.name === "credential") {
    const credential = held?.find((one) => one.id === top.id);
    if (credential) {
      return (
        <CredentialScreen
          credential={credential}
          checking={checking}
          onCheck={() => void check()}
          onRemove={() => remove(credential.id)}
          onBack={pop}
        />
      );
    }
  }

  return (
    <HomeScreen
      held={held}
      failed={failed}
      checking={checking}
      onCheck={() => void check()}
      onOpen={(id) => push({ name: "credential", id })}
      onScan={onScan}
      backup={
        held !== null && backupDue(held, mark)
          ? {
              busy: backingUp,
              error: backupError,
              onBackUp: () => void backUp(),
              onDismiss: () => setMark(dismissBackupReminder()),
            }
          : undefined
      }
    />
  );
}
