import type { SiteId } from "./types.js";
import { clearButton } from "./mounts/common.js";
import * as youtube from "./mounts/youtube.js";
import * as facebook from "./mounts/facebook.js";
import * as instagram from "./mounts/instagram.js";
import * as x from "./mounts/x.js";
import * as generic from "./mounts/generic.js";

export function mountFor(site: SiteId, video: HTMLMediaElement | null): void {
  if (site === "youtube") {
    youtube.mount();
    return;
  }
  if (!video) {
    clearButton();
    return;
  }
  switch (site) {
    case "facebook":
      facebook.mount(video);
      return;
    case "instagram":
      instagram.mount(video);
      return;
    case "x":
      x.mount(video);
      return;
    case "generic":
      generic.mount(video);
      return;
  }
}
