import { useCallback, useEffect, useState } from "react";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import {
  checkPermissions,
  openAppSettings,
  requestPermissions,
} from "@tauri-apps/plugin-barcode-scanner";
import { checkStatus } from "@tauri-apps/plugin-biometric";

import { useTranslations } from "../../i18n";
import type { PlatformInfo } from "../../platform";

/**
 * Every answer a permission can give, including "this platform has no such
 * thing" and "there is nothing on this device to ask with".
 *
 * `unenrolled` is the one that is not a permission at all: a phone with a face
 * reader and no face set up on it has refused nothing, and telling somebody
 * their wallet was denied would send them looking for a switch that is not
 * there.
 */
type Status = "unknown" | "granted" | "denied" | "prompt" | "unenrolled" | "unsupported";

type PermissionsSettingsProps = {
  platform: PlatformInfo;
};

/**
 * What this device lets the wallet do.
 *
 * Each row asks the platform rather than remembering: a permission taken away in
 * the system settings has to read as taken away here, and nothing the wallet
 * stored would have known.
 */
export function PermissionsSettings({ platform }: PermissionsSettingsProps) {
  const t = useTranslations();
  const [notifications, setNotifications] = useState<Status>("unknown");
  const [camera, setCamera] = useState<Status>("unknown");
  const [biometrics, setBiometrics] = useState<Status>("unknown");

  const readCamera = useCallback(async () => {
    if (!platform.barcodeScanner) {
      setCamera("unsupported");
      return;
    }
    try {
      const state = await checkPermissions();
      setCamera(state === "granted" ? "granted" : state === "denied" ? "denied" : "prompt");
    } catch {
      setCamera("unsupported");
    }
  }, [platform.barcodeScanner]);

  /**
   * Whether this device can recognise the person holding it.
   *
   * There is nothing to request: no platform asks for this ahead of time — the
   * system puts its own prompt up the first time something needs it. What the
   * row can say is whether it would work, which is what somebody wondering why
   * the switch in Security is greyed out has come here to find out.
   */
  const readBiometrics = useCallback(async () => {
    if (platform.kind !== "mobile") {
      setBiometrics("unsupported");
      return;
    }
    try {
      const state = await checkStatus();
      if (state.isAvailable) {
        setBiometrics("granted");
      } else if (
        state.errorCode === "biometryNotEnrolled" ||
        state.errorCode === "passcodeNotSet"
      ) {
        setBiometrics("unenrolled");
      } else {
        setBiometrics("denied");
      }
    } catch {
      setBiometrics("unsupported");
    }
  }, [platform.kind]);

  useEffect(() => {
    let active = true;
    isPermissionGranted()
      .then((granted) => {
        if (active) {
          setNotifications(granted ? "granted" : "prompt");
        }
      })
      .catch(() => {
        // No native backend behind the webview.
        if (active) {
          setNotifications("unsupported");
        }
      });
    void readCamera();
    void readBiometrics();
    return () => {
      active = false;
    };
  }, [readCamera, readBiometrics]);

  async function askForNotifications() {
    try {
      const permission = await requestPermission();
      setNotifications(permission === "granted" ? "granted" : "denied");
    } catch {
      setNotifications("unsupported");
    }
  }

  async function askForCamera() {
    try {
      const state = await requestPermissions();
      setCamera(state === "granted" ? "granted" : "denied");
    } catch {
      setCamera("unsupported");
    }
  }

  const labels: Record<Status, string> = {
    unknown: "…",
    granted: t.settings.permissions.granted,
    denied: t.settings.permissions.denied,
    prompt: t.settings.permissions.prompt,
    unenrolled: t.settings.permissions.unenrolled,
    unsupported: t.settings.permissions.unsupported,
  };

  function row(
    id: string,
    title: string,
    status: Status,
    // Absent where there is nothing to ask for: biometrics is offered by the
    // system when it is needed, never requested in advance.
    ask: (() => void) | null,
    canOpenSettings: boolean,
  ) {
    return (
      <section className="card" aria-labelledby={id}>
        <h2 className="card__title" id={id}>
          {title}
        </h2>
        <dl className="detail-list">
          <div className="detail-list__row">
            <dt>{t.settings.permissions.statusLabel}</dt>
            <dd>{labels[status]}</dd>
          </div>
        </dl>
        {status === "denied" ? (
          <p className="card__note">{t.settings.permissions.deniedHint}</p>
        ) : null}
        {status === "prompt" && ask ? (
          <div className="button-row">
            <button type="button" className="button button--primary" onClick={ask}>
              {t.settings.permissions.request}
            </button>
          </div>
        ) : null}
        {status === "denied" && canOpenSettings ? (
          <div className="button-row">
            <button
              type="button"
              className="button"
              onClick={() => {
                openAppSettings().catch(() => {});
              }}
            >
              {t.settings.permissions.openSettings}
            </button>
          </div>
        ) : null}
      </section>
    );
  }

  return (
    <>
      {row(
        "permission-notifications",
        t.settings.permissions.notifications.title,
        notifications,
        () => {
          void askForNotifications();
        },
        false,
      )}
      {row(
        "permission-camera",
        t.settings.permissions.camera.title,
        camera,
        () => {
          void askForCamera();
        },
        // Only the mobile plugin can open the page that holds the switch.
        platform.barcodeScanner,
      )}
      {row(
        "permission-biometrics",
        t.settings.permissions.biometrics.title,
        biometrics,
        null,
        platform.barcodeScanner,
      )}
    </>
  );
}
