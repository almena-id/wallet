import { useCallback, useState } from "react";

/**
 * A tab's screens, as a stack: the one on top is shown, a screen it leads to
 * is pushed over it, and its way back — the arrow, Android's back — takes it
 * off again. The first screen is never taken off: there is nothing under it.
 *
 * Screens are not URLs here; a stack is all the history a wallet needs, and it
 * is what lets "back" mean where somebody came from without each screen
 * remembering it.
 */
export type Stack<View> = readonly View[];

export function pushed<View>(stack: Stack<View>, view: View): Stack<View> {
  return [...stack, view];
}

export function popped<View>(stack: Stack<View>): Stack<View> {
  return stack.length > 1 ? stack.slice(0, -1) : stack;
}

export function replaced<View>(stack: Stack<View>, view: View): Stack<View> {
  return [...stack.slice(0, -1), view];
}

export function useStack<View>(initial: Stack<View> | (() => Stack<View>)) {
  const [stack, setStack] = useState<Stack<View>>(initial);
  const push = useCallback((view: View) => setStack((current) => pushed(current, view)), []);
  const pop = useCallback(() => setStack((current) => popped(current)), []);
  const replace = useCallback((view: View) => setStack((current) => replaced(current, view)), []);
  return { top: stack[stack.length - 1], push, pop, replace };
}
