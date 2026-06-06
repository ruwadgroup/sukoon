import type { AdapterClass, SiteAdapter } from "./types.js";
import { hostOf } from "./shared.js";
import { YouTubeAdapter } from "./youtube.js";
import { FacebookAdapter } from "./facebook.js";
import { InstagramAdapter } from "./instagram.js";
import { XAdapter } from "./x.js";
import { GenericAdapter } from "./generic.js";

const REGISTRY: AdapterClass[] = [YouTubeAdapter, FacebookAdapter, InstagramAdapter, XAdapter];

function classFor(host: string): AdapterClass {
  return REGISTRY.find((A) => A.matches(host)) ?? GenericAdapter;
}

export function pickAdapter(host: string): SiteAdapter {
  return new (classFor(host))();
}

export function mediaKeyFromUrl(url: string | undefined): string | null {
  if (!url) return null;
  return new (classFor(hostOf(url)))().keyFromUrl(url);
}
