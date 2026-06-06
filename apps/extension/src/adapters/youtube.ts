import { AbstractAdapter } from "./base.js";
import type { SiteId } from "./types.js";
import { requestMount } from "./mount-lazy.js";

export class YouTubeAdapter extends AbstractAdapter {
  readonly id: SiteId = "youtube";

  static matches(host: string): boolean {
    return /(^|\.)youtube\.com$/.test(host) || host === "youtu.be";
  }

  static videoId(url: string): string | null {
    try {
      const u = new URL(url);
      if (u.hostname === "youtu.be") return u.pathname.slice(1) || null;
      if (!/(^|\.)youtube\.com$/.test(u.hostname)) return null;
      if (u.pathname === "/watch") return u.searchParams.get("v");
      const m = u.pathname.match(/^\/shorts\/([^/?#]+)/);
      return m ? m[1]! : null;
    } catch {
      return null;
    }
  }

  keyFromUrl(url: string): string | null {
    const id = YouTubeAdapter.videoId(url);
    return id ? `yt:${id}` : null;
  }

  pickVideo(): HTMLMediaElement | null {
    const short = document.querySelector<HTMLVideoElement>(
      "ytd-reel-video-renderer[is-active] video.html5-main-video",
    );
    return (
      short ?? document.querySelector<HTMLVideoElement>("#movie_player video.html5-main-video")
    );
  }

  isPlayerVideo(el: EventTarget | null): el is HTMLMediaElement {
    return (
      el instanceof HTMLVideoElement &&
      el.closest("#movie_player, ytd-reel-video-renderer, #shorts-player") !== null
    );
  }

  mountButton(): void {
    const path = location.pathname;
    if (path.startsWith("/watch") || path.startsWith("/shorts")) {
      requestMount("youtube", this.pickVideo());
    }
  }

  onNavigate(cb: () => void): () => void {
    document.addEventListener("yt-navigate-finish", cb);
    return () => document.removeEventListener("yt-navigate-finish", cb);
  }
}
