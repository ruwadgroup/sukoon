import { BestVideoAdapter } from "./base.js";
import type { SiteId } from "./types.js";

export class FacebookAdapter extends BestVideoAdapter {
  readonly id: SiteId = "facebook";

  static matches(host: string): boolean {
    return /(^|\.)facebook\.com$/.test(host) || host === "fb.watch";
  }

  keyFromUrl(url: string): string | null {
    try {
      const u = new URL(url);
      if (u.hostname === "fb.watch") return `fb:${u.pathname.replace(/\//g, "") || u.pathname}`;
      const v = u.searchParams.get("v");
      if (v) return `fb:${v}`;
      const m =
        u.pathname.match(/\/reel\/(\d+)/) ??
        u.pathname.match(/\/videos\/(?:[^/]+\/)?(\d+)/) ??
        u.pathname.match(/\/stories\/(\d+)/);
      return m ? `fb:${m[1]}` : `fb:${u.pathname}`;
    } catch {
      return null;
    }
  }
}
