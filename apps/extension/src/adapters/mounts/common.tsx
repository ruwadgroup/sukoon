import { useSyncExternalStore, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { buttonStore } from "../../button-store.js";
import type { GraphStatus } from "../../audio-graph.js";

export const PRIMARY = "#025763";
const HOST_TAG = "sukoon-host";

export interface Display {
  label: string;
  short: string;
  on: boolean;
  loading: boolean;
  warn: boolean;
}

export function displayFor(status: GraphStatus): Display {
  switch (status) {
    case "loading":
    case "buffering":
      return { label: "Preparing…", short: "Prep…", on: true, loading: true, warn: false };
    case "active":
      return {
        label: "Instruments reduced",
        short: "Reduced",
        on: true,
        loading: false,
        warn: false,
      };
    case "unavailable":
      return { label: "Unavailable", short: "Retry", on: false, loading: false, warn: true };
    default:
      return { label: "Reduce music", short: "Music", on: false, loading: false, warn: false };
  }
}

export function useStatus(): Display {
  return displayFor(useSyncExternalStore(buttonStore.subscribe, buttonStore.get, buttonStore.get));
}

export const toggle = (): void => buttonStore.toggle();

export function btnClass(variant: string, d: Display): string {
  return [
    "sukoon-btn",
    `sukoon-btn--${variant}`,
    d.on && "is-on",
    d.loading && "is-loading",
    d.warn && "is-warn",
  ]
    .filter(Boolean)
    .join(" ");
}

export function Spinner() {
  return (
    <svg
      className="sukoon-icon sukoon-spin"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M21 12a9 9 0 1 1-6.219-8.56" />
    </svg>
  );
}

export function MusicIcon({ muted }: { muted: boolean }) {
  return (
    <svg className="sukoon-icon" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M12 3v10.55A4 4 0 1 0 14 17V7h4V3h-6Z" />
      {muted && (
        <path
          d="M4 4 L20 20"
          stroke="currentColor"
          strokeWidth="2.2"
          strokeLinecap="round"
          fill="none"
        />
      )}
    </svg>
  );
}

export function Icon({ d }: { d: Display }) {
  return d.loading ? <Spinner /> : <MusicIcon muted={d.on} />;
}

const BASE_CSS = `
  :host { display: inline-flex; }
  .sukoon-root { display: inline-flex; -webkit-font-smoothing: antialiased; }
  .sukoon-icon { display: block; width: 24px; height: 24px; }
  .sukoon-icon-wrap { display: inline-flex; align-items: center; justify-content: center; }
  .sukoon-label { font-weight: 500; }
  .sukoon-spin { animation: sukoon-spin 0.9s linear infinite; transform-origin: center; }
  @keyframes sukoon-spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .sukoon-spin { animation-duration: 2s; } }
  .sukoon-btn {
    font-family: inherit; display: inline-flex; align-items: center; justify-content: center;
    box-sizing: border-box; border: none; cursor: pointer; white-space: nowrap;
  }
  .sukoon-btn:focus-visible { outline: 2px solid ${PRIMARY}; outline-offset: 2px; }
`;

export function isDark(): boolean {
  if (document.documentElement.hasAttribute("dark")) return true;
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? true;
}

export type Mounted = { host: HTMLElement; root: Root };

export function shadowHost(css: string, element: ReactNode, dark = false): Mounted {
  const host = document.createElement(HOST_TAG);
  const shadow = host.attachShadow({ mode: "open" });
  const style = document.createElement("style");
  style.textContent = BASE_CSS + css;
  const mount = document.createElement("div");
  mount.className = dark ? "sukoon-root sukoon-dark" : "sukoon-root";
  shadow.append(style, mount);
  const root = createRoot(mount);
  root.render(element);
  return { host, root };
}

export function alreadyMounted(container: Element): boolean {
  return container.querySelector(`:scope > ${HOST_TAG}`) !== null;
}

let active: { mounted: Mounted; target: Element | HTMLMediaElement } | null = null;

export function clearButton(): void {
  if (!active) return;
  active.mounted.root.unmount();
  active.mounted.host.remove();
  active = null;
}

export function mountInToolbar(bar: Element, build: () => Mounted): void {
  if (active && active.target === bar && bar.contains(active.mounted.host)) return;
  clearButton();
  const mounted = build();
  bar.appendChild(mounted.host);
  active = { mounted, target: bar };
}

// Anchor inside the player so the button scrolls and clips with the video — no scroll listener
// (smooth) and no page-wide z-index fight.
function overlayContainer(video: HTMLMediaElement): HTMLElement {
  const parent = (video.parentElement ?? document.body) as HTMLElement;
  if (getComputedStyle(parent).position === "static") parent.style.position = "relative";
  return parent;
}

export function mountOverlay(video: HTMLMediaElement, build: () => Mounted): void {
  if (active && active.target === video && active.mounted.host.isConnected) return;
  clearButton();
  const container = overlayContainer(video);
  const mounted = build();
  const s = mounted.host.style;
  s.position = "absolute";
  s.top = "8px";
  s.right = "8px";
  s.zIndex = "2147483000";
  container.appendChild(mounted.host);
  active = { mounted, target: video };
}
