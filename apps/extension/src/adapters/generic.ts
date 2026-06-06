import { BestVideoAdapter } from "./base.js";
import type { SiteId } from "./types.js";
import { isOriginSafe } from "./shared.js";
import { requestMount } from "./mount-lazy.js";

export class GenericAdapter extends BestVideoAdapter {
  readonly id: SiteId = "generic";

  static matches(): boolean {
    return true;
  }

  keyFromUrl(url: string): string | null {
    try {
      const u = new URL(url);
      if (!/^https?:$/.test(u.protocol)) return null;
      return `web:${u.hostname}${u.pathname}`;
    } catch {
      return null;
    }
  }

  override isPlayerVideo(el: EventTarget | null): el is HTMLMediaElement {
    return el instanceof HTMLVideoElement && isOriginSafe(el);
  }

  override mountButton(): void {
    if (!location.host) return;
    const video = this.pickVideo();
    if (video) requestMount(this.id, video);
  }
}
