import {
  PRIMARY,
  Icon,
  btnClass,
  toggle,
  useStatus,
  shadowHost,
  isDark,
  mountInToolbar,
} from "./common.js";
import { showOverlay } from "./generic.js";

const CSS = `
  .sukoon-root { font-family: inherit; color: inherit; }
  .sukoon-btn--x {
    height: 34px; padding: 0 14px 0 12px; gap: 6px; border-radius: 9999px; font-size: 14px; color: inherit;
    background: transparent; transition: background-color 0.15s ease, color 0.15s ease;
  }
  .sukoon-btn--x .sukoon-icon { width: 20px; height: 20px; }
  .sukoon-btn--x:hover { background: rgba(29,155,240,0.1); color: rgb(29,155,240); }
  .sukoon-btn--x.is-on { color: ${PRIMARY}; }
  .sukoon-btn--x.is-warn { color: #f4212e; }
`;

function XButton() {
  const d = useStatus();
  return (
    <button
      type="button"
      className={btnClass("x", d)}
      onClick={toggle}
      aria-pressed={d.on}
      title={`Sukoon — ${d.label}`}
    >
      <span className="sukoon-icon-wrap">
        <Icon d={d} />
      </span>
      <span className="sukoon-label">{d.label}</span>
    </button>
  );
}

// The tweet action bar (reply / repost / like / bookmark / share).
function toolbar(v: HTMLMediaElement): Element | null {
  return (
    v.closest("article")?.querySelector('div[role="group"]') ?? v.closest('[role="group"]') ?? null
  );
}

export function mount(video: HTMLMediaElement): void {
  const bar = toolbar(video);
  if (bar) mountInToolbar(bar, () => shadowHost(CSS, <XButton />, isDark()));
  else showOverlay(video);
}
