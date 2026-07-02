/**
 * Popup — a per-video Off/On toggle for the **currently active tab's video**. Scoped to that video id
 * (persisted via prefs.ts), so it doesn't affect other videos or tabs. On a non-video page the toggle
 * edits the global default that new videos inherit.
 */

import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/outfit";
import { Logo } from "@sukoon/ui/logo";
import { Spinner } from "@sukoon/ui/spinner";
import { Switch } from "@sukoon/ui/switch";
import { cn } from "@sukoon/ui/lib/utils";
import {
  getDefaults,
  getHqSettings,
  getNoiseMode,
  getVideoPrefs,
  prefsFromChange,
  setDefaults,
  setHqSettings,
  setNoiseMode,
  setVideoPrefs,
  videoKey,
  type HqSettings,
  type NoiseMode,
  type Prefs,
} from "./prefs.js";
import { mediaKeyFromUrl } from "./adapters/index.js";
import "./popup.css";

function Segmented({ enabled, onChange }: { enabled: boolean; onChange: (next: boolean) => void }) {
  const options = [
    { value: false, label: "Off" },
    { value: true, label: "On" },
  ];
  return (
    <div className="grid grid-cols-2 gap-1 rounded-xl bg-muted-100 p-1">
      {options.map((o) => (
        <button
          key={o.label}
          type="button"
          aria-pressed={enabled === o.value}
          onClick={() => onChange(o.value)}
          className={cn(
            "rounded-lg px-3 py-1.5 text-sm font-medium transition-colors",
            enabled === o.value
              ? o.value
                ? "bg-primary text-primary-foreground shadow-sm"
                : "bg-card text-foreground shadow-sm"
              : "text-muted-500 hover:text-foreground",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/**
 * HQ Live toggle. Pairing with the desktop app is automatic — HQ activates whenever the Sukoon
 * desktop app is running (even in the background) and falls back to the instant engine otherwise.
 */
function HqSection({ hq, onChange }: { hq: HqSettings; onChange: (next: HqSettings) => void }) {
  return (
    <div className="rounded-xl bg-muted-100 p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-medium">HQ Live</span>
        <Switch
          size="sm"
          checked={hq.enabled}
          onCheckedChange={(enabled) => onChange({ enabled })}
          aria-label="HQ Live"
        />
      </div>
      <p className="mt-1 text-xs text-muted-500">
        Studio-grade separation while the Sukoon desktop app is running. Adds a few seconds of
        delay; ads always play instantly.
      </p>
    </div>
  );
}

const NOISE_COLORS: { value: NoiseMode; label: string }[] = [
  { value: "smart", label: "Smart" },
  { value: "white", label: "White" },
  { value: "pink", label: "Pink" },
  { value: "brown", label: "Brown" },
];

/**
 * Comfort-noise controls: a low, auto-leveled noise bed over the cleaned audio so removed music
 * doesn't leave dead-sounding gaps. "Smart" matches the video's own noise floor; the classic
 * colors force a shape. Level is always automatic.
 */
function NoiseSection({ mode, onChange }: { mode: NoiseMode; onChange: (m: NoiseMode) => void }) {
  const enabled = mode !== "off";
  return (
    <div className="rounded-xl bg-muted-100 p-3">
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-medium">Add noise</span>
        <Switch
          size="sm"
          checked={enabled}
          onCheckedChange={(on) => onChange(on ? "smart" : "off")}
          aria-label="Add noise"
        />
      </div>
      <p className="mt-1 text-xs text-muted-500">
        A gentle, auto-leveled noise bed that keeps cleaned audio sounding natural.
      </p>
      {enabled && (
        <div className="mt-2 grid grid-cols-4 gap-1 rounded-lg bg-card p-1">
          {NOISE_COLORS.map((c) => (
            <button
              key={c.value}
              type="button"
              aria-pressed={mode === c.value}
              onClick={() => onChange(c.value)}
              className={cn(
                "rounded-md px-1 py-1 text-xs font-medium transition-colors",
                mode === c.value
                  ? "bg-primary text-primary-foreground shadow-sm"
                  : "text-muted-500 hover:text-foreground",
              )}
            >
              {c.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function Popup() {
  const [videoId, setVideoId] = useState<string | null | undefined>(undefined); // undefined = loading
  const [prefs, setPrefs] = useState<Prefs | null>(null);
  const [hq, setHq] = useState<HqSettings | null>(null);
  const [noise, setNoise] = useState<NoiseMode | null>(null);

  useEffect(() => {
    let active = true;
    void (async () => {
      const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
      const id = mediaKeyFromUrl(tab?.url);
      const [p, h, nm] = await Promise.all([
        id ? getVideoPrefs(id) : getDefaults(),
        getHqSettings(),
        getNoiseMode(),
      ]);
      if (!active) return;
      setVideoId(id);
      setPrefs(p);
      setHq(h);
      setNoise(nm);
    })();
    const listener = (changes: Record<string, chrome.storage.StorageChange>, area: string) => {
      if (area === "local" && videoId && changes[videoKey(videoId)]) {
        setPrefs(prefsFromChange(changes[videoKey(videoId)]!.newValue));
      }
    };
    chrome.storage.onChanged.addListener(listener);
    return () => {
      active = false;
      chrome.storage.onChanged.removeListener(listener);
    };
  }, [videoId]);

  const update = (enabled: boolean) => {
    setPrefs({ enabled });
    if (videoId) void setVideoPrefs(videoId, { enabled });
    else void setDefaults({ enabled });
  };

  const updateHq = (next: HqSettings) => {
    setHq(next);
    void setHqSettings(next);
  };

  const updateNoise = (next: NoiseMode) => {
    setNoise(next);
    void setNoiseMode(next);
  };

  const enabled = prefs?.enabled ?? null;

  return (
    <main className="flex w-72 flex-col gap-5 p-5 text-foreground">
      <header className="flex flex-col items-center text-center">
        <Logo className="size-14 text-primary" />
        <h1 className="mt-2 text-lg font-semibold tracking-tight">Sukoon · سكون</h1>
        <p className="mt-1 text-xs text-muted-500">Reduces instruments, protects recitation.</p>
      </header>

      <div>
        {prefs === null ? (
          <div className="flex h-10 items-center justify-center rounded-xl bg-muted-100">
            <Spinner size="sm" variant="muted" />
          </div>
        ) : (
          <Segmented enabled={enabled ?? false} onChange={update} />
        )}
        <p className="mt-2 text-center text-xs text-muted-500">
          {videoId === null
            ? "Not on a video — this sets the default for new videos."
            : enabled === false
              ? "Playing the original audio for this video."
              : "Real-time, on-device. Instrumental music is reduced; the human voice is kept."}
        </p>
      </div>

      {hq !== null && <HqSection hq={hq} onChange={updateHq} />}
      {noise !== null && <NoiseSection mode={noise} onChange={updateNoise} />}

      <p className="border-t border-border pt-3 text-center text-[11px] leading-snug text-muted-500">
        Sukoon processes all audio, including ads. It never blocks, skips, or mutes ads.
      </p>
    </main>
  );
}

const container = document.getElementById("root");
if (container) {
  createRoot(container).render(
    <StrictMode>
      <Popup />
    </StrictMode>,
  );
}
