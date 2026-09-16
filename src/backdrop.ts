import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

import type { Theme } from "./theme";

/**
 * The colour behind the webview.
 *
 * A webview is a rectangle drawn on top of a native view, and that view has a
 * colour of its own that the page cannot declare. It shows wherever the page is
 * not painted yet — most visibly while a device is being turned, because the
 * window changes shape a moment before the page has been laid out again inside
 * it. Unset, it is white, and a white sheet flashing behind a dark wallet is
 * exactly what somebody rotating a tablet sees.
 *
 * **The stylesheet stays the only place a colour is written down.** This reads
 * back what the page is actually painting and hands that same value to the
 * window, rather than keeping a second copy of the palette over here that would
 * be wrong the first time anybody edited the first one.
 *
 * It is read from `body` rather than from the `--bg` token because a custom
 * property answers with whatever text was typed into it, while a computed
 * background answers with the colour the platform is really drawing — which is
 * the thing that has to match.
 *
 * On a computer the window's own chrome is sent the theme as well, because a
 * title bar the system draws light over a page that is dark is the same flash
 * made permanent — see `src-tauri/src/backdrop.rs`.
 *
 * It goes through this application's own command and not through Tauri's
 * `setBackgroundColor`, which is registered for desktop alone and rejects on a
 * phone — the one place any of this matters.
 */

/** Whichever way the device is leaning, when nobody has chosen for it. */
function useSystemScheme(): "dark" | "light" {
  const [scheme, setScheme] = useState<"dark" | "light">(() =>
    window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light",
  );

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const answer = () => setScheme(media.matches ? "dark" : "light");

    media.addEventListener("change", answer);
    return () => media.removeEventListener("change", answer);
  }, []);

  return scheme;
}

/** What the page is painting behind everything, as three or four numbers. */
function painted(): [number, number, number, number] | null {
  const computed = window.getComputedStyle(document.body).backgroundColor;
  const parts = computed.match(/[\d.]+/g);
  if (parts === null || parts.length < 3) {
    return null;
  }

  const [red, green, blue, alpha] = parts.map(Number);

  // An alpha the page did not ask for would let the white through again, so a
  // missing one is opaque rather than nothing.
  return [
    red,
    green,
    blue,
    alpha === undefined ? 255 : Math.round(alpha * 255),
  ];
}

/**
 * Keep the window's colour the same as the page's.
 *
 * `theme` and the system's own answer are both watched, because either can move
 * the palette: somebody choosing dark, and somebody's phone deciding it is
 * evening, arrive here the same way.
 *
 * `scanning` pauses it. The scanner makes the webview transparent so the camera
 * behind it can be seen, and a colour pushed while it is open would paint over
 * the picture.
 */
export function useBackdrop(theme: Theme, scanning: boolean): void {
  const scheme = useSystemScheme();

  useEffect(() => {
    if (scanning) {
      return;
    }

    // Read after the theme has reached the document: `getComputedStyle` settles
    // the pending style change, so this is the palette that is about to be on
    // screen and not the one leaving it.
    const colour = painted();
    if (colour === null) {
      return;
    }

    const [red, green, blue, alpha] = colour;

    // `system` is the absence of a choice, and the window is handed back to the
    // system in the same terms rather than told which way the system leans:
    // the window already follows that on its own, and telling it would stop it.
    const chosen = theme === "system" ? null : theme;

    // A platform that will not take a colour keeps the one it has. The page is
    // painted the same either way — this is only what shows around it, and it
    // is not worth a message to somebody who cannot act on it.
    void invoke("backdrop_set", {
      red,
      green,
      blue,
      alpha,
      theme: chosen,
    }).catch(() => undefined);
  }, [theme, scheme, scanning]);
}
