import { useCallback, useEffect, useState } from "react";

import { BrandSpinner } from "./components/BrandSpinner";
import { LiquidTabBar, type TabDefinition } from "./components/LiquidTabBar";
import { HomeIcon, MessagesIcon, ProfileIcon, QrIcon } from "./components/icons";
import { useI18n } from "./i18n";
import { plural } from "./i18n/format";
import { useIdle } from "./autolock";
import { useBackdrop } from "./backdrop";
import { invitationKind, useDeepLinks } from "./links";
import { useLive } from "./live";
import { useRung, useRungCalls } from "./ring";
import { calls, useCall } from "./call";
import { useBackInSight } from "./lock";
import { usePlatform } from "./platform";
import { PreferencesProvider, useDevicePreferences } from "./preferences";
import { registerPush, unregisterPush } from "./push";
import { forgetIdentity, type Identity } from "./identity";
import { useTray } from "./tray";
import {
  destroyVault,
  errorCode as vaultErrorCode,
  openVault,
  openVaultWithDevice,
  useVault,
} from "./vault";
import { CallScreen } from "./screens/CallScreen";
import { DeviceLockScreen } from "./screens/DeviceLockScreen";
import { HomeScreen } from "./screens/HomeScreen";
import { LinkSheet, type LinkOrigin } from "./screens/LinkSheet";
import { LogoutScreen } from "./screens/LogoutScreen";
import { MessagesTab } from "./screens/MessagesTab";
import { PinScreen } from "./screens/PinScreen";
import { ProfileScreen } from "./screens/ProfileScreen";
import { ScanScreen } from "./screens/ScanScreen";
import { Onboarding } from "./screens/onboarding/Onboarding";

type Route = "home" | "messages" | "scan" | "profile";

/** An invitation that came from outside — a link or a code — waiting to be put to the person. */
type Link = { url: string; origin: LinkOrigin };

export default function App() {
  const { t, locale } = useI18n();
  // Applied here for the tokens they put on the root element, and chosen in
  // Profile (`preferences.tsx`).
  const preferences = useDevicePreferences();
  const { theme, autoLock } = preferences;
  const platform = usePlatform();
  // On a computer, the wallet on the system tray: closing the window puts it
  // away rather than ending it, and the tray's menu is where it is quit.
  useTray();
  // While the scanner's preview is live the camera is drawn behind the page,
  // and the chrome steps aside for it.
  const [cameraPreview, setCameraPreview] = useState(false);
  // The window behind the page wears the page's colour — see `backdrop`. Read
  // after `useTheme`, because it reads the palette that hook just applied.
  useBackdrop(theme, cameraPreview);
  const vault = useVault();
  const [identity, setIdentity] = useState<Identity | null>(null);
  const [route, setRoute] = useState<Route>("home");
  // Bumped by every press on the floating menu, so a tab is opened afresh at
  // its first screen even when it is the one already showing.
  const [visit, setVisit] = useState(0);
  const [unlockError, setUnlockError] = useState<string | null>(null);
  const [unlockBusy, setUnlockBusy] = useState(false);
  // Signing out is reachable from Settings, from behind the lock and from a
  // record that cannot be opened; it is one screen whichever way it is reached.
  const [signOutAsked, setSignOutAsked] = useState(false);
  // A keypad that takes the whole screen — replacing the PIN, arming the
  // device — has the bar step aside: under a keypad it is a band of nothing, and
  // a stray tap there is a tap past the question.
  const [keypad, setKeypad] = useState(false);

  // **Links and codes from outside go to a sheet that asks, never straight to
  // an action.** An `almena://` link that opened the wallet, or a code the
  // scanner read, is kept here until an identity is open — a link that arrives
  // behind the lock waits for the PIN — and then put to the person over the
  // open screen, with Cancel and Accept (`LinkSheet`).
  const [link, setLink] = useState<Link | null>(null);
  const openLink = useCallback(async (url: string, origin: LinkOrigin) => {
    if ((await invitationKind(url)) !== "unknown") {
      setLink({ url, origin });
    }
  }, []);
  const closeLink = useCallback(() => setLink(null), []);
  useDeepLinks((url) => void openLink(url, "link"));
  // A conversation to open the Messages tab on, when a link led to one.
  const [conversation, setConversation] = useState<string | null>(null);
  // A call, placed or ringing, while an identity is open.
  const call = useCall();
  const inCall = call !== null && call.phase !== "ended";
  useEffect(() => (identity !== null ? calls.attach() : undefined), [identity]);
  // A call the phone rang while the wallet was closed; the lock says so.
  useRungCalls();
  const rung = useRung();
  // Messages arrive live while an identity is open and the wallet is seen —
  // and during a call even when it is not, or its hang-up would not arrive.
  useLive(identity !== null, inCall);
  // And while it is not running, the mediator notifies this device.
  useEffect(() => {
    if (identity !== null) {
      void registerPush();
    }
  }, [identity]);

  // **Locking is letting go, not hiding.** There is no flag that says the wallet
  // is closed while the seed sits behind it in memory: the lock drops the
  // identity, and coming back opens the record again with a PIN or a face.
  // Live delivery stops with it, since it follows `identity`.
  const lockNow = useCallback(() => {
    setIdentity((open) => {
      if (open) {
        void forgetIdentity();
      }
      return null;
    });
    setRoute("home");
  }, []);

  // One clock, for every way of not using the wallet — left open on a desk or
  // left behind for another app — armed only while a wallet is open. Coming
  // back is when the clock is asked the time, for a webview whose timer the
  // system throttled or froze while nobody could see it.
  // Nobody touches a wallet while they talk: a call holds the clock.
  const catchUp = useIdle(autoLock, identity !== null && !inCall, lockNow);
  useBackInSight(catchUp);

  // Signing out is not locking: the record itself goes, and the phrase is what
  // is left.
  const signOut = useCallback(async () => {
    setRoute("home");
    setSignOutAsked(false);
    // While the identity can still speak to its mediator: a device that keeps
    // being woken for an identity that has left it is told nothing true.
    await unregisterPush();
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
        const code = vaultErrorCode(failure);
        // Without a PIN, a missing key is not "use your PIN": it is the passcode
        // having been taken off the phone, and the phrase is what is left.
        setUnlockError(
          code === "vault_no_device_key" && !vault.status.pin
            ? t.vault.deviceLock.keyGone
            : t.vault.errors[code],
        );
        // The count of what is left changed, and a record spent to its last
        // attempt is gone — which the welcome screen has to be told about.
        void vault.refresh();
      } finally {
        setUnlockBusy(false);
      }
    },
    [t, vault],
  );

  // Nothing is drawn on a guess: whether this device holds an identity decides
  // between the way in and the lock.
  if (!vault.read) {
    return (
      <div className="app">
        <main className="app__view app__view--plain">
          <BrandSpinner label={t.app.name} />
        </main>
      </div>
    );
  }

  if (signOutAsked) {
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
            deviceLock={vault.status.deviceLock}
            onReady={(made) => {
              setIdentity(made);
              void vault.refresh();
            }}
          />
        </main>
      </div>
    );
  }

  // There is one, it is not open, and the iPhone's lock is the only one on it.
  if (!identity && !vault.status.pin) {
    return (
      <div className="app">
        <main className="app__view app__view--plain">
          <DeviceLockScreen
            calling={rung !== null}
            error={unlockError}
            busy={unlockBusy}
            onUnlock={() => {
              void unlock(openVaultWithDevice);
            }}
            footer={
              <button
                type="button"
                className="button button--danger"
                onClick={() => setSignOutAsked(true)}
              >
                {t.settings.security.signOut}
              </button>
            }
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
            subtitle={rung ? t.pin.callSubtitle : t.pin.unlockSubtitle}
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
                    from ten at every launch reads as broken; one that says
                    nothing lets somebody destroy an identity by guessing. */}
                {vault.status.attemptsLeft <= 3 ? (
                  <p className="card__note card__note--warning">
                    {plural(t.pin.attemptsLeft, vault.status.attemptsLeft, locale)}
                  </p>
                ) : null}
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

  // **Scanning is offered only where it can happen.** A computer has no camera
  // the wallet may drive, and it opens invitations as `almena://` links
  // instead. The answer comes from the Rust side, from the same switch that
  // decided whether to register the scanner at all.
  const tabs: TabDefinition<Route>[] = [
    { id: "home", label: t.nav.home, icon: <HomeIcon /> },
    { id: "messages", label: t.nav.messages, icon: <MessagesIcon /> },
    ...(platform.barcodeScanner ? [{ id: "scan" as const, label: t.nav.scan, icon: <QrIcon /> }] : []),
    { id: "profile", label: t.nav.profile, icon: <ProfileIcon /> },
  ];

  const select = (next: Route) => {
    setConversation(null);
    setRoute(next);
    setVisit((count) => count + 1);
  };

  return (
    <PreferencesProvider value={preferences}>
      <div className={cameraPreview ? "app app--camera" : "app"}>
        <main className={keypad ? "app__view app__view--plain" : "app__view"} key={`${route}-${visit}`}>
          {route === "home" ? <HomeScreen /> : null}
          {route === "messages" ? <MessagesTab
              initialConversation={conversation}
              onLink={(url) => void openLink(url, "link")}
            /> : null}
          {route === "scan" ? (
            <ScanScreen
              onBack={() => setRoute("home")}
              onPreviewChange={setCameraPreview}
              onInvitation={(content) => {
                // The scanner has done its part: the sheet is put over the home
                // screen, and whatever the answer is, that is where it leaves.
                setRoute("home");
                void openLink(content, "qr");
              }}
            />
          ) : null}
          {route === "profile" ? (
            <ProfileScreen
              vault={vault}
              onKeypad={setKeypad}
              onSignOut={() => setSignOutAsked(true)}
            />
          ) : null}
        </main>

        {keypad || cameraPreview ? null : (
          <LiquidTabBar label={t.nav.label} tabs={tabs} active={route} onSelect={select} />
        )}

        {call ? <CallScreen view={call} /> : null}

        {link ? (
          <LinkSheet
            key={link.url}
            url={link.url}
            origin={link.origin}
            onClose={closeLink}
            onConversation={(id) => {
              setLink(null);
              setConversation(id);
              setRoute("messages");
              // The tab reads which conversation to open only when it is
              // mounted: already showing, it has to be opened afresh.
              setVisit((count) => count + 1);
            }}
            onMediatorConnected={() => {
              setLink(null);
              void registerPush();
            }}
          />
        ) : null}
      </div>
    </PreferencesProvider>
  );
}
