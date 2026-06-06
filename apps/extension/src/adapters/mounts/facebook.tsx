import { Icon, btnClass, toggle, useStatus, shadowHost, mountInToolbar } from "./common.js";
import { showOverlay } from "./generic.js";

const CSS = `
  .sukoon-root { font-family: inherit; }
  .sukoon-btn--fb {
    height: 36px; padding: 0 12px; gap: 6px; border-radius: 6px; font-size: 15px; font-weight: 600; color: #606770;
    background: transparent; transition: background-color 0.15s ease, color 0.15s ease;
  }
  .sukoon-btn--fb .sukoon-icon { width: 20px; height: 20px; }
  .sukoon-btn--fb:hover { background: rgba(0,0,0,0.05); }
  .sukoon-btn--fb.is-on { color: #1877f2; }
  .sukoon-btn--fb.is-warn { color: #e41e3f; }
`;

function FacebookButton() {
  const d = useStatus();
  return (
    <button
      type="button"
      className={btnClass("fb", d)}
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

// The like/comment/share row: walk up from a reaction button to the row that holds them.
function toolbar(v: HTMLMediaElement): Element | null {
  const post = v.closest('[role="article"]');
  if (!post) return null;
  const explicit = post.querySelector('[role="toolbar"]');
  if (explicit) return explicit;
  const buttons = post.querySelectorAll('[role="button"][aria-label]');
  if (buttons.length < 3) return null;
  let row = buttons[0]!.parentElement;
  while (row && row !== post) {
    if (row.querySelectorAll(':scope [role="button"][aria-label]').length >= 3) return row;
    row = row.parentElement;
  }
  return null;
}

export function mount(video: HTMLMediaElement): void {
  const bar = toolbar(video);
  if (bar) mountInToolbar(bar, () => shadowHost(CSS, <FacebookButton />));
  else showOverlay(video);
}
