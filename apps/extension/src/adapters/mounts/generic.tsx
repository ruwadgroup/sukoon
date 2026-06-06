import { PRIMARY, Icon, btnClass, toggle, useStatus, shadowHost, mountOverlay } from "./common.js";

const CSS = `
  .sukoon-root { font-family: system-ui, sans-serif; }
  .sukoon-btn--generic {
    height: 32px; padding: 0 12px 0 10px; gap: 6px; border-radius: 9999px; font-size: 13px; font-weight: 600;
    color: #fff; background: rgba(0,0,0,0.6); backdrop-filter: blur(4px); transition: background-color 0.15s ease;
  }
  .sukoon-btn--generic .sukoon-icon { width: 18px; height: 18px; }
  .sukoon-btn--generic:hover { background: rgba(0,0,0,0.78); }
  .sukoon-btn--generic.is-on { background: ${PRIMARY}; }
  .sukoon-btn--generic.is-warn { background: rgba(120,0,0,0.7); }
`;

function GenericButton() {
  const d = useStatus();
  return (
    <button
      type="button"
      className={btnClass("generic", d)}
      onClick={toggle}
      aria-busy={d.loading}
      aria-pressed={d.on}
      title={`Sukoon — ${d.label}`}
    >
      <span className="sukoon-icon-wrap">
        <Icon d={d} />
      </span>
      <span className="sukoon-label">{d.short}</span>
    </button>
  );
}

/** Player overlay — the generic catch-all, and the fallback when a platform toolbar isn't found. */
export function showOverlay(video: HTMLMediaElement): void {
  mountOverlay(video, () => shadowHost(CSS, <GenericButton />));
}

export function mount(video: HTMLMediaElement): void {
  showOverlay(video);
}
