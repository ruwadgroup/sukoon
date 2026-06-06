import type { SiteId } from "./types.js";

let modulePromise: Promise<typeof import("./mount.js")> | null = null;

export function requestMount(site: SiteId, video: HTMLMediaElement | null): void {
  modulePromise ??= import("./mount.js");
  void modulePromise.then((m) => m.mountFor(site, video)).catch(() => {});
}
