import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { useTranslations } from "./i18n";

/**
 * Puts the wallet on the system tray and keeps its one menu entry in the
 * language the interface is showing.
 *
 * The entry is text a person reads, so it comes from the catalogue on this side
 * and is handed to the backend rather than written there. Running again on a
 * change of language renames the entry instead of building a second tray.
 *
 * Answers whether there is a tray on the bar: false on a phone, which has no
 * bar, and on a desktop where the platform refused one — on Linux, usually
 * because nothing on the desktop is serving a tray. Where it is false, closing
 * the window closes the wallet.
 */
export function useTray(): boolean {
  const t = useTranslations();
  const [installed, setInstalled] = useState(false);

  useEffect(() => {
    let active = true;
    invoke<boolean>("install_tray", { quit: t.tray.quit })
      .then((result) => {
        if (active) {
          setInstalled(result);
        }
      })
      .catch(() => {
        // No native backend behind the webview.
        if (active) {
          setInstalled(false);
        }
      });
    return () => {
      active = false;
    };
  }, [t]);

  return installed;
}
