import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { listContacts } from "./contacts";
import { errorCode } from "./mediator";
import { settleRung, takeRung } from "./ring";

/**
 * Calls: one at a time, one to one, audio or video — `SPEC.md` §3.
 *
 * **The media is here; the words are on the Rust side.** The
 * `RTCPeerConnection`, the microphone and the camera belong to the webview,
 * so the call is run from this module. What the two wallets say to set it up
 * — offer, answer, hang-up — is sent and received by
 * `src-tauri/src/messaging/call.rs`, over the relationship's pairwise, where
 * the keys are; it arrives here as the `call-signal` event.
 *
 * **Relayed, always.** The connection takes only relayed candidates, through
 * the TURN server of this wallet's own mediator, so the other side never
 * learns this device's address. There is no trickle: each side gathers its
 * candidates before it sends its description.
 */

export type Media = "audio" | "video";

export type HangupReason = "ended" | "cancelled" | "unanswered" | "declined" | "busy" | "failed";

type Signal =
  | { kind: "offer"; media: Media; sdp: string }
  | { kind: "answer"; sdp: string }
  | { kind: "hangup"; reason: HangupReason };

type Incoming = { contact: string; call: string; signal: Signal };

export type CallPhase = "outgoing" | "incoming" | "connecting" | "active" | "ended";

/** A call as the screen draws it. */
export type CallView = {
  phase: CallPhase;
  /** The conversation it is with. */
  contact: string;
  name: string;
  media: Media;
  /** When the media started flowing, in milliseconds since the epoch. */
  since: number | null;
  micOn: boolean;
  cameraOn: boolean;
  local: MediaStream | null;
  remote: MediaStream | null;
  /** How it ended: what the other side said, or what went wrong here. */
  outcome: Outcome | null;
};

export type Outcome =
  | { kind: "hangup"; reason: HangupReason; mine: boolean }
  | { kind: "missed" }
  | { kind: "error"; code: CallErrorCode };

/** What can go wrong on this side, beyond what the Rust side reports. */
export type CallErrorCode =
  | "call_media"
  | "call_relay"
  | "call_failed"
  | ReturnType<typeof errorCode>;

/** How long the caller rings before giving up; the offer itself expires at 60. */
const RING_MS = 45_000;
/** How long the callee rings: as long as the offer lives. */
const INCOMING_MS = 60_000;
/** How long gathering relayed candidates may take. */
const GATHER_MS = 8_000;
/** How long an ended call stays on screen. */
const ENDED_MS = 2_500;

/**
 * Whether this webview can place a call at all. Some Linux builds of
 * WebKitGTK come without WebRTC; there the call buttons are not drawn.
 */
export const callsSupported =
  typeof RTCPeerConnection !== "undefined" &&
  typeof navigator !== "undefined" &&
  typeof navigator.mediaDevices?.getUserMedia === "function";

class Calls {
  private view: CallView | null = null;
  private readonly listeners = new Set<() => void>();
  private pc: RTCPeerConnection | null = null;
  private call: string | null = null;
  /** The offer being rung, until it is answered. */
  private offer: string | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  /** Bumped by every new call and every ending, so a step that was awaiting
   * something can tell it no longer belongs to the current call. */
  private generation = 0;

  readonly subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  readonly snapshot = () => this.view;

  /** Starts listening for signals. Returns the function that stops, and ends any call. */
  attach(): () => void {
    const stopped = listen<Incoming>("call-signal", (event) => this.received(event.payload));
    return () => {
      void stopped.then((stop) => stop()).catch(() => undefined);
      this.hangUp();
    };
  }

  /** Calls the contact of the conversation `contact`. */
  async start(contact: string, name: string, media: Media) {
    if (this.view && this.view.phase !== "ended") {
      return;
    }
    const generation = this.fresh();
    this.set({
      phase: "outgoing",
      contact,
      name,
      media,
      since: null,
      micOn: true,
      cameraOn: media === "video",
      local: null,
      remote: null,
      outcome: null,
    });
    try {
      const pc = await this.connect(generation, media);
      const offer = await pc.createOffer();
      await pc.setLocalDescription(offer);
      const sdp = await gathered(pc);
      this.check(generation);
      const call = await invoke<string>("call_send", {
        id: contact,
        call: null,
        signal: { kind: "offer", media, sdp } satisfies Signal,
      });
      // Cancelled while the offer was on its way: it has arrived all the same
      // and is ringing there, so it is hung up rather than left to ring out.
      if (generation !== this.generation) {
        void send(contact, call, { kind: "hangup", reason: "cancelled" });
      }
      this.check(generation);
      this.call = call;
      this.arm(RING_MS, () => this.end({ kind: "hangup", reason: "unanswered", mine: true }, "unanswered"));
    } catch (failure) {
      this.failed(generation, failure);
    }
  }

  /** Answers the call that is ringing. */
  async accept() {
    const view = this.view;
    const offer = this.offer;
    if (view?.phase !== "incoming" || offer === null || this.call === null) {
      return;
    }
    const generation = this.generation;
    const call = this.call;
    this.clear();
    this.offer = null;
    this.set({ ...view, phase: "connecting" });
    try {
      const pc = await this.connect(generation, view.media);
      await pc.setRemoteDescription({ type: "offer", sdp: offer });
      const answer = await pc.createAnswer();
      await pc.setLocalDescription(answer);
      const sdp = await gathered(pc);
      this.check(generation);
      await invoke<string>("call_send", {
        id: view.contact,
        call,
        signal: { kind: "answer", sdp } satisfies Signal,
      });
    } catch (failure) {
      this.failed(generation, failure);
    }
  }

  /** Declines the call that is ringing. */
  decline() {
    if (this.view?.phase === "incoming") {
      this.end({ kind: "hangup", reason: "declined", mine: true }, "declined");
    }
  }

  /** Hangs up whatever call there is. */
  hangUp() {
    const phase = this.view?.phase;
    if (phase === "incoming") {
      this.decline();
    } else if (phase === "outgoing") {
      this.end({ kind: "hangup", reason: "cancelled", mine: true }, "cancelled");
    } else if (phase === "connecting" || phase === "active") {
      this.end({ kind: "hangup", reason: "ended", mine: true }, "ended");
    }
  }

  toggleMic() {
    this.toggle("audio");
  }

  toggleCamera() {
    this.toggle("video");
  }

  private toggle(kind: "audio" | "video") {
    const view = this.view;
    if (!view?.local) {
      return;
    }
    const on = kind === "audio" ? !view.micOn : !view.cameraOn;
    for (const track of view.local.getTracks()) {
      if (track.kind === kind) {
        track.enabled = on;
      }
    }
    this.set(kind === "audio" ? { ...view, micOn: on } : { ...view, cameraOn: on });
  }

  private received({ contact, call, signal }: Incoming) {
    const view = this.view;
    const busy = view !== null && view.phase !== "ended";
    switch (signal.kind) {
      case "offer": {
        if (busy && view.contact === contact && view.phase === "outgoing" && this.pc?.remoteDescription == null) {
          // Both called at once: the lower id is kept (`SPEC.md` §3.3). Ours
          // wins by being ignored on the other side; theirs wins by ours
          // being dropped without a word — as does one not sent yet.
          if (this.call !== null && this.call < call) {
            return;
          }
          this.teardown();
        } else if (busy) {
          void send(contact, call, { kind: "hangup", reason: "busy" });
          return;
        }
        this.ring(contact, call, signal.media, signal.sdp);
        return;
      }
      case "answer": {
        if (call !== this.call || view?.phase !== "outgoing" || !this.pc) {
          return;
        }
        this.clear();
        this.set({ ...view, phase: "connecting" });
        const generation = this.generation;
        this.pc
          .setRemoteDescription({ type: "answer", sdp: signal.sdp })
          .catch((failure) => this.failed(generation, failure));
        return;
      }
      case "hangup": {
        if (call === this.call && busy) {
          this.end({ kind: "hangup", reason: signal.reason, mine: false });
        }
        return;
      }
    }
  }

  private ring(contact: string, call: string, media: Media, sdp: string) {
    this.fresh();
    this.call = call;
    this.offer = sdp;
    this.set({
      phase: "incoming",
      contact,
      name: "",
      media,
      since: null,
      micOn: true,
      cameraOn: media === "video",
      local: null,
      remote: null,
      outcome: null,
    });
    // The contact's name, as the conversation shows it.
    void listContacts()
      .then((contacts) => {
        const known = contacts.find((c) => c.id === contact);
        if (known && this.call === call && this.view) {
          this.set({ ...this.view, name: known.name });
        }
      })
      .catch(() => undefined);
    this.arm(INCOMING_MS, () => this.end({ kind: "missed" }));
    // Rung by the phone while the wallet was closed: it rings here now, so
    // the phone stops; and if the person answered there, it is answered.
    const rung = takeRung();
    if (rung !== null) {
      settleRung();
      if (rung === "answer") {
        void this.accept();
      }
    }
  }

  /** The microphone (and camera), the relay, and a connection with both. */
  private async connect(generation: number, media: Media): Promise<RTCPeerConnection> {
    let local: MediaStream;
    try {
      local = await navigator.mediaDevices.getUserMedia({ audio: true, video: media === "video" });
    } catch {
      throw new CallError("call_media");
    }
    if (generation !== this.generation) {
      local.getTracks().forEach((track) => track.stop());
      throw new Stale();
    }
    this.set({ ...this.current(), local });

    const iceServers = await invoke<RTCIceServer[]>("call_ice_servers");
    this.check(generation);
    const pc = new RTCPeerConnection({ iceServers, iceTransportPolicy: "relay" });
    this.pc = pc;
    for (const track of local.getTracks()) {
      pc.addTrack(track, local);
    }
    pc.ontrack = (event) => {
      if (this.pc !== pc) {
        return;
      }
      const remote = event.streams[0] ?? new MediaStream([event.track]);
      this.set({ ...this.current(), remote });
    };
    pc.onconnectionstatechange = () => {
      if (this.pc !== pc) {
        return;
      }
      if (pc.connectionState === "connected" && this.view?.phase === "connecting") {
        this.set({ ...this.view, phase: "active", since: Date.now() });
      } else if (pc.connectionState === "failed") {
        this.end({ kind: "error", code: "call_failed" }, "failed");
      }
    };
    return pc;
  }

  private failed(generation: number, failure: unknown) {
    if (failure instanceof Stale || generation !== this.generation) {
      return;
    }
    const code: CallErrorCode = failure instanceof CallError ? failure.code : errorCode(failure);
    this.end({ kind: "error", code }, "failed");
  }

  /** Ends the call, telling the other side `reason` when there is one to tell. */
  private end(outcome: Outcome, reason?: HangupReason) {
    const view = this.view;
    if (!view || view.phase === "ended") {
      return;
    }
    if (reason && this.call) {
      void send(view.contact, this.call, { kind: "hangup", reason });
    }
    this.teardown();
    this.set({ ...view, phase: "ended", local: null, remote: null, outcome });
    const generation = this.generation;
    this.arm(ENDED_MS, () => {
      if (generation === this.generation) {
        this.set(null);
      }
    });
  }

  /** Lets go of the connection, the devices and the timers. */
  private teardown() {
    this.generation += 1;
    this.clear();
    this.pc?.close();
    this.pc = null;
    this.view?.local?.getTracks().forEach((track) => track.stop());
    this.call = null;
    this.offer = null;
  }

  /** A clean slate for a new call. */
  private fresh(): number {
    this.teardown();
    return this.generation;
  }

  private check(generation: number) {
    if (generation !== this.generation) {
      throw new Stale();
    }
  }

  private arm(ms: number, then: () => void) {
    this.clear();
    this.timer = setTimeout(then, ms);
  }

  private clear() {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }

  private current(): CallView {
    if (!this.view) {
      throw new Stale();
    }
    return this.view;
  }

  private set(view: CallView | null) {
    this.view = view;
    this.listeners.forEach((listener) => listener());
  }
}

class CallError extends Error {
  constructor(readonly code: CallErrorCode) {
    super(code);
  }
}

/** The call this step belonged to is over. */
class Stale extends Error {}

function send(id: string, call: string, signal: Signal): Promise<string | void> {
  return invoke<string>("call_send", { id, call, signal }).catch(() => undefined);
}

/**
 * The local description once every candidate is in it — or once gathering
 * has had its time — with at least one relayed candidate, since that is the
 * only kind that may be used.
 */
async function gathered(pc: RTCPeerConnection): Promise<string> {
  if (pc.iceGatheringState !== "complete") {
    await new Promise<void>((resolve) => {
      const done = () => {
        if (pc.iceGatheringState === "complete") {
          pc.removeEventListener("icegatheringstatechange", done);
          clearTimeout(timer);
          resolve();
        }
      };
      const timer = setTimeout(() => {
        pc.removeEventListener("icegatheringstatechange", done);
        resolve();
      }, GATHER_MS);
      pc.addEventListener("icegatheringstatechange", done);
    });
  }
  const sdp = pc.localDescription?.sdp ?? "";
  if (!sdp.includes(" typ relay")) {
    throw new CallError("call_relay");
  }
  return sdp;
}

export const calls = new Calls();

/** The call there is, if any. */
export function useCall(): CallView | null {
  return useSyncExternalStore(calls.subscribe, calls.snapshot);
}
