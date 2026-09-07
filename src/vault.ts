import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import type { Identity } from "./identity";

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
  /** How long the PIN is, so the keypad can be drawn before it is typed. */
  digits: number | null;
  /** Whether the device is holding a key, and so whether a face opens this. */
  deviceKey: boolean;
  /**
   * Whether this platform can offer opening with the device at all.
   *
   * Only iOS, and only because the system there refuses to hand the key over
   * until it has recognised somebody. A keychain on a computer gives its items
   * to whoever is logged in without asking, so a face button there would be a
   * way in that asks for nothing.
   */
  deviceUnlock: boolean;
  /** Wrong PINs left before the record is destroyed. */
  attemptsLeft: number;
  /** Where the record is kept, which the Development section shows. */
  home: "store" | "file" | null;
  /** The format version found on the device. */
  version: number | null;
};

/** What a wallet that has never been opened looks like. */
export const emptyVault: VaultStatus = {
  exists: false,
  problem: null,
  digits: null,
  deviceKey: false,
  deviceUnlock: false,
  attemptsLeft: 10,
  home: null,
  version: null,
};

export function readVault(): Promise<VaultStatus> {
  return invoke<VaultStatus>("vault_status");
}

/** Writes down the identity that is open, behind a PIN. */
export function createVault(pin: string): Promise<VaultStatus> {
  return invoke<VaultStatus>("vault_create", { pin });
}

/** Opens it with the digits, and hands back the identity that was inside. */
export function openVault(pin: string): Promise<Identity> {
  return invoke<Identity>("vault_open", { pin });
}

/**
 * Opens it with the key the device is holding.
 *
 * On iOS the prompt this raises is the system's own: the key is stored so that
 * it is not handed over until somebody has been recognised, so there is no
 * separate question to ask first.
 */
export function openVaultWithDevice(): Promise<Identity> {
  return invoke<Identity>("vault_open_with_device");
}

export function changeVaultPin(current: string, next: string): Promise<VaultStatus> {
  return invoke<VaultStatus>("vault_change_pin", { current, next });
}

/** Arms or disarms opening with the device. Arming needs the PIN. */
export function setVaultDevice(enabled: boolean, pin?: string): Promise<VaultStatus> {
  return invoke<VaultStatus>("vault_set_device", { enabled, pin: pin ?? null });
}

/** Takes the identity off the device. The phrase is what is left. */
export function destroyVault(): Promise<VaultStatus> {
  return invoke<VaultStatus>("vault_destroy");
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
  | "vault_no_identity"
  | "vault_no_device_key"
  | "vault_device_unproven"
  | "vault_no_device_store"
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
  "vault_no_identity",
  "vault_no_device_key",
  "vault_device_unproven",
  "vault_no_device_store",
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
