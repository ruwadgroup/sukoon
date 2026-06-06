export function isOriginSafe(media: HTMLMediaElement): boolean {
  const src = media.currentSrc || media.src;
  if (!src) return true;
  if (/^(blob:|data:|mediastream:)/i.test(src)) return true;
  try {
    return new URL(src, location.href).origin === location.origin;
  } catch {
    return true;
  }
}

function isVisibleVideo(el: HTMLVideoElement): boolean {
  const r = el.getBoundingClientRect();
  if (r.width * r.height < 1) return false;
  return r.bottom > 0 && r.top < innerHeight && r.right > 0 && r.left < innerWidth;
}

export function bestPlayingVideo(originSafe = false): HTMLVideoElement | null {
  let best: HTMLVideoElement | null = null;
  let bestScore = 0;
  for (const v of document.querySelectorAll<HTMLVideoElement>("video")) {
    if (originSafe && !isOriginSafe(v)) continue;
    if (!isVisibleVideo(v)) continue;
    const r = v.getBoundingClientRect();
    const playing = !v.paused && !v.ended && v.readyState >= 2;
    const score = r.width * r.height * (playing ? 1_000_000 : 1);
    if (score > bestScore) {
      bestScore = score;
      best = v;
    }
  }
  return best;
}

export function urlNavigation(cb: () => void, intervalMs = 700): () => void {
  let last = location.href;
  const fire = () => {
    if (location.href === last) return;
    last = location.href;
    cb();
  };
  window.addEventListener("popstate", fire);
  window.addEventListener("hashchange", fire);
  const poll = window.setInterval(fire, intervalMs);
  return () => {
    window.removeEventListener("popstate", fire);
    window.removeEventListener("hashchange", fire);
    window.clearInterval(poll);
  };
}

export function hostOf(url: string | undefined): string {
  if (!url) return "";
  try {
    return new URL(url).hostname;
  } catch {
    return "";
  }
}
