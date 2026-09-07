import type { Dictionary } from "./i18n/locales";
import type { PlatformInfo } from "./platform";

/** Human name of the operating system, translated when the catalogue knows it. */
export function osLabel(t: Dictionary, os: string): string {
  const known = t.platform.os as Record<string, string | undefined>;
  return known[os] ?? os;
}

/** For example "Mobile · iOS", or just "Desktop" while the host is unknown. */
export function platformLabel(t: Dictionary, platform: PlatformInfo): string {
  const kind =
    platform.kind === "mobile"
      ? t.platform.mobile
      : platform.kind === "desktop"
        ? t.platform.desktop
        : t.platform.unknown;
  const os = platform.os ? osLabel(t, platform.os) : "";
  return os ? `${kind} · ${os}` : kind;
}
