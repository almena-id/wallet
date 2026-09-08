import { useCallback, useEffect, useState } from "react";
import {
  Format,
  cancel,
  checkPermissions,
  openAppSettings,
  requestPermissions,
  scan,
} from "@tauri-apps/plugin-barcode-scanner";

import { ChevronLeftIcon, QrIcon } from "../components/icons";
import { ScanFramingGuide } from "../components/ScanFramingGuide";
import { useTranslations } from "../i18n";
import type { PlatformInfo } from "../platform";
import { isInvitationLink } from "../invitation";
import { isSignInLink } from "../signin";

type ScanState =
  /** The camera scanner is a mobile-only feature; a computer lands here. */
  | { status: "unsupported" }
  | { status: "starting" }
  | { status: "scanning" }
  | { status: "denied" }
  | { status: "error" }
  | { status: "result"; content: string };

type ScanScreenProps = {
  platform: PlatformInfo;
  /** Leaves the scanner and goes back to the dashboard home. */
  onBack: () => void;
  /**
   * Reports whether the camera preview is live. While it is, the app chrome
   * gets out of the way: the operating system draws the camera behind a
   * transparent webview.
   */
  onPreviewChange: (previewing: boolean) => void;
  /**
   * A scanned `almena://` link the wallet knows what to do with. It is handed
   * on rather than shown here: what it asks for has to be read and checked
   * before anybody is asked to decide, and which screen reads it is decided in
   * one place, beside the links that arrive without a camera.
   */
  onRequest: (link: string) => void;
};

export function ScanScreen({
  platform,
  onBack,
  onPreviewChange,
  onRequest,
}: ScanScreenProps) {
  const t = useTranslations();
  const [state, setState] = useState<ScanState>(
    platform.barcodeScanner ? { status: "starting" } : { status: "unsupported" },
  );
  // Bumped to start a fresh scan, which re-runs the effect below.
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (!platform.barcodeScanner) {
      setState({ status: "unsupported" });
      return;
    }

    let active = true;
    setState({ status: "starting" });

    (async () => {
      // Compared as plain strings: the permission state a plugin reports can
      // be any of granted, denied, prompt or prompt-with-rationale, and only
      // "granted" lets the camera start.
      let permission: string = await checkPermissions();
      if (permission !== "granted") {
        permission = await requestPermissions();
      }
      if (!active) {
        return;
      }
      if (permission !== "granted") {
        setState({ status: "denied" });
        return;
      }

      setState({ status: "scanning" });
      const scanned = await scan({
        windowed: true,
        formats: [Format.QRCode],
        cameraDirection: "back",
      });
      if (!active) {
        return;
      }

      if (isSignInLink(scanned.content) || isInvitationLink(scanned.content)) {
        onRequest(scanned.content);
      } else {
        setState({ status: "result", content: scanned.content });
      }
    })().catch(() => {
      if (active) {
        setState({ status: "error" });
      }
    });

    return () => {
      active = false;
      // Releases the camera whichever way this screen is left.
      cancel().catch(() => {});
    };
  }, [attempt, platform.barcodeScanner]);

  const previewing = state.status === "scanning";

  useEffect(() => {
    onPreviewChange(previewing);
    // The camera is behind the webview, so the page has to stop painting over it.
    document.documentElement.dataset.cameraPreview = previewing ? "true" : "false";
    return () => {
      onPreviewChange(false);
      delete document.documentElement.dataset.cameraPreview;
    };
  }, [previewing, onPreviewChange]);

  const leave = useCallback(() => {
    cancel().catch(() => {});
    onBack();
  }, [onBack]);

  return (
    <div className={previewing ? "screen screen--camera" : "screen"}>
      <header className="screen__header screen__header--scan">
        <button type="button" className="icon-button" onClick={leave} aria-label={t.nav.back}>
          <ChevronLeftIcon />
        </button>
        <h1 className="screen__title screen__title--compact">{t.scan.title}</h1>
      </header>

      {previewing ? (
        <ScanFramingGuide instruction={t.scan.instruction} hint={t.scan.hint} />
      ) : null}

      {state.status === "starting" ? (
        <div className="card card--centered">
          <span className="spinner" aria-hidden="true" />
          <p className="card__title">{t.scan.starting}</p>
        </div>
      ) : null}

      {state.status === "unsupported" ? (
        <div className="card">
          <span className="card__icon">
            <QrIcon />
          </span>
          <h2 className="card__title">{t.scan.unsupported.title}</h2>
          <p className="card__body">{t.scan.unsupported.body}</p>
          <button type="button" className="button button--primary" onClick={onBack}>
            {t.nav.backToHome}
          </button>
        </div>
      ) : null}

      {state.status === "denied" ? (
        <div className="card">
          <h2 className="card__title">{t.scan.permission.deniedTitle}</h2>
          <p className="card__body">{t.scan.permission.deniedBody}</p>
          <div className="button-row">
            <button
              type="button"
              className="button button--primary"
              onClick={() => {
                openAppSettings().catch(() => {});
              }}
            >
              {t.scan.permission.openSettings}
            </button>
            <button type="button" className="button" onClick={() => setAttempt((n) => n + 1)}>
              {t.scan.error.retry}
            </button>
          </div>
        </div>
      ) : null}

      {state.status === "error" ? (
        <div className="card">
          <h2 className="card__title">{t.scan.error.title}</h2>
          <div className="button-row">
            <button
              type="button"
              className="button button--primary"
              onClick={() => setAttempt((n) => n + 1)}
            >
              {t.scan.error.retry}
            </button>
            <button type="button" className="button" onClick={onBack}>
              {t.nav.backToHome}
            </button>
          </div>
        </div>
      ) : null}

      {state.status === "result" ? (
        <div className="card">
          <h2 className="card__title">{t.scan.result.title}</h2>
          <p className="scan-result">{state.content}</p>
          <div className="button-row">
            <button
              type="button"
              className="button button--primary"
              onClick={() => setAttempt((n) => n + 1)}
            >
              {t.scan.result.again}
            </button>
            <button type="button" className="button" onClick={onBack}>
              {t.nav.backToHome}
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
