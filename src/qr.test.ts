import jsQR from "jsqr";
import { describe, expect, it } from "vitest";

import { encodeQr, QrTooLong } from "./qr";

/** The code drawn as a scanner would see it: 4 pixels a module, quiet zone round it. */
function decode(text: string): string | null {
  const { size, modules } = encodeQr(text);
  const scale = 4;
  const quiet = 4;
  const side = (size + quiet * 2) * scale;
  const pixels = new Uint8ClampedArray(side * side * 4).fill(255);
  for (let row = 0; row < size; row += 1) {
    for (let column = 0; column < size; column += 1) {
      if (!modules[row][column]) {
        continue;
      }
      for (let y = 0; y < scale; y += 1) {
        for (let x = 0; x < scale; x += 1) {
          const at = (((row + quiet) * scale + y) * side + (column + quiet) * scale + x) * 4;
          pixels[at] = pixels[at + 1] = pixels[at + 2] = 0;
        }
      }
    }
  }
  return jsQR(pixels, side, side)?.data ?? null;
}

describe("encodeQr", () => {
  it("draws codes a scanner reads back, in the smallest version", () => {
    expect(encodeQr("almena").size).toBe(21);
    for (const text of ["almena", "https://almena.id/", "ñandú · 🦆"]) {
      expect(decode(text)).toBe(text);
    }
  });

  it("holds an invitation-sized link", () => {
    const invitation = `almena://invite?_oob=${"x".repeat(520)}`;
    expect(decode(invitation)).toBe(invitation);
  });

  it("refuses what no version it carries can hold", () => {
    expect(() => encodeQr("x".repeat(700))).toThrow(QrTooLong);
  });
});
