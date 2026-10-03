import { useCallback, useEffect, useRef, useState } from "react";
import {
  Format,
  cancel,
  checkPermissions,
  openAppSettings,
  requestPermissions,
  scan,
} from "@tauri-apps/plugin-barcode-scanner";

import { ScanFramingGuide } from "../components/ScanFramingGuide";
import { useTranslations } from "../i18n";
import { invitationKind } from "../links";
import { BackButton } from "../components/BackButton";

type ScanState =
  | { status: "starting" }
  | { status: "scanning" }
  | { status: "denied" }
  | { status: "error" }
  | { status: "result"; content: string };

type ScanScreenProps = {
  /** Leaves the scanner for the home screen. */
  onBack: () => void;
  /**
   * Reports whether the camera preview is live. While it is, the app chrome
   * gets out of the way: the operating system draws the camera behind a
   * transparent webview, and the menu and the backdrop would paint over it.
   */
  onPreviewChange: (previewing: boolean) => void;
  /**
   * Hands a code that reads as an invitation — somebody's, or a mediator's —
   * to the screen that puts it to the person. Nothing is accepted here.
   */
  onInvitation: (content: string) => void;
};

/**
 * The camera, pointed at a code.
 *
 * Only reached where the scanner exists: the tab that opens this screen is
 * offered on a phone or a tablet and nowhere else, because the answer comes
 * from the same switch on the Rust side that registered the plugin. A code the
 * wallet knows is carried, as read, to the one screen that asks about it; the
 * code itself is shown here only when it reads as nothing the wallet knows,
 * because then that is all there is to show.
 */
export function ScanScreen({ onBack, onPreviewChange, onInvitation }: ScanScreenProps) {
  const t = useTranslations();
  const [state, setState] = useState<ScanState>({ status: "starting" });
  // Bumped to start a fresh scan, which re-runs the effect below.
  const [attempt, setAttempt] = useState(0);
  // Read through a ref so that a parent re-rendering with a fresh callback
  // does not restart the camera: the effect below is keyed on `attempt` alone.
  const handOff = useRef(onInvitation);
  handOff.current = onInvitation;

  useEffect(() => {
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
      const content = scanned.content;
      if ((await invitationKind(content)) !== "unknown") {
        if (active) {
          handOff.current(content);
        }
        return;
      }
      if (active) {
        setState({ status: "result", content });
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
  }, [attempt]);

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
      <header className="screen__header screen__header--compact">
        <BackButton onBack={leave} />
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
          </div>
        </div>
      ) : null}

      {state.status === "result" ? (
        <div className="card">
          <h2 className="card__title">{t.scan.result.title}</h2>
          <p className="card__body">{t.scan.result.unknown}</p>
          <p className="identifier">{state.content}</p>
          <div className="button-row">
            <button
              type="button"
              className="button button--primary"
              onClick={() => setAttempt((n) => n + 1)}
            >
              {t.scan.result.again}
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
