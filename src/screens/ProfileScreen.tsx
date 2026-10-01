import { useEffect, useState } from "react";

import { ChevronLeftIcon, QrIcon } from "../components/icons";
import { ProfileHeader } from "../components/ProfileHeader";
import { useTranslations } from "../i18n";
import { useMediation } from "../mediator";
import { registerPush } from "../push";
import type { Accent } from "../appearance";
import type { AutoLock } from "../autolock";
import type { Privacy } from "../notify";
import { usePlatform } from "../platform";
import type { Theme } from "../theme";
import type { Vault } from "../vault";
import { PinChange } from "./PinChange";
import { PinConfirm } from "./PinConfirm";
import { InviteScreen } from "./InviteScreen";
import { MediatorConnectScreen } from "./MediatorConnectScreen";
import { AppearanceSettings } from "./settings/AppearanceSettings";
import { MessagingSettings } from "./settings/MessagingSettings";
import { NotificationsSettings } from "./settings/NotificationsSettings";
import { ProfileSettings } from "./settings/ProfileSettings";
import { CredentialsSettings } from "./settings/CredentialsSettings";
import { SecuritySettings } from "./settings/SecuritySettings";

/** Where the profile leads, and the list that leads there. */
type Section =
  | "credentials"
  | "profile"
  | "appearance"
  | "notifications"
  | "messaging"
  | "security";

const SECTIONS: Section[] = [
  "credentials",
  "profile",
  "appearance",
  "notifications",
  "messaging",
  "security",
];

/** A screen a section sends somebody to, and comes back from. */
type Aside = "connect" | "pin" | "device" | "invite";

type ProfileScreenProps = {
  /** What the device is keeping, which the security section describes and changes. */
  vault: Vault;
  accent: Accent;
  onAccentChange: (accent: Accent) => void;
  theme: Theme;
  onThemeChange: (theme: Theme) => void;
  /** How long the wallet stays open with nobody using it. */
  autoLock: AutoLock;
  onAutoLockChange: (minutes: AutoLock) => void;
  /** How much a system notification says about a message. */
  privacy: Privacy;
  onPrivacyChange: (privacy: Privacy) => void;
  /**
   * Told whether a keypad is taking the whole screen — replacing the PIN or
   * arming the device — so the tab bar steps aside for it.
   */
  onKeypad: (showing: boolean) => void;
  /** Opens the screen that asks whether somebody really means it. */
  onSignOut: () => void;
};

/**
 * The profile tab: the identity's picture and name at the top, then a list of
 * sections, each on a screen of its own behind it — the shape of the previous
 * Almena ID wallet's settings. A section holds only what this wallet can
 * already do; one that has nothing yet is not listed.
 */
export function ProfileScreen({
  vault,
  accent,
  onAccentChange,
  theme,
  onThemeChange,
  autoLock,
  onAutoLockChange,
  privacy,
  onPrivacyChange,
  onKeypad,
  onSignOut,
}: ProfileScreenProps) {
  const t = useTranslations();
  const [section, setSection] = useState<Section | null>(null);
  const mediation = useMediation();
  // Notifications are the computer's: a phone is told by push, which says
  // nothing about the message whatever is chosen here.
  const sections = usePlatform().kind === "mobile"
    ? SECTIONS.filter((name) => name !== "notifications")
    : SECTIONS;
  // Choosing a mediator, replacing the PIN and arming the device are screens of
  // their own, with their own way back, so the section is left for them rather
  // than drawn under them — and returned to afterwards.
  const [aside, setAside] = useState<Aside | null>(null);
  const back = () => setAside(null);

  const keypad = aside === "pin" || aside === "device";
  useEffect(() => {
    onKeypad(keypad);
  }, [keypad, onKeypad]);
  // Leaving the tab — a lock, say — must not leave the bar hidden behind it.
  useEffect(() => () => onKeypad(false), [onKeypad]);

  // This wallet's invitation, as a code somebody in front of it scans to open a
  // relationship — each of them answered from a pairwise of their own.
  if (aside === "invite") {
    return <InviteScreen onBack={back} />;
  }
  if (aside === "connect") {
    return (
      <MediatorConnectScreen
        onBack={back}
        onConnected={(status) => {
          mediation.adopt(status);
          back();
          void registerPush();
        }}
      />
    );
  }
  if (aside === "pin") {
    return (
      <PinChange
        vault={vault}
        digits={vault.status.digits ?? 4}
        onBack={back}
        onChanged={(status) => {
          vault.adopt(status);
          back();
        }}
      />
    );
  }
  if (aside === "device") {
    return (
      <PinConfirm
        vault={vault}
        digits={vault.status.digits ?? 4}
        onBack={back}
        onArmed={(status) => {
          vault.adopt(status);
          back();
        }}
      />
    );
  }

  if (section) {
    return (
      <div className="screen">
        <header className="screen__header screen__header--compact">
          <button
            type="button"
            className="icon-button"
            onClick={() => setSection(null)}
            aria-label={t.nav.back}
          >
            <ChevronLeftIcon />
          </button>
          <h1 className="screen__title screen__title--compact">
            {t.settings.sections[section].title}
          </h1>
        </header>

        {section === "credentials" ? <CredentialsSettings /> : null}
        {section === "profile" ? <ProfileSettings /> : null}
        {section === "appearance" ? (
          <AppearanceSettings
            accent={accent}
            onAccentChange={onAccentChange}
            theme={theme}
            onThemeChange={onThemeChange}
          />
        ) : null}
        {section === "notifications" ? (
          <NotificationsSettings privacy={privacy} onPrivacyChange={onPrivacyChange} />
        ) : null}
        {section === "messaging" ? (
          <MessagingSettings mediation={mediation} onConnect={() => setAside("connect")} />
        ) : null}
        {section === "security" ? (
          <SecuritySettings
            vault={vault}
            autoLock={autoLock}
            onAutoLockChange={onAutoLockChange}
            onChangePin={() => setAside("pin")}
            onArmDevice={() => setAside("device")}
            onSignOut={onSignOut}
          />
        ) : null}
      </div>
    );
  }

  return (
    <div className="screen">
      <header className="screen__header screen__header--end">
        <button
          type="button"
          className="icon-button"
          onClick={() => setAside("invite")}
          aria-label={t.profile.showCode}
        >
          <QrIcon />
        </button>
      </header>

      <ProfileHeader />

      <nav className="menu" aria-label={t.settings.title}>
        {sections.map((name) => (
          <button key={name} type="button" className="menu__item" onClick={() => setSection(name)}>
            <span className="menu__text">
              <span className="menu__title">{t.settings.sections[name].title}</span>
              <span className="menu__hint">{t.settings.sections[name].hint}</span>
            </span>
            {/* The back arrow, turned to point where the row leads. */}
            <ChevronLeftIcon className="menu__chevron" />
          </button>
        ))}
      </nav>
    </div>
  );
}
