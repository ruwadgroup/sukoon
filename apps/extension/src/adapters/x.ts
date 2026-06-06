import { BestVideoAdapter } from "./base.js";
import type { SiteId } from "./types.js";

export class XAdapter extends BestVideoAdapter {
  readonly id: SiteId = "x";

  static matches(host: string): boolean {
    return /(^|\.)x\.com$/.test(host) || /(^|\.)twitter\.com$/.test(host);
  }

  keyFromUrl(url: string): string | null {
    try {
      const u = new URL(url);
      const m = u.pathname.match(/\/status(?:es)?\/(\d+)/);
      return m ? `x:${m[1]}` : `x:${u.pathname}`;
    } catch {
      return null;
    }
  }
}
