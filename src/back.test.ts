import { beforeEach, describe, expect, it, vi } from "vitest";

const unregister = vi.fn();
let pressed: (() => void) | null = null;

vi.mock("@tauri-apps/api/app", () => ({
  onBackButtonPress: vi.fn(async (handler: () => void) => {
    pressed = handler;
    return { unregister };
  }),
}));

vi.stubGlobal("navigator", { userAgent: "Mozilla/5.0 (Linux; Android 15)" });

const { offer } = await import("./back");
const { onBackButtonPress } = await import("@tauri-apps/api/app");

describe("the system's back", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    pressed = null;
  });

  it("goes to the way back offered last, and is left to Android when none is", async () => {
    const screen = vi.fn();
    const sheet = vi.fn();
    const leaveScreen = offer({ current: screen });
    const leaveSheet = offer({ current: sheet });
    expect(onBackButtonPress).toHaveBeenCalledTimes(1);

    pressed?.();
    expect(sheet).toHaveBeenCalledTimes(1);
    expect(screen).not.toHaveBeenCalled();

    leaveSheet();
    pressed?.();
    expect(screen).toHaveBeenCalledTimes(1);

    leaveScreen();
    await Promise.resolve();
    await Promise.resolve();
    expect(unregister).toHaveBeenCalledTimes(1);
  });
});
