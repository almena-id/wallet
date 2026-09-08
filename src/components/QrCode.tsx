import { useMemo } from "react";

import { encodeQr } from "../qr";

type QrCodeProps = {
  /** The text the code carries. */
  value: string;
  /** What a reader hears in place of the code. */
  label: string;
};

/** The quiet zone, in modules. Four is what the standard asks for. */
const QUIET = 4;

/**
 * A QR code, drawn as one path so the whole of it is a single element.
 *
 * **Black on white, in either theme.** Every other surface in the wallet turns
 * over with the system, and this one does not: a code is read by a camera, and
 * a camera wants the contrast the standard assumes. Light modules on a dark
 * plate are decoded by some scanners and not by others, and a code that works
 * on one phone is not a code — so the plate stays white and the wallet's dark
 * mode stops at its edge.
 */
export function QrCode({ value, label }: QrCodeProps) {
  const code = useMemo(() => encodeQr(value), [value]);
  const side = code.size + QUIET * 2;

  // One `d` for the lot: a rectangle per dark module, drawn in module units so
  // the viewBox does the scaling and nothing has to be rounded here.
  const path = useMemo(() => {
    const parts: string[] = [];
    for (let row = 0; row < code.size; row += 1) {
      for (let column = 0; column < code.size; column += 1) {
        if (code.modules[row][column]) {
          parts.push(`M${column + QUIET} ${row + QUIET}h1v1h-1z`);
        }
      }
    }
    return parts.join("");
  }, [code]);

  return (
    <svg
      className="qr"
      viewBox={`0 0 ${side} ${side}`}
      role="img"
      aria-label={label}
      shapeRendering="crispEdges"
    >
      <rect width={side} height={side} fill="#ffffff" />
      <path d={path} fill="#000000" />
    </svg>
  );
}
