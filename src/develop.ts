import { invoke } from "@tauri-apps/api/core";

/**
 * The log, and getting it off the device it was written on.
 *
 * The interface never opens a file: it asks where the log is, asks for it to be
 * handed to the system's share sheet, and — when somebody wants a copy
 * somewhere else — names the place the system's own dialog answered with. It
 * never reads or writes a byte itself. Which directory the log is in, and what
 * can be done with it on this device, are answers from the Rust side rather
 * than a guess made from the user agent.
 */

/** One of the files the log is spread across. */
export type LogFile = {
  name: string;
  bytes: number;
};

/** What the person holding this device can actually do with the log. */
export type Reach = {
  /** A file manager opens the directory itself. Not Android. */
  browsable: boolean;
  /** There is a system share sheet here to hand the file to. */
  shareable: boolean;
  /** There is a save dialog, so a copy can be put where the person chooses. */
  savable: boolean;
};

/** Where the log is and what is in it, as the Development section shows it. */
export type Logs = {
  /** The directory, spelled out: on a phone it is the only way to say where to look. */
  directory: string;
  files: LogFile[];
  totalBytes: number;
  reach: Reach;
};

/** Where the log is, and how much of it there is. */
export function readLogs(): Promise<Logs> {
  return invoke<Logs>("develop_logs");
}

/**
 * Gathers the log and hands it to whatever this device shares files with.
 *
 * The header is prose and so it comes from here: the Rust side answers in codes
 * and never in sentences, and this file is one somebody hands to somebody else,
 * so the first thing it should say is what is in it — in the language the
 * person reading it chose.
 */
export function shareLogs(header: string, title: string): Promise<void> {
  return invoke<void>("develop_logs_share", { header, title });
}

/**
 * Gathers the log and writes a copy where the person said to put it.
 *
 * `destination` is whatever the save dialog answered with — a path on a
 * computer and on iOS, a `content://` URI on Android. It is passed straight
 * through: which of the two it is is the device's business, not this file's.
 */
export function saveLogsTo(destination: string, header: string): Promise<void> {
  return invoke<void>("develop_logs_save_to", { destination, header });
}

/**
 * The error codes the develop commands answer with. They are codes and not
 * sentences, so this side says them in the language somebody is reading.
 */
export type DevelopErrorCode =
  | "develop_no_logs"
  | "develop_unreadable"
  | "develop_storage"
  | "develop_no_share"
  | "develop_cancelled"
  | "develop_empty"
  | "develop_unknown";

const CODES: DevelopErrorCode[] = [
  "develop_no_logs",
  "develop_unreadable",
  "develop_storage",
  "develop_no_share",
  "develop_cancelled",
  "develop_empty",
];

/** Whatever a rejected command threw, as a code this interface has a word for. */
export function errorCode(error: unknown): DevelopErrorCode {
  return typeof error === "string" && (CODES as string[]).includes(error)
    ? (error as DevelopErrorCode)
    : "develop_unknown";
}
