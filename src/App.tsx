import { useCallback, useEffect, useState } from "react";

import { LiquidTabBar, type TabDefinition } from "./components/LiquidTabBar";
import { BrandSpinner } from "./components/BrandSpinner";
import { HomeIcon, MessagesIcon, QrIcon, SettingsIcon, SyncIcon } from "./components/icons";
import { plural, useI18n } from "./i18n";
import { useAccent } from "./appearance";
import { useAutoLock, useIdle } from "./autolock";
import { useBackdrop } from "./backdrop";
import { useBackInSight } from "./lock";
import { useMediator } from "./mediator";
import { errorCode, threadsOf, useMessaging, type Messaging } from "./messaging";
import { usePlatform } from "./platform";
import { useTheme } from "./theme";
import { useTray } from "./tray";
import { forgetIdentity, type Identity } from "./identity";
import {
  destroyVault,
  errorCode as vaultErrorCode,
  openVault,
  openVaultWithDevice,
  useVault,
} from "./vault";
import { HomeScreen } from "./screens/HomeScreen";
import { InviteScreen } from "./screens/InviteScreen";
import { PinChange } from "./screens/PinChange";
import { PinConfirm } from "./screens/PinConfirm";
import { PinScreen } from "./screens/PinScreen";
import { LogoutScreen } from "./screens/LogoutScreen";
import { CredentialRequestScreen } from "./screens/CredentialRequestScreen";
import { InvitationScreen } from "./screens/InvitationScreen";
import { RequestStartScreen } from "./screens/RequestStartScreen";
import { MessagesScreen } from "./screens/MessagesScreen";
import { ScanScreen } from "./screens/ScanScreen";
import { ThreadScreen } from "./screens/ThreadScreen";
import { Onboarding } from "./screens/onboarding/Onboarding";
import { SettingsScreen } from "./screens/settings/SettingsScreen";

/**
 * `logout` is not a tab: it is what Settings opens to ask whether somebody
 * means it. `invite` is where the home screen's own card leads, to show the
 * invitation as a code. `pin` and `device` are the two things Security sends
 * somebody to. All of them are left through their own back button.
 * `messages` is a tab, the inbox; `thread` is where one of its rows
 * leads, and `invitation` is where a new relationship is opened — from the
 * inbox's own button, or from the scanner with a code that reads as one.
 * `requestStart` and `request` are where the scanner leads with the
 * marketplace's two codes: the first puts the request to the person before
 * it is started, the second shows the filled form before it is sent.
 */
type Route =
  | "home"
  | "messages"
  | "thread"
  | "invitation"
  | "requestStart"
  | "request"
  | "scan"
  | "settings"
  | "invite"
  | "logout"
  | "pin"
  | "device";

export default function App() {
  const { t, locale } = useI18n();
  const { platform } = usePlatform();
  // Called for the tray it puts on the bar, not for what it answers: the wallet
  // no longer has anything to say about the tray, but closing the window still
  // means "put away" wherever one was installed.
  useTray();
  const { accent, setAccent } = useAccent();
  const { autoLock, setAutoLock } = useAutoLock();
  const { mediator, setMediator } = useMediator();
  const { theme, setTheme } = useTheme();
  const vault = useVault();
  const [identity, setIdentity] = useState<Identity | null>(null);
  // Readable exactly while an identity is open: the book is sealed under a
  // key the seed derives, and the seed leaves with the lock.
  const messaging = useMessaging(identity !== null);
  const [route, setRoute] = useState<Route>("home");
  // Where Settings opens: back where somebody was, when they are coming back
  // from a screen a section sent them to.
  const [settingsSection, setSettingsSection] = useState<"security" | null>(null);
  const [cameraPreview, setCameraPreview] = useState(false);
  // Something the scanner read that looks like an invitation, carried to the
  // invitation screen for the person to open — or not — there.
  const [scannedInvitation, setScannedInvitation] = useState<string | null>(null);
  // The marketplace's first code the scanner read, carried to the screen that
  // puts the request to the person — and starts it only if they say so.
  const [scannedStart, setScannedStart] = useState<string | null>(null);
  // The second code the scanner read, carried to the screen that shows what
  // would be sent — and sends it only if the person says so.
  const [scannedRequest, setScannedRequest] = useState<string | null>(null);
  // The thread that is open, by key.
  const [openThread, setOpenThread] = useState<string | null>(null);
  // What the last collection asked for by hand brought, and what stopped it.
  // Said in the inbox, which is where what arrived is read.
  const [syncNote, setSyncNote] = useState<string | null>(null);
  const [syncError, setSyncError] = useState<string | null>(null);
  // The window behind the page wears the same colour the page does, so turning
  // the device does not flash the native white through — see `backdrop`. Read
  // after `useTheme` above, because it reads the palette that hook just applied.
  useBackdrop(theme, cameraPreview);
  const [unlockError, setUnlockError] = useState<string | null>(null);
  // Signing out is reachable from behind the lock and from a record that cannot
  // be opened, neither of which has a Settings to route through.
  const [signOutAsked, setSignOutAsked] = useState(false);
  const [unlockBusy, setUnlockBusy] = useState(false);

  // **Locking is letting go, not hiding.** There is no flag that says the wallet
  // is closed while the seed sits behind it in memory: the lock drops the
  // identity, and coming back opens the record again with a PIN or a face. What
  // is written on the device stays written, and the seed the wallet was signing
  // with is no longer in this process at all — which is a stronger thing to say
  // than that a screen is over it, and it costs one derivation to undo.
  const lockNow = useCallback(() => {
    setIdentity((open) => {
      if (open) {
        void forgetIdentity();
      }
      return null;
    });
    setRoute("home");
  }, []);

  // **One clock, for every way of not using the wallet.** Left open on a desk,
  // left behind for another app, left on the tray, left under the system's own
  // Face ID prompt — none of them is somebody using a wallet, and none of them
  // is somebody who is not coming back. What decides is the length they chose,
  // and it goes on counting while the wallet is off the screen.
  //
  // Armed only while a wallet is open: there is nothing to let go of behind the
  // lock, and a clock running there would be counting nothing.
  const catchUp = useIdle(autoLock, identity !== null, lockNow);

  // Coming back is not what locks; it is when the clock is asked the time, for a
  // webview whose timer the system throttled or froze while nobody could see it.
  useBackInSight(catchUp);

  // Signing out is the other thing entirely: the record itself goes, and the
  // the phrase is what is left.
  const signOut = useCallback(async () => {
    setRoute("home");
    setSignOutAsked(false);
    setIdentity(null);
    setUnlockError(null);
    void forgetIdentity();
    vault.adopt(await destroyVault().catch(() => vault.status));
  }, [vault]);

  const unlock = useCallback(
    async (open: () => Promise<Identity>) => {
      setUnlockError(null);
      setUnlockBusy(true);
      try {
        setIdentity(await open());
      } catch (failure) {
        setUnlockError(t.vault.errors[vaultErrorCode(failure)]);
        // The count of what is left changed, and a record spent to its last
        // attempt is gone — which the welcome screen has to be told about.
        void vault.refresh();
      } finally {
        setUnlockBusy(false);
      }
    },
    [t, vault],
  );

  // **The mailboxes are emptied from one place, whatever screen is open.** A
  // wallet on a phone is not listening, it asks; the clock asks while the
  // wallet is open, and this is the person asking. The button that does it is
  // pinned to the shell rather than to the inbox, because what it brings —
  // an answer about a request — matters on every screen, not only on the one
  // that lists it.
  const sync = useCallback(async () => {
    setSyncNote(null);
    setSyncError(null);
    try {
      const collected = await messaging.collect();
      const parts = [
        collected.received === 0
          ? t.messages.sync.none
          : plural(t.messages.sync.received, collected.received, locale),
      ];
      if (collected.unreachable.length > 0) {
        parts.push(plural(t.messages.sync.unreachable, collected.unreachable.length, locale));
      }
      setSyncNote(parts.join(" · "));
    } catch (failure) {
      setSyncError(t.messages.errors[errorCode(failure)]);
    }
  }, [messaging, t, locale]);

  const goHome = useCallback(() => setRoute("home"), []);
  const backToInbox = useCallback(() => {
    setScannedInvitation(null);
    setRoute("messages");
  }, []);
  const backToSecurity = useCallback(() => {
    setSettingsSection("security");
    setRoute("settings");
  }, []);

  // **Scanning is offered only where it can happen.** A computer has no camera
  // the wallet may drive, and a tab there would be a tab that only ever leads
  // to an apology. The answer comes from the Rust side, which knows because it
  // is the same switch that decided whether to register the scanner at all.
  const tabs: TabDefinition<Route>[] = [
    { id: "home", label: t.nav.home, icon: <HomeIcon /> },
    { id: "messages", label: t.nav.messages, icon: <MessagesIcon /> },
    ...(platform.barcodeScanner
      ? [{ id: "scan" as const, label: t.nav.scan, icon: <QrIcon /> }]
      : []),
    { id: "settings", label: t.nav.settings, icon: <SettingsIcon /> },
  ];

  // Nothing is drawn on a guess. Until the device has said whether it is holding
  // an identity, showing the welcome screen would tell somebody who has one that
  // they do not.
  if (!vault.read) {
    return (
      <div className="app">
        <main className="app__view app__view--plain">
          <BrandSpinner label={t.app.name} />
        </main>
      </div>
    );
  }

  // Somebody wanting out from behind the lock, or from a record that cannot be
  // opened. The same screen, and the same ten seconds, as from Settings.
  if (signOutAsked && !identity) {
    return (
      <div className="app">
        <main className="app__view app__view--plain">
          <LogoutScreen onBack={() => setSignOutAsked(false)} onConfirmed={() => void signOut()} />
        </main>
      </div>
    );
  }

  // There is something where the record goes and this wallet cannot read it. The
  // one thing that must not happen here is the welcome screen: offering to create
  // an identity would write a second one over the first.
  if (vault.status.problem) {
    return (
      <div className="app">
        <main className="app__view app__view--plain">
          <div className="screen">
            <header className="screen__header">
              <h1 className="screen__title">{t.vault.problem.title}</h1>
            </header>
            <p className="screen__intro">{t.vault.errors[vault.status.problem]}</p>
            <section className="card">
              <p className="card__body">{t.vault.problem.body}</p>
              <div className="button-row">
                <button
                  type="button"
                  className="button button--primary"
                  onClick={() => void vault.refresh()}
                >
                  {t.vault.problem.retry}
                </button>
                <button
                  type="button"
                  className="button button--danger"
                  onClick={() => setSignOutAsked(true)}
                >
                  {t.settings.security.signOut}
                </button>
              </div>
            </section>
          </div>
        </main>
      </div>
    );
  }

  // No identity on this device: the way in is the only thing there is.
  if (!vault.status.exists) {
    return (
      <div className="app">
        <main className="app__view app__view--plain">
          <Onboarding
            onReady={(made) => {
              setIdentity(made);
              void vault.refresh();
            }}
          />
        </main>
      </div>
    );
  }

  // There is one, and it is not open. The only ways past are the PIN, a face
  // where the system will vouch for one, and signing out to start from the words.
  if (!identity) {
    return (
      <div className="app">
        <main className="app__view app__view--plain">
          <PinScreen
            title={t.pin.unlockTitle}
            subtitle={t.pin.unlockSubtitle}
            digits={vault.status.digits ?? 4}
            error={unlockError}
            busy={unlockBusy}
            busyLabel={t.pin.checking}
            onBiometrics={
              vault.status.deviceKey
                ? () => {
                    void unlock(openVaultWithDevice);
                  }
                : undefined
            }
            onComplete={(code) => {
              void unlock(() => openVault(code));
            }}
            footer={
              <>
                {/* Said only once it is worth saying. A wallet that counts down
                    from ten at every launch is a wallet that reads as broken;
                    one that says nothing lets somebody destroy an identity by
                    guessing. */}
                {vault.status.attemptsLeft <= 3 ? (
                  <p className="card__note card__note--warning">
                    {plural(t.pin.attemptsLeft, vault.status.attemptsLeft, locale)}
                  </p>
                ) : null}
                {/* Behind the lock this is still the button that destroys an
                    identity, so it asks the same question Settings asks rather
                    than doing it on one tap. */}
                <button
                  type="button"
                  className="button button--danger"
                  onClick={() => setSignOutAsked(true)}
                >
                  {t.settings.security.signOut}
                </button>
              </>
            }
          />
        </main>
      </div>
    );
  }

  // Screens with no bar at the foot of them: the room the layout keeps for one
  // is otherwise a band of nothing under a keypad, and it is what stops the
  // keypad reaching the bottom of the screen at all.
  const barless = route === "logout" || route === "pin" || route === "device";

  // The collection button keeps the menu's company: not over a camera, and not
  // over a keypad or a question, where a stray control is one somebody taps
  // past by accident. The scanner hides it whether or not its preview is live,
  // so nothing sits in the corner of what the camera frames.
  const syncShown = route !== "scan" && !cameraPreview && !barless;

  return (
    <div className={[cameraPreview ? "app app--camera" : "app", syncShown ? "app--sync" : ""].join(" ").trim()}>
      <main
        className={barless && !cameraPreview ? "app__view app__view--plain" : "app__view"}
        key={route}
      >
        {route === "home" ? <HomeScreen onShowCode={() => setRoute("invite")} /> : null}
        {route === "invite" ? <InviteScreen mediator={mediator} onBack={goHome} /> : null}
        {route === "messages" ? (
          <MessagesScreen
            messaging={messaging}
            syncNote={syncNote}
            syncError={syncError}
            onOpenThread={(key) => {
              setOpenThread(key);
              setRoute("thread");
            }}
            onNewRelationship={() => setRoute("invitation")}
          />
        ) : null}
        {route === "thread" ? (
          <OpenThread messaging={messaging} threadKey={openThread} onBack={backToInbox} />
        ) : null}
        {route === "invitation" ? (
          <InvitationScreen
            messaging={messaging}
            mediator={mediator}
            initialInvitation={scannedInvitation}
            onBack={backToInbox}
          />
        ) : null}
        {route === "requestStart" && scannedStart !== null ? (
          <RequestStartScreen
            messaging={messaging}
            mediator={mediator}
            code={scannedStart}
            onCancel={() => {
              setScannedStart(null);
              goHome();
            }}
            onContinued={() => {
              // The page that showed the code is on to the form now, and the
              // form comes back as a second code: the camera is the next step.
              setScannedStart(null);
              setRoute("scan");
            }}
          />
        ) : null}
        {route === "request" && scannedRequest !== null ? (
          <CredentialRequestScreen
            messaging={messaging}
            mediator={mediator}
            code={scannedRequest}
            onBack={() => {
              setScannedRequest(null);
              setRoute("scan");
            }}
            onSent={(sent) => {
              setScannedRequest(null);
              setOpenThread(sent.thread);
              setRoute("thread");
            }}
          />
        ) : null}
        {route === "scan" ? (
          <ScanScreen
            onBack={goHome}
            onPreviewChange={setCameraPreview}
            onOpenRelationship={(content) => {
              setScannedInvitation(content);
              setRoute("invitation");
            }}
            onRequestStart={(content) => {
              setScannedStart(content);
              setRoute("requestStart");
            }}
            onCredentialRequest={(content) => {
              setScannedRequest(content);
              setRoute("request");
            }}
          />
        ) : null}
        {route === "settings" ? (
          <SettingsScreen
            platform={platform}
            accent={accent}
            onAccentChange={setAccent}
            theme={theme}
            onThemeChange={setTheme}
            autoLock={autoLock}
            onAutoLockChange={setAutoLock}
            mediator={mediator}
            onMediatorChange={setMediator}
            vault={vault}
            initialSection={settingsSection}
            onChangePin={() => setRoute("pin")}
            onArmDevice={() => setRoute("device")}
            onSignOut={() => setRoute("logout")}
          />
        ) : null}
        {route === "pin" ? (
          <PinChange
            vault={vault}
            digits={vault.status.digits ?? 4}
            onBack={backToSecurity}
            onChanged={(status) => {
              vault.adopt(status);
              backToSecurity();
            }}
          />
        ) : null}
        {route === "device" ? (
          <PinConfirm
            vault={vault}
            digits={vault.status.digits ?? 4}
            onBack={backToSecurity}
            onArmed={(status) => {
              vault.adopt(status);
              backToSecurity();
            }}
          />
        ) : null}
        {route === "logout" ? (
          <LogoutScreen onBack={backToSecurity} onConfirmed={() => void signOut()} />
        ) : null}
      </main>

      {syncShown ? (
        <button
          type="button"
          className="icon-button app__sync"
          onClick={() => void sync()}
          disabled={messaging.collecting || messaging.book.relationships.length === 0}
          aria-label={messaging.collecting ? t.messages.sync.syncing : t.messages.sync.action}
          aria-busy={messaging.collecting}
        >
          <SyncIcon className={messaging.collecting ? "icon-button__spin" : undefined} />
        </button>
      ) : null}

      {/* The menu steps aside while the camera preview is live. */}
      {cameraPreview || barless ? null : (
        <LiquidTabBar
          label={t.nav.label}
          tabs={tabs}
          active={tabOf(route)}
          onSelect={(next) => {
            setSettingsSection(null);
            setScannedInvitation(null);
            setScannedStart(null);
            setScannedRequest(null);
            setRoute(next);
          }}
        />
      )}
    </div>
  );
}

/** The tab a route is under: the screens behind a tab light that tab. */
function tabOf(route: Route): Route {
  switch (route) {
    case "invite":
      return "home";
    case "thread":
    case "invitation":
      return "messages";
    case "requestStart":
    case "request":
      return "scan";
    default:
      return route;
  }
}

type OpenThreadProps = {
  messaging: Messaging;
  threadKey: string | null;
  onBack: () => void;
};

/**
 * The thread route, resolved against the book: the thread and its
 * relationship are looked up on every render, so a book read again while
 * the screen is open — after a collection — is what the screen shows. One
 * that is not in the book is nothing to show, and the inbox is where
 * somebody goes instead.
 */
function OpenThread({ messaging, threadKey, onBack }: OpenThreadProps) {
  const thread = threadsOf(messaging.book).find((t) => t.key === threadKey);
  const relationship = messaging.book.relationships.find(
    (r) => r.counterparty === thread?.counterparty,
  );
  useEffect(() => {
    if (messaging.read && (!thread || !relationship)) {
      onBack();
    }
  }, [messaging.read, thread, relationship, onBack]);
  if (!thread || !relationship) {
    return null;
  }
  return (
    <ThreadScreen
      messaging={messaging}
      thread={thread}
      relationship={relationship}
      onBack={onBack}
    />
  );
}
