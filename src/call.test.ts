import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// What the Rust side is asked, and the handler `call-signal` reaches.
const invoke = vi.fn<(command: string, args?: unknown) => Promise<unknown>>(async () => undefined);
let signal: ((event: { payload: unknown }) => void) | null = null;

vi.mock("@tauri-apps/api/core", () => ({ invoke: (c: string, a?: unknown) => invoke(c, a) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_name: string, handler: (event: { payload: unknown }) => void) => {
    signal = handler;
    return () => undefined;
  }),
}));
vi.mock("./contacts", () => ({ listContacts: vi.fn(async () => [{ id: "c1", name: "Ana" }]) }));
vi.mock("./ring", () => ({ takeRung: vi.fn(() => null), settleRung: vi.fn() }));
vi.mock("./mediator", () => ({ errorCode: () => "messaging_unknown" }));

const { calls } = await import("./call");

const offer = (contact: string, call: string) => ({
  contact,
  call,
  signal: { kind: "offer", media: "audio", sdp: "v=0\r\n" },
});
const hangup = (contact: string, call: string, reason: string) => ({
  contact,
  call,
  signal: { kind: "hangup", reason },
});

/** The hang-ups the wallet sent, as `[contact, call, reason]`. */
function hangupsSent() {
  return invoke.mock.calls
    .filter(([command]) => command === "call_send")
    .map(([, args]) => args as { id: string; call: string; signal: { kind: string; reason?: string } })
    .filter((args) => args.signal.kind === "hangup")
    .map((args) => [args.id, args.call, args.signal.reason]);
}

describe("calls", () => {
  let detach: () => void;

  beforeEach(() => {
    vi.useFakeTimers();
    invoke.mockClear();
    detach = calls.attach();
  });

  afterEach(() => {
    detach();
    vi.runAllTimers();
    vi.useRealTimers();
  });

  it("rings with an offer, and declining says so", async () => {
    signal?.({ payload: offer("c1", "call-1") });
    expect(calls.snapshot()?.phase).toBe("incoming");
    await vi.waitFor(() => expect(calls.snapshot()?.name).toBe("Ana"));

    calls.decline();
    expect(calls.snapshot()?.phase).toBe("ended");
    expect(hangupsSent()).toEqual([["c1", "call-1", "declined"]]);
    // Gone from the screen a moment later.
    vi.runAllTimers();
    expect(calls.snapshot()).toBeNull();
  });

  it("answers a second caller with busy, and leaves the first ringing", () => {
    signal?.({ payload: offer("c1", "call-1") });
    signal?.({ payload: offer("c2", "call-2") });
    expect(calls.snapshot()?.contact).toBe("c1");
    expect(hangupsSent()).toEqual([["c2", "call-2", "busy"]]);
  });

  it("ends on the other side's hang-up, only for the call it is", () => {
    signal?.({ payload: offer("c1", "call-1") });
    signal?.({ payload: hangup("c1", "another", "cancelled") });
    expect(calls.snapshot()?.phase).toBe("incoming");
    signal?.({ payload: hangup("c1", "call-1", "cancelled") });
    expect(calls.snapshot()?.phase).toBe("ended");
    expect(calls.snapshot()?.outcome).toEqual({ kind: "hangup", reason: "cancelled", mine: false });
    // Nothing is said back to a hang-up.
    expect(hangupsSent()).toEqual([]);
  });

  it("stops ringing after a minute as missed, saying nothing", () => {
    signal?.({ payload: offer("c1", "call-1") });
    vi.advanceTimersByTime(60_000);
    expect(calls.snapshot()?.outcome).toEqual({ kind: "missed" });
    expect(hangupsSent()).toEqual([]);
  });

  it("ignores an answer to a call it did not place", () => {
    signal?.({ payload: { contact: "c1", call: "call-9", signal: { kind: "answer", sdp: "v=0" } } });
    expect(calls.snapshot()).toBeNull();
  });
});
