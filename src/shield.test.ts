import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const { hold, reset } = await import("./shield");

describe("the screen shield", () => {
  const sent = vi.fn(async (on: boolean) => on);

  beforeEach(() => {
    vi.useFakeTimers();
    sent.mockClear();
    reset(sent);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("is raised by the first screen and lowered after the last one leaves", () => {
    const pad = hold();
    const screen = hold();
    expect(sent.mock.calls).toEqual([[true]]);

    pad();
    vi.runAllTimers();
    expect(sent.mock.calls).toEqual([[true]]);

    screen();
    expect(sent.mock.calls).toEqual([[true]]);
    vi.runAllTimers();
    expect(sent.mock.calls).toEqual([[true], [false]]);
  });

  it("stays up from one screen to the next", () => {
    const phrase = hold();
    phrase();
    const quiz = hold();
    vi.runAllTimers();
    expect(sent.mock.calls).toEqual([[true]]);

    quiz();
    vi.runAllTimers();
    expect(sent.mock.calls).toEqual([[true], [false]]);
  });

  it("counts a release once, however often it is called", () => {
    const first = hold();
    const second = hold();
    first();
    first();
    vi.runAllTimers();
    expect(sent.mock.calls).toEqual([[true]]);

    second();
    vi.runAllTimers();
    expect(sent.mock.calls).toEqual([[true], [false]]);
  });

  it("shows the screen anyway where the host cannot shield it", () => {
    const refused = vi.fn(async () => {
      throw new Error("screen_shield is not a command here");
    });
    reset(refused);
    const release = hold();
    release();
    vi.runAllTimers();
    expect(refused.mock.calls).toEqual([[true], [false]]);
  });
});
