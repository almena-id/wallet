import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import type { Identity } from "./identity";

/** Who recognises the person: Face ID or Touch ID, Android's biometrics, Windows Hello. */
export type BiometricKind = "apple" | "android" | "windows";

/**
 * The words the system's prompt shows, in the interface's language. Only
 * Android's prompt is worded by the app; elsewhere they are ignored.
 */
export type BiometricPrompt = {
  title: string;
  subtitle: string;
  /** The button that leaves the prompt for the PIN. */
  cancel: string;
};

/**
 * What the device is keeping, and what it takes to open it.
 *
 * The seed itself never crosses to this side. The interface asks whether there
 * is an identity, offers the two ways of opening it, and is handed back the
 * identity — never the secret it was derived from.
 */
export type VaultStatus = {
  /** Whether this device is holding an identity it can open. */
  exists: boolean;
  /**
   * Why it cannot be read, when something is there and unreadable.
   *
   * "There is no identity here" and "there is one and it cannot be reached" are
   * different answers, and telling them apart is what stops the wallet offering
   * to create a second identity over somebody's first one because a keyring was
   * locked at the wrong moment.
   */
  problem: VaultErrorCode | null;
  /**
   * Whether digits open this at all. False on an iPhone with a passcode, where
   * the phone's own lock — the face, then the passcode — is the only one.
   */
  pin: boolean;
  /** How long the PIN is, so the keypad can be drawn before it is typed. */
  digits: number | null;
  /**
   * The shortest PIN this device takes when one is chosen or changed: four
   * where hardware keeps the record on the device (an iPhone, the Android
   * Keystore), six where a process of the same user could copy it off and try
   * every PIN at leisure (a computer, an Android file).
   */
  minDigits: number;
  /**
   * Whether the PIN is shorter than `minDigits`. It still opens the wallet;
   * once it has, a longer one is asked for before anything else.
   */
  lengthen: boolean;
  /** Whether the device's own lock can be the wallet's, with no PIN: an iPhone. */
  deviceLock: boolean;
  /** Whether the device is holding a key, and so whether a face opens this. */
  deviceKey: boolean;
  /**
   * Whether this platform can offer opening with the device at all.
   *
   * Only where the system refuses to hand the key over until it has
   * recognised somebody: iOS, a signed macOS build on a Mac with a sensor, an
   * Android phone with a strong biometric enrolled, Windows with Hello set up.
   * Linux has no standard way to ask, so a button there would be a way in that
   * asks for nothing.
   */
  deviceUnlock: boolean;
  /**
   * Whether the device's copy of the key can no longer come back: a
   * fingerprint or face enrolled since it was turned on, or Windows Hello
   * reset. `deviceKey` is then false and the PIN is the way in.
   */
  deviceChanged: boolean;
  /** Which system recognises the person here, so it is named as that system names it. */
  biometrics: BiometricKind | null;
  /** Wrong PINs left before the record is destroyed. */
  attemptsLeft: number;
  /** Where the record is kept: the platform's secret store, a private file, or that file sealed under the Android Keystore. */
  home: "store" | "file" | "keystore" | null;
  /** The format version found on the device. */
  version: number | null;
};

/** What a wallet that has never been opened looks like. */
export const emptyVault: VaultStatus = {
  exists: false,
  problem: null,
  pin: false,
  digits: null,
  minDigits: 6,
  lengthen: false,
  deviceLock: false,
  deviceKey: false,
  deviceUnlock: false,
  deviceChanged: false,
  biometrics: null,
  attemptsLeft: 10,
  home: null,
  version: null,
};

/** The lengths a PIN can have. */
export const PIN_LENGTHS = [4, 6] as const;

/** The lengths a new PIN may be chosen at on this device, shortest first. */
export function pinLengths(minDigits: number): number[] {
  return PIN_LENGTHS.filter((digits) => digits >= minDigits);
}

export function readVault(): Promise<VaultStatus> {
  return invoke<VaultStatus>("vault_status");
}

/**
 * Writes down the identity that is open, behind a PIN — or, with none, behind
 * the iPhone's own lock, which answers `vault_no_passcode` where the phone has
 * no passcode to put it behind.
 */
export function createVault(pin?: string): Promise<VaultStatus> {
  return invoke<VaultStatus>("vault_create", { pin: pin ?? null });
}

/** Opens it with the digits, and hands back the identity that was inside. */
export function openVault(pin: string): Promise<Identity> {
  return invoke<Identity>("vault_open", { pin });
}

/**
 * Opens it with the key the device is holding.
 *
 * The prompt this raises is the system's own: the key is stored so that it is
 * not handed over until somebody has been recognised, so there is no separate
 * question to ask first. `vault_device_changed` means the copy could no longer
 * come back and biometrics have been turned off.
 */
export function openVaultWithDevice(prompt?: BiometricPrompt): Promise<Identity> {
  return invoke<Identity>("vault_open_with_device", { prompt: prompt ?? null });
}

export function changeVaultPin(current: string, next: string): Promise<VaultStatus> {
  return invoke<VaultStatus>("vault_change_pin", { current, next });
}

/** Arms or disarms opening with the device. Arming needs the PIN. */
export function setVaultDevice(
  enabled: boolean,
  pin?: string,
  prompt?: BiometricPrompt,
): Promise<VaultStatus> {
  return invoke<VaultStatus>("vault_set_device", {
    enabled,
    pin: pin ?? null,
    prompt: prompt ?? null,
  });
}

/**
 * Confirms the holder is in front of the open wallet — the PIN, or with none
 * the device's own check — before checking the recovery phrase, which the Rust
 * side refuses for a few minutes without it.
 */
export function confirmVault(pin?: string, prompt?: BiometricPrompt): Promise<void> {
  return invoke<void>("vault_confirm", { pin: pin ?? null, prompt: prompt ?? null });
}

/**
 * Takes the identity off the device. The phrase is what is left.
 *
 * Called only once the sign-out screen has been confirmed; the Rust side
 * removes nothing without being told that is what the call is for.
 */
export function destroyVault(): Promise<VaultStatus> {
  return invoke<VaultStatus>("vault_destroy", { confirm: "sign-out" });
}

/**
 * The error codes the vault commands answer with. They are codes and not
 * sentences, so this side says them in the language somebody is reading.
 */
export type VaultErrorCode =
  | "vault_wrong_pin"
  | "vault_destroyed"
  | "vault_nothing"
  | "vault_exists"
  | "vault_pin_length"
  | "vault_pin_not_digits"
  | "vault_pin_too_short"
  | "vault_unconfirmed"
  | "vault_no_identity"
  | "vault_no_device_key"
  | "vault_device_unproven"
  | "vault_no_device_store"
  | "vault_device_refused"
  | "vault_device_changed"
  | "vault_no_pin"
  | "vault_no_passcode"
  | "vault_unreadable"
  | "vault_too_new"
  | "vault_storage"
  | "vault_entropy"
  | "vault_unknown";

const CODES: VaultErrorCode[] = [
  "vault_wrong_pin",
  "vault_destroyed",
  "vault_nothing",
  "vault_exists",
  "vault_pin_length",
  "vault_pin_not_digits",
  "vault_pin_too_short",
  "vault_unconfirmed",
  "vault_no_identity",
  "vault_no_device_key",
  "vault_device_unproven",
  "vault_no_device_store",
  "vault_device_refused",
  "vault_device_changed",
  "vault_no_pin",
  "vault_no_passcode",
  "vault_unreadable",
  "vault_too_new",
  "vault_storage",
  "vault_entropy",
];

/** Whatever a rejected command threw, as a code this interface has a word for. */
export function errorCode(error: unknown): VaultErrorCode {
  return typeof error === "string" && (CODES as string[]).includes(error)
    ? (error as VaultErrorCode)
    : "vault_unknown";
}

export type Vault = {
  status: VaultStatus;
  /** False until the device has answered, so nothing is drawn on a guess. */
  read: boolean;
  /** Asks again, after something changed it. */
  refresh: () => Promise<VaultStatus>;
  /** Replaces what is known, for a command that already answered with it. */
  adopt: (status: VaultStatus) => void;
};

/**
 * What this device is holding, kept in front of the whole application.
 *
 * It is read once at startup because it decides the first screen: a wallet with
 * a record opens on the lock, and one without opens on the way in. Getting that
 * wrong shows somebody the welcome screen for an identity they already have.
 */
export function useVault(): Vault {
  const [status, setStatus] = useState<VaultStatus>(emptyVault);
  const [read, setRead] = useState(false);

  const refresh = useCallback(async () => {
    const current = await readVault().catch(() => emptyVault);
    setStatus(current);
    setRead(true);
    return current;
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return {
    status,
    read,
    refresh,
    adopt: useCallback((next: VaultStatus) => setStatus(next), []),
  };
}
