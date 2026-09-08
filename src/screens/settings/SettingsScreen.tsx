import { useState } from "react";

import { ChevronLeftIcon } from "../../components/icons";
import { useTranslations } from "../../i18n";
import type { Accent } from "../../appearance";
import type { Vault } from "../../vault";
import type { PlatformInfo } from "../../platform";
import type { Theme } from "../../theme";
import { AppearanceSettings } from "./AppearanceSettings";
import { DevelopmentSettings } from "./DevelopmentSettings";
import { PermissionsSettings } from "./PermissionsSettings";
import { SecuritySettings } from "./SecuritySettings";
import type { AutoLock } from "../../autolock";

/** Where Settings leads, and the list that leads there. */
type Section = "appearance" | "permissions" | "security" | "development";

const SECTIONS: Section[] = ["appearance", "permissions", "security", "development"];

type SettingsScreenProps = {
  platform: PlatformInfo;
  accent: Accent;
  onAccentChange: (accent: Accent) => void;
  theme: Theme;
  onThemeChange: (theme: Theme) => void;
  autoLock: AutoLock;
  onAutoLockChange: (minutes: AutoLock) => void;
  vault: Vault;
  /** Where to open, for a return from a screen a section sent somebody to. */
  initialSection?: Section | null;
  /** Opens the screen where the PIN is replaced. */
  onChangePin: () => void;
  /** Opens the screen that asks for the PIN before arming the device. */
  onArmDevice: () => void;
  onSignOut: () => void;
};

export function SettingsScreen({
  platform,
  accent,
  onAccentChange,
  theme,
  onThemeChange,
  autoLock,
  onAutoLockChange,
  vault,
  initialSection = null,
  onChangePin,
  onArmDevice,
  onSignOut,
}: SettingsScreenProps) {
  const t = useTranslations();
  const [section, setSection] = useState<Section | null>(initialSection);

  if (section) {
    return (
      <div className="screen">
        <header className="screen__header screen__header--scan">
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

        {section === "appearance" ? (
          <AppearanceSettings
            accent={accent}
            onAccentChange={onAccentChange}
            theme={theme}
            onThemeChange={onThemeChange}
          />
        ) : null}
        {section === "permissions" ? (
          <PermissionsSettings platform={platform} deviceUnlock={vault.status.deviceUnlock} />
        ) : null}
        {section === "security" ? (
          <SecuritySettings
            vault={vault}
            autoLock={autoLock}
            onAutoLockChange={onAutoLockChange}
            onChangePin={onChangePin}
            onArmDevice={onArmDevice}
            onSignOut={onSignOut}
          />
        ) : null}
        {section === "development" ? <DevelopmentSettings platform={platform} /> : null}
      </div>
    );
  }

  return (
    <div className="screen">
      <header className="screen__header">
        <h1 className="screen__title">{t.settings.title}</h1>
      </header>

      <nav className="menu" aria-label={t.settings.title}>
        {SECTIONS.map((name) => (
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
