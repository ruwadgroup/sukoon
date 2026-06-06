import type { SiteAdapter, SiteId } from "./types.js";
import { bestPlayingVideo, urlNavigation } from "./shared.js";
import { requestMount } from "./mount-lazy.js";

export abstract class AbstractAdapter implements SiteAdapter {
  abstract readonly id: SiteId;
  abstract keyFromUrl(url: string): string | null;
  abstract pickVideo(): HTMLMediaElement | null;
  abstract isPlayerVideo(el: EventTarget | null): el is HTMLMediaElement;
  abstract mountButton(): void;
  abstract onNavigate(cb: () => void): () => void;

  mediaKey(): string | null {
    return this.keyFromUrl(location.href);
  }
}

export abstract class BestVideoAdapter extends AbstractAdapter {
  pickVideo(): HTMLMediaElement | null {
    return bestPlayingVideo(true);
  }

  isPlayerVideo(el: EventTarget | null): el is HTMLMediaElement {
    return el instanceof HTMLVideoElement;
  }

  mountButton(): void {
    const video = this.pickVideo();
    if (video) requestMount(this.id, video);
  }

  onNavigate(cb: () => void): () => void {
    return urlNavigation(cb);
  }
}
