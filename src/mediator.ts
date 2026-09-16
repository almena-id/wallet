import { useCallback, useState } from "react";

/**
 * The mediators this wallet can leave its messages with.
 *
 * A wallet on a phone has no address of its own, so what it hands out to a
 * relationship is a mediator's — the one chosen here — and a mediator is
 * named by its DID: resolving it is how a sender finds the mediator's own
 * keys and the address messages are posted to, so nothing else about it is
 * kept on this side.
 *
 * **The list is the wallet's own for now.** The platform publishes the ones
 * it offers at `/v1/mediators`, but this wallet talks to no API yet, and a
 * list of one does not justify starting to. When it does, that answer is what
 * replaces this constant; the choice below and where it is kept stay as they
 * are. The first one is what a wallet uses until somebody picks another.
 */
export const mediators = ["did:web:mediator.almena.id"] as const;

export type Mediator = (typeof mediators)[number];

/** The one a wallet uses when nobody has said otherwise. */
export const defaultMediator: Mediator = mediators[0];

/**
 * Where the choice is kept.
 *
 * A preference about this device, in the same store as the theme and the
 * accent, and holding nothing about the identity: it is what the *next*
 * relationship is opened with. A relationship already open keeps the mediator
 * it was opened with until it is told otherwise by protocol, which is not this
 * setting's business.
 */
const storageKey = "almena.mediator";

function isMediator(value: unknown): value is Mediator {
  return typeof value === "string" && (mediators as readonly string[]).includes(value);
}

function stored(): Mediator {
  try {
    const value = window.localStorage.getItem(storageKey);
    return isMediator(value) ? value : defaultMediator;
  } catch {
    // Private modes and locked down webviews can refuse storage entirely.
    return defaultMediator;
  }
}

export function useMediator(): {
  mediator: Mediator;
  setMediator: (mediator: Mediator) => void;
} {
  const [mediator, setMediatorState] = useState<Mediator>(() => stored());

  const setMediator = useCallback((next: Mediator) => {
    setMediatorState(next);
    try {
      window.localStorage.setItem(storageKey, next);
    } catch {
      // Losing the preference is acceptable; refusing the change is not.
    }
  }, []);

  return { mediator, setMediator };
}

/**
 * The name a mediator is shown under: the host its `did:web` is served from,
 * with any path after it.
 *
 * Derived rather than catalogued because it is an identifier and not a word:
 * `did:web:mediator.almena.id` is *mediator.almena.id* in every language.
 * The method writes a path's segments with colons and a port's colon as
 * `%3A`, and this undoes both. A DID of another method has no host to show
 * and is shown as it is.
 */
export function mediatorName(did: string): string {
  const prefix = "did:web:";
  if (!did.startsWith(prefix)) {
    return did;
  }
  return did
    .slice(prefix.length)
    .split(":")
    .map((segment) => decodeURIComponent(segment))
    .join("/");
}
