import { describe, expect, it } from "vitest";
import { mediaKeyFromUrl } from "./index.js";

describe("mediaKeyFromUrl", () => {
  it("derives YouTube keys from watch, shorts, and youtu.be", () => {
    expect(mediaKeyFromUrl("https://www.youtube.com/watch?v=abc123")).toBe("yt:abc123");
    expect(mediaKeyFromUrl("https://www.youtube.com/shorts/XYZ")).toBe("yt:XYZ");
    expect(mediaKeyFromUrl("https://youtu.be/abc123")).toBe("yt:abc123");
    expect(mediaKeyFromUrl("https://www.youtube.com/")).toBeNull();
  });

  it("derives Facebook keys from watch, reel, and permalink videos", () => {
    expect(mediaKeyFromUrl("https://www.facebook.com/watch/?v=12345")).toBe("fb:12345");
    expect(mediaKeyFromUrl("https://www.facebook.com/reel/999")).toBe("fb:999");
    expect(mediaKeyFromUrl("https://web.facebook.com/user/videos/777")).toBe("fb:777");
  });

  it("derives Instagram keys from reels and posts", () => {
    expect(mediaKeyFromUrl("https://www.instagram.com/reel/AbC/")).toBe("ig:AbC");
    expect(mediaKeyFromUrl("https://www.instagram.com/p/XyZ/")).toBe("ig:XyZ");
  });

  it("derives X keys from status urls on x.com and twitter.com", () => {
    expect(mediaKeyFromUrl("https://x.com/user/status/123")).toBe("x:123");
    expect(mediaKeyFromUrl("https://twitter.com/i/status/456")).toBe("x:456");
  });

  it("falls back to a per-page key for any other site, null for non-http", () => {
    expect(mediaKeyFromUrl("https://example.com/video/page")).toBe("web:example.com/video/page");
    expect(mediaKeyFromUrl("chrome://extensions")).toBeNull();
    expect(mediaKeyFromUrl(undefined)).toBeNull();
  });
});
