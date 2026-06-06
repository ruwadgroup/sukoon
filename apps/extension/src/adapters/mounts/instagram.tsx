import {
  PRIMARY,
  Icon,
  btnClass,
  toggle,
  useStatus,
  shadowHost,
  mountInToolbar,
} from "./common.js";
import { showOverlay } from "./generic.js";

const CSS = `
  .sukoon-btn--ig { height: 40px; padding: 8px; gap: 0; border-radius: 8px; color: inherit; background: transparent; }
  .sukoon-btn--ig .sukoon-label { display: none; }
  .sukoon-btn--ig.is-on { color: ${PRIMARY}; }
`;

function InstagramButton() {
  const d = useStatus();
  return (
    <button
      type="button"
      className={btnClass("ig", d)}
      onClick={toggle}
      aria-pressed={d.on}
      title={`Sukoon — ${d.label}`}
    >
      <span className="sukoon-icon-wrap">
        <Icon d={d} />
      </span>
    </button>
  );
}

// The post action row (like / comment / share / save).
function toolbar(v: HTMLMediaElement): Element | null {
  const post = v.closest("article");
  if (!post) return null;
  for (const section of post.querySelectorAll("section")) {
    if (section.querySelector('svg[aria-label], [role="button"]')) return section;
  }
  return null;
}

export function mount(video: HTMLMediaElement): void {
  const bar = toolbar(video);
  if (bar) mountInToolbar(bar, () => shadowHost(CSS, <InstagramButton />));
  else showOverlay(video);
}
