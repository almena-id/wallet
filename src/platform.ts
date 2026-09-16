import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type PlatformKind = "desktop" | "mobile" | "unknown";

/**
 * What the Rust side reports about the host. It is the authority on which
 * plugins this build actually registered, because it answers from the same
 * compile-time switches that registered them.
 */
export type PlatformInfo = {
  kind: PlatformKind;
  os: string;
  barcodeScanner: boolean;
  windowState: boolean;
  singleInstance: boolean;
  tray: boolean;
  version: string;
};

/**
 * Used until the backend answers, and when there is no backend at all — the
 * web layer also runs on its own under `task dev:web`, where no native
 * feature is available.
 */
export const unknownPlatform: PlatformInfo = {
  kind: "unknown",
  os: "",
  barcodeScanner: false,
  windowState: false,
  singleInstance: false,
  tray: false,
  version: "",
};

export function usePlatform(): { platform: PlatformInfo; resolved: boolean } {
  const [platform, setPlatform] = useState<PlatformInfo>(unknownPlatform);
  const [resolved, setResolved] = useState(false);

  useEffect(() => {
    let active = true;
    invoke<PlatformInfo>("platform_info")
      .then((info) => {
        if (active) {
          setPlatform(info);
        }
      })
      .catch(() => {
        // No Tauri backend: the web layer is running in a plain browser.
      })
      .finally(() => {
        if (active) {
          setResolved(true);
        }
      });
    return () => {
      active = false;
    };
  }, []);

  return { platform, resolved };
}
