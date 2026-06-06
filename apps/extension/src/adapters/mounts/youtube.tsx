import {
  PRIMARY,
  Icon,
  btnClass,
  toggle,
  useStatus,
  shadowHost,
  isDark,
  alreadyMounted,
} from "./common.js";

const CSS = `
  :host([data-variant="watch"]) { margin-left: 8px; }
  .sukoon-root { font-family: "Roboto", "Arial", sans-serif;
    --s-bg: rgba(0,0,0,0.05); --s-bg-hover: rgba(0,0,0,0.1); --s-fg: #0f0f0f; }
  .sukoon-root.sukoon-dark { --s-bg: rgba(255,255,255,0.1); --s-bg-hover: rgba(255,255,255,0.2); --s-fg: #f1f1f1; }
  .sukoon-btn--ytwatch {
    height: 36px; padding: 0 16px 0 14px; border-radius: 18px; gap: 8px; font-size: 1.4rem; line-height: 2rem;
    color: var(--s-fg); background: var(--s-bg); transition: background-color 0.18s ease, color 0.18s ease;
  }
  .sukoon-btn--ytwatch:hover { background: var(--s-bg-hover); }
  .sukoon-btn--ytwatch.is-on { background: ${PRIMARY}; color: #fff; }
  .sukoon-btn--ytwatch.is-on:hover { background: #036b79; }
  .sukoon-btn--ytwatch.is-warn { background: transparent; border: 1px dashed var(--s-bg-hover); }
  .sukoon-btn--ytshorts {
    flex-direction: column; width: 48px; height: auto; padding: 0; gap: 6px; background: transparent; color: var(--s-fg);
  }
  .sukoon-btn--ytshorts .sukoon-icon-wrap {
    width: 48px; height: 48px; border-radius: 50%; background: var(--s-bg); transition: background-color 0.18s ease;
  }
  .sukoon-btn--ytshorts:hover .sukoon-icon-wrap { background: var(--s-bg-hover); }
  .sukoon-btn--ytshorts .sukoon-label { font-size: 1.2rem; line-height: 1.6rem; font-weight: 600; }
  .sukoon-btn--ytshorts.is-on { color: #fff; }
  .sukoon-btn--ytshorts.is-on .sukoon-icon-wrap { background: ${PRIMARY}; }
`;

function WatchButton() {
  const d = useStatus();
  return (
    <button
      type="button"
      className={btnClass("ytwatch", d)}
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

function ShortsButton() {
  const d = useStatus();
  return (
    <button
      type="button"
      className={btnClass("ytshorts", d)}
      onClick={toggle}
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

export function mount(): void {
  const path = location.pathname;
  if (path.startsWith("/watch")) {
    const row = document.querySelector("ytd-watch-metadata #top-level-buttons-computed");
    if (row && !alreadyMounted(row)) {
      const { host } = shadowHost(CSS, <WatchButton />, isDark());
      host.dataset.variant = "watch";
      row.appendChild(host);
    }
  } else if (path.startsWith("/shorts")) {
    for (const rail of document.querySelectorAll("reel-action-bar-view-model")) {
      if (!alreadyMounted(rail)) rail.prepend(shadowHost(CSS, <ShortsButton />, isDark()).host);
    }
  }
}
