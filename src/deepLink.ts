import { useCallback, useEffect, useState } from "react";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";

/**
 * The `almena://` links the wallet is opened with.
 *
 * Two ways in, and both end here: a link that started the wallet, which is
 * waiting in `getCurrent` by the time the interface loads, and a link that
 * arrives while it is already running.
 *
 * **What arrives is shown, never acted on.** A link comes from outside the
 * wallet — a page, a message, anything that can put a URL in front of somebody —
 * and nothing outside gets to tell the wallet what to do with the identity it
 * holds. Deciding what an `almena://` link may ask for is a decision of its own.
 */
export function useDeepLink(): { url: string | null; clear: () => void } {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;

    // The link the wallet was started with, if it was started by one.
    getCurrent()
      .then((urls) => {
        if (active && urls && urls.length > 0) {
          setUrl(urls[0]);
        }
      })
      .catch(() => {
        // No native backend behind the webview.
      });

    onOpenUrl((urls) => {
      if (active && urls.length > 0) {
        setUrl(urls[0]);
      }
    })
      .then((unlisten) => {
        if (active) {
          stop = unlisten;
        } else {
          unlisten();
        }
      })
      .catch(() => {});

    return () => {
      active = false;
      stop?.();
    };
  }, []);

  const clear = useCallback(() => setUrl(null), []);

  return { url, clear };
}
