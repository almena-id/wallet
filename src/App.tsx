import { useCallback, useEffect, useState } from "react";

import { LiquidTabBar, type TabDefinition } from "./components/LiquidTabBar";
import { BrandSpinner } from "./components/BrandSpinner";
import { HomeIcon, QrIcon, SettingsIcon } from "./components/icons";
import { plural, useI18n } from "./i18n";
import { useAccent } from "./appearance";
import { useAutoLock, useIdle } from "./autolock";
import { useBackdrop } from "./backdrop";
import { useDeepLink } from "./deepLink";
import { useBackInSight } from "./lock";
import { forgetInvitation, isInvitationLink } from "./invitation";
import { forgetSignIn, isSignInLink } from "./signin";
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
import { IdentityScreen } from "./screens/IdentityScreen";
import { PinChange } from "./screens/PinChange";
import { PinConfirm } from "./screens/PinConfirm";
import { PinScreen } from "./screens/PinScreen";
import { LogoutScreen } from "./screens/LogoutScreen";
import { ApprovalScreen } from "./screens/ApprovalScreen";
import { InvitationScreen } from "./screens/InvitationScreen";
import { LinkScreen } from "./screens/LinkScreen";
import { ScanScreen } from "./screens/ScanScreen";
import { Onboarding } from "./screens/onboarding/Onboarding";
import { SettingsScreen } from "./screens/settings/SettingsScreen";

/**
 * `link`, `approve`, `invite` and `logout` are not tabs: the first three are
 * where an `almena://` link puts the wallet, and the last is what Settings
 * opens to ask whether somebody means it.
 * `identity` is where the home screen's own card leads, to show the identifier
 * as a code.
 * `pin` and `device` are the two things Security sends somebody to. All of them
 * are left through their own back button.
 */
type Route =
  | "home"
  | "scan"
  | "settings"
  | "identity"
  | "link"
  | "approve"
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
  const deepLink = useDeepLink();
  const { accent, setAccent } = useAccent();
  const { autoLock, setAutoLock } = useAutoLock();
  const { theme, setTheme } = useTheme();
  const vault = useVault();
  const [identity, setIdentity] = useState<Identity | null>(null);
  const [route, setRoute] = useState<Route>("home");
  // Where Settings opens: back where somebody was, when they are coming back
  // from a screen a section sent them to.
  const [settingsSection, setSettingsSection] = useState<"security" | null>(null);
  const [cameraPreview, setCameraPreview] = useState(false);
  // The window behind the page wears the same colour the page does, so turning
  // the device does not flash the native white through — see `backdrop`. Read
  // after `useTheme` above, because it reads the palette that hook just applied.
  useBackdrop(theme, cameraPreview);
  // The request being answered, whether it arrived by link or through the
  // camera. Which screen reads it is the route beside it.
  const [request, setRequest] = useState<string | null>(null);
  const [unlockError, setUnlockError] = useState<string | null>(null);
  // Signing out is reachable from behind the lock and from a record that cannot
  // be opened, neither of which has a Settings to route through.
  const [signOutAsked, setSignOutAsked] = useState(false);
  const [unlockBusy, setUnlockBusy] = useState(false);

  // **What an `almena://` link means is decided here and nowhere else.** Links
  // reach the wallet two ways — the system opens it with one, or the camera
  // reads one — and both end at this function, so a kind of link the wallet
  // learns to answer is learned once rather than in each doorway.
  //
  // Everything the wallet has no answer for is still only shown. Nothing
  // outside gets to tell the wallet what to do with the identity it holds.
  const openLink = useCallback((url: string) => {
    if (isSignInLink(url)) {
      setRequest(url);
      setRoute("approve");
    } else if (isInvitationLink(url)) {
      setRequest(url);
      setRoute("invite");
    } else {
      setRoute("link");
    }
  }, []);

  // A link that arrives takes the screen, whatever was on it. It is the only
  // thing here that can come from outside while somebody is looking elsewhere.
  useEffect(() => {
    if (deepLink.url) {
      openLink(deepLink.url);
    }
  }, [deepLink.url, openLink]);

  const leaveLink = useCallback(() => {
    deepLink.clear();
    setRoute("home");
  }, [deepLink]);

  // Whatever was not answered is dropped on the Rust side too, so nothing is
  // left waiting behind a screen nobody is looking at. Both are forgotten
  // without asking which was being held: only one ever is, and the other call
  // finds nothing and says so quietly.
  const forgetRequests = useCallback(() => {
    void forgetSignIn();
    void forgetInvitation();
  }, []);

  const leaveRequest = useCallback(() => {
    forgetRequests();
    setRequest(null);
    deepLink.clear();
    setRoute("home");
  }, [deepLink, forgetRequests]);

  // **Locking is letting go, not hiding.** There is no flag that says the wallet
  // is closed while the seed sits behind it in memory: the lock drops the
  // identity, and coming back opens the record again with a PIN or a face. What
  // is written on the device stays written, and the seed the wallet was signing
  // with is no longer in this process at all — which is a stronger thing to say
  // than that a screen is over it, and it costs one derivation to undo.
  //
  // A request nobody answered goes with it, on both sides, rather than waiting
  // behind a lock for somebody who has walked away.
  const lockNow = useCallback(() => {
    forgetRequests();
    setRequest(null);
    deepLink.clear();
    setIdentity((open) => {
      if (open) {
        void forgetIdentity();
      }
      return null;
    });
    setRoute("home");
  }, [deepLink, forgetRequests]);

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

  const goHome = useCallback(() => setRoute("home"), []);
  const backToSecurity = useCallback(() => {
    setSettingsSection("security");
    setRoute("settings");
  }, []);

  // **Scanning is offered only where it can happen.** A computer has no camera
  // the wallet may drive, and a request reaches it as an `almena://` link
  // instead; a tab there would be a tab that only ever leads to an apology. The
  // answer comes from the Rust side, which knows because it is the same switch
  // that decided whether to register the scanner at all.
  const tabs: TabDefinition<Route>[] = [
    { id: "home", label: t.nav.home, icon: <HomeIcon /> },
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

  return (
    <div className={cameraPreview ? "app app--camera" : "app"}>
      <main
        className={barless && !cameraPreview ? "app__view app__view--plain" : "app__view"}
        key={route}
      >
        {route === "home" ? (
          <HomeScreen identity={identity} onShowCode={() => setRoute("identity")} />
        ) : null}
        {route === "identity" ? <IdentityScreen identity={identity} onBack={goHome} /> : null}
        {route === "scan" ? (
          <ScanScreen
            platform={platform}
            onBack={goHome}
            onPreviewChange={setCameraPreview}
            onRequest={openLink}
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
        {route === "link" && deepLink.url ? (
          <LinkScreen url={deepLink.url} onBack={leaveLink} />
        ) : null}
        {route === "approve" && request ? (
          <ApprovalScreen link={request} onBack={leaveRequest} />
        ) : null}
        {route === "invite" && request ? (
          <InvitationScreen link={request} onBack={leaveRequest} />
        ) : null}
      </main>

      {/* The menu steps aside while the camera preview is live. */}
      {cameraPreview || barless ? null : (
        <LiquidTabBar
          label={t.nav.label}
          tabs={tabs}
          active={
            route === "link" ||
            route === "approve" ||
            route === "invite" ||
            route === "identity"
              ? "home"
              : route
          }
          onSelect={(next) => {
            forgetRequests();
            setRequest(null);
            deepLink.clear();
            setSettingsSection(null);
            setRoute(next);
          }}
        />
      )}
    </div>
  );
}
