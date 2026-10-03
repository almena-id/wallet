import { useEffect } from "react";

import { BackButton } from "../components/BackButton";
import { ChevronLeftIcon, QrIcon } from "../components/icons";
import { ProfileHeader } from "../components/ProfileHeader";
import { useTranslations } from "../i18n";
import { useMediation } from "../mediator";
import { useStack } from "../nav";
import { usePreferences } from "../preferences";
import { registerPush } from "../push";
import { usePlatform } from "../platform";
import type { Vault } from "../vault";
import { PinChange } from "./PinChange";
import { PinConfirm } from "./PinConfirm";
import { InviteScreen } from "./InviteScreen";
import { MediatorConnectScreen } from "./MediatorConnectScreen";
import { AppearanceSettings } from "./settings/AppearanceSettings";
import { BackupSettings } from "./settings/BackupSettings";
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
  | "backup"
  | "security";

const SECTIONS: Section[] = [
  "credentials",
  "profile",
  "appearance",
  "notifications",
  "messaging",
  "backup",
  "security",
];

/**
 * Where the profile tab is, on a stack (`nav.ts`): the menu, a section, or a
 * screen a section sends somebody to — choosing a mediator, replacing the PIN,
 * arming the device, this wallet's code — each left by its own way back.
 */
type View =
  | { name: "menu" }
  | { name: "section"; section: Section }
  | { name: "connect" }
  | { name: "pin" }
  | { name: "device" }
  | { name: "invite" };

type ProfileScreenProps = {
  /** What the device is keeping, which the security section describes and changes. */
  vault: Vault;
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
export function ProfileScreen({ vault, onKeypad, onSignOut }: ProfileScreenProps) {
  const t = useTranslations();
  const preferences = usePreferences();
  const { top, push, pop } = useStack<View>([{ name: "menu" }]);
  const mediation = useMediation();
  // Notifications are the computer's: a phone is told by push, which says
  // nothing about the message whatever is chosen here.
  const sections = usePlatform().kind === "mobile"
    ? SECTIONS.filter((name) => name !== "notifications")
    : SECTIONS;

  const keypad = top.name === "pin" || top.name === "device";
  useEffect(() => {
    onKeypad(keypad);
  }, [keypad, onKeypad]);
  // Leaving the tab — a lock, say — must not leave the bar hidden behind it.
  useEffect(() => () => onKeypad(false), [onKeypad]);

  // This wallet's invitation, as a code somebody in front of it scans to open a
  // relationship — each of them answered from a pairwise of their own.
  if (top.name === "invite") {
    return <InviteScreen onBack={pop} />;
  }
  if (top.name === "connect") {
    return (
      <MediatorConnectScreen
        onBack={pop}
        onConnected={(status) => {
          mediation.adopt(status);
          pop();
          void registerPush();
        }}
      />
    );
  }
  if (top.name === "pin") {
    return (
      <PinChange
        vault={vault}
        digits={vault.status.digits ?? 4}
        onBack={pop}
        onChanged={(status) => {
          vault.adopt(status);
          pop();
        }}
      />
    );
  }
  if (top.name === "device") {
    return (
      <PinConfirm
        vault={vault}
        digits={vault.status.digits ?? 4}
        onBack={pop}
        onArmed={(status) => {
          vault.adopt(status);
          pop();
        }}
      />
    );
  }

  if (top.name === "section") {
    const section = top.section;
    return (
      <div className="screen">
        <header className="screen__header screen__header--compact">
          <BackButton onBack={pop} />
          <h1 className="screen__title screen__title--compact">
            {t.settings.sections[section].title}
          </h1>
        </header>

        {section === "credentials" ? <CredentialsSettings /> : null}
        {section === "profile" ? <ProfileSettings /> : null}
        {section === "appearance" ? (
          <AppearanceSettings
            accent={preferences.accent}
            onAccentChange={preferences.setAccent}
            theme={preferences.theme}
            onThemeChange={preferences.setTheme}
          />
        ) : null}
        {section === "notifications" ? (
          <NotificationsSettings
            privacy={preferences.privacy}
            onPrivacyChange={preferences.setPrivacy}
          />
        ) : null}
        {section === "messaging" ? (
          <MessagingSettings mediation={mediation} onConnect={() => push({ name: "connect" })} />
        ) : null}
        {section === "backup" ? <BackupSettings /> : null}
        {section === "security" ? (
          <SecuritySettings
            vault={vault}
            autoLock={preferences.autoLock}
            onAutoLockChange={preferences.setAutoLock}
            onChangePin={() => push({ name: "pin" })}
            onArmDevice={() => push({ name: "device" })}
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
          onClick={() => push({ name: "invite" })}
          aria-label={t.profile.showCode}
        >
          <QrIcon />
        </button>
      </header>

      <ProfileHeader />

      <nav className="menu" aria-label={t.settings.title}>
        {sections.map((name) => (
          <button
            key={name}
            type="button"
            className="menu__item"
            onClick={() => push({ name: "section", section: name })}
          >
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
