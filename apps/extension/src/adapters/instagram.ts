import { BestVideoAdapter } from "./base.js";
import type { SiteId } from "./types.js";

export class InstagramAdapter extends BestVideoAdapter {
  readonly id: SiteId = "instagram";

  static matches(host: string): boolean {
    return /(^|\.)instagram\.com$/.test(host);
  }

  keyFromUrl(url: string): string | null {
    try {
      const u = new URL(url);
      const m =
        u.pathname.match(/\/reels?\/([^/]+)/) ??
        u.pathname.match(/\/p\/([^/]+)/) ??
        u.pathname.match(/\/tv\/([^/]+)/) ??
        u.pathname.match(/\/stories\/[^/]+\/([^/]+)/);
      return m ? `ig:${m[1]}` : `ig:${u.pathname}`;
    } catch {
      return null;
    }
  }
}
