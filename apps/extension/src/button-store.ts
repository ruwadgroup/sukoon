/**
 * Tiny external store shared by every injected button instance (watch page + Shorts can both be
 * mounted) so they all reflect one source of truth via React's useSyncExternalStore.
 *
 * The content script pushes engine status in with `setStatus`; the buttons call `toggle()` on click,
 * routed to the handler the content script registers with `setToggleHandler`.
 */

import type { GraphStatus } from "./audio-graph.js";

type Listener = () => void;

let status: GraphStatus = "off";
let toggleHandler: (() => void) | null = null;
const listeners = new Set<Listener>();

function notify(): void {
  for (const listener of listeners) listener();
}

export const buttonStore = {
  get: (): GraphStatus => status,
  subscribe: (listener: Listener): (() => void) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
  setStatus: (next: GraphStatus): void => {
    if (next === status) return;
    status = next;
    notify();
  },
  setToggleHandler: (handler: () => void): void => {
    toggleHandler = handler;
  },
  toggle: (): void => {
    toggleHandler?.();
  },
};
