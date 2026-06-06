"use client";

import { useEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { Button } from "@sukoon/ui/button";
import { Logo } from "@sukoon/ui/logo";
import { ProgressLine } from "@sukoon/ui/progress-line";
import { Spinner } from "@sukoon/ui/spinner";
import { cn } from "@sukoon/ui/lib/utils";

function WaveIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      className={className}
      aria-hidden
    >
      <path d="M2 12h2M7 7v10M12 3v18M17 7v10M22 12h-2" />
    </svg>
  );
}

function CheckIcon({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
    >
      <path d="M20 6 9 17l-5-5" />
    </svg>
  );
}

type EngineId = "fast" | "hq" | "fallback";
type Status = "queued" | "running" | "done" | "error";

type Job = {
  id: string;
  path: string;
  name: string;
  status: Status;
  stage?: string;
  pct?: number | null;
  output?: string;
  error?: string;
};

const ENGINES: { id: EngineId; label: string; blurb: string }[] = [
  { id: "fast", label: "Fast", blurb: "Real-time, on-device. Best for most videos." },
  { id: "hq", label: "HQ", blurb: "Highest-quality separation. Slower; uses the GPU." },
  { id: "fallback", label: "Fallback", blurb: "Smaller model for low-memory machines." },
];

const MEDIA_EXT = ["mp4", "mkv", "mov", "webm", "avi", "mp3", "wav", "m4a", "aac", "flac"];

const STAGE_LABEL: Record<string, string> = {
  extract: "Extracting…",
  separate: "Removing music…",
  encode: "Encoding…",
  remux: "Finishing…",
  done: "Done",
};

type ProgressEvent = { job_id: string; stage: string; chunk: number; total: number };
type DownloadEvent = { id: string; downloaded: number; total: number | null };
type PreviewPaths = { original: string; cleaned: string };

function basename(p: string): string {
  return p.split(/[\\/]/).pop() || p;
}

function deriveOutput(input: string): string {
  const slash = Math.max(input.lastIndexOf("/"), input.lastIndexOf("\\"));
  const dot = input.lastIndexOf(".");
  if (dot <= slash) return `${input}.clean`;
  return `${input.slice(0, dot)}.clean${input.slice(dot)}`;
}

function mb(bytes: number): string {
  return (bytes / 1_000_000).toFixed(1);
}

function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function notifyDone(count: number) {
  try {
    let granted = await isPermissionGranted();
    if (!granted) granted = (await requestPermission()) === "granted";
    if (granted) {
      sendNotification({
        title: "Sukoon",
        body: count === 1 ? "Your file is cleaned." : `${count} files cleaned.`,
      });
    }
  } catch {}
}

export default function Home() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [engine, setEngine] = useState<EngineId>("fast");
  const [dragOver, setDragOver] = useState(false);
  const [running, setRunning] = useState(false);
  const [download, setDownload] = useState<DownloadEvent | null>(null);
  const [preview, setPreview] = useState<PreviewPaths | null>(null);
  const [previewingId, setPreviewingId] = useState<string | null>(null);
  const nextId = useRef(0);

  function addPaths(paths: string[]) {
    const media = paths.filter((p) => MEDIA_EXT.includes((p.split(".").pop() || "").toLowerCase()));
    if (!media.length) return;
    setJobs((prev) => {
      const have = new Set(prev.map((j) => j.path));
      const added = media
        .filter((p) => !have.has(p))
        .map((p) => ({
          id: `job-${nextId.current++}`,
          path: p,
          name: basename(p),
          status: "queued" as Status,
        }));
      return [...prev, ...added];
    });
  }

  useEffect(() => {
    if (!inTauri()) return;
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over" || event.payload.type === "enter") {
          setDragOver(true);
        } else if (event.payload.type === "drop") {
          setDragOver(false);
          addPaths(event.payload.paths);
        } else {
          setDragOver(false);
        }
      })
      .then((un) => (unlisten = un))
      .catch(() => {});
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (!inTauri()) return;
    const unlisteners: Array<() => void> = [];
    listen<ProgressEvent>("clean://progress", (e) => {
      const { job_id, stage, chunk, total } = e.payload;
      const pct = stage === "separate" && total > 0 ? Math.round((chunk * 100) / total) : null;
      setJobs((prev) => prev.map((j) => (j.id === job_id ? { ...j, stage, pct } : j)));
    })
      .then((un) => unlisteners.push(un))
      .catch(() => {});
    listen<DownloadEvent>("clean://download", (e) => {
      const d = e.payload;
      setDownload(d.total !== null && d.downloaded >= d.total ? null : d);
    })
      .then((un) => unlisteners.push(un))
      .catch(() => {});
    return () => unlisteners.forEach((un) => un());
  }, []);

  async function chooseFiles() {
    if (!inTauri()) return;
    try {
      const picked = await open({
        multiple: true,
        directory: false,
        filters: [{ name: "Media", extensions: MEDIA_EXT }],
      });
      if (Array.isArray(picked)) addPaths(picked);
      else if (typeof picked === "string") addPaths([picked]);
    } catch {}
  }

  function patch(id: string, fields: Partial<Job>) {
    setJobs((prev) => prev.map((j) => (j.id === id ? { ...j, ...fields } : j)));
  }

  async function previewJob(job: Job) {
    if (!inTauri() || running || previewingId) return;
    setPreviewingId(job.id);
    setPreview(null);
    try {
      const paths = await invoke<PreviewPaths>("preview", { input: job.path, engine });
      setPreview(paths);
    } catch (e) {
      patch(job.id, { status: "error", error: String(e) });
    } finally {
      setPreviewingId(null);
    }
  }

  async function runQueue() {
    if (running) return;
    if (!inTauri()) {
      setJobs((prev) =>
        prev.map((j) =>
          j.status === "queued"
            ? { ...j, status: "error", error: "Open the Sukoon desktop app." }
            : j,
        ),
      );
      return;
    }
    setRunning(true);
    const pending = jobs.filter((j) => j.status === "queued" || j.status === "error");
    let cleaned = 0;
    for (const job of pending) {
      patch(job.id, { status: "running", stage: undefined, pct: null, error: undefined });
      try {
        const output = await invoke<string>("clean_file", {
          jobId: job.id,
          input: job.path,
          output: deriveOutput(job.path),
          engine,
          mode: "remove-all",
        });
        patch(job.id, { status: "done", output, stage: "done" });
        cleaned++;
      } catch (e) {
        patch(job.id, { status: "error", error: String(e) });
      }
    }
    setRunning(false);
    if (cleaned > 0) notifyDone(cleaned);
  }

  const pendingCount = jobs.filter((j) => j.status === "queued" || j.status === "error").length;
  const doneCount = jobs.filter((j) => j.status === "done").length;
  const errorCount = jobs.filter((j) => j.status === "error").length;

  return (
    <main className="flex h-screen flex-col gap-4 px-6 py-6">
      <header className="flex shrink-0 flex-col items-center text-center">
        <Logo className="size-16 text-primary" />
        <h1 className="mt-2 text-xl font-semibold tracking-tight">Sukoon · سكون</h1>
        <p className="mt-1 text-sm text-muted-500">
          Remove background music, keep speech clear. Ads are never modified.
        </p>
      </header>

      <div className="shrink-0">
        <div className="grid grid-cols-3 gap-1 rounded-xl bg-muted-100 p-1">
          {ENGINES.map((e) => (
            <button
              key={e.id}
              type="button"
              disabled={running}
              onClick={() => setEngine(e.id)}
              className={cn(
                "rounded-lg px-3 py-1.5 text-sm font-medium transition-colors disabled:opacity-50",
                engine === e.id
                  ? "bg-card text-foreground shadow-sm"
                  : "text-muted-500 hover:text-foreground",
              )}
            >
              {e.label}
            </button>
          ))}
        </div>
        <p className="mt-2 text-center text-xs text-muted-500">
          {ENGINES.find((e) => e.id === engine)?.blurb}
        </p>
      </div>

      <button
        type="button"
        onClick={chooseFiles}
        disabled={running}
        className={cn(
          "shrink-0 rounded-xl border-2 border-dashed px-6 py-6 text-center transition-colors disabled:opacity-60",
          dragOver ? "border-primary bg-primary/5" : "border-muted-300 hover:border-muted-400",
        )}
      >
        <WaveIcon
          className={cn("mx-auto mb-2 h-7 w-7", dragOver ? "text-primary" : "text-muted-400")}
        />
        <p className="font-medium">Drag files here, or click to choose</p>
        <p className="mt-1 text-xs text-muted-500">
          Speech is kept. The video is never re-encoded.
        </p>
      </button>

      {preview && <PreviewPlayer paths={preview} onClose={() => setPreview(null)} />}

      <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto">
        {jobs.map((job) => (
          <JobRow
            key={job.id}
            job={job}
            disabled={running}
            previewing={previewingId === job.id}
            onPreview={() => previewJob(job)}
          />
        ))}
      </div>

      <div className="shrink-0 space-y-3">
        {download && (
          <div className="space-y-1.5 text-center text-sm">
            <p className="text-muted-600">
              Downloading {download.id} model…{" "}
              {download.total
                ? `${mb(download.downloaded)} / ${mb(download.total)} MB`
                : `${mb(download.downloaded)} MB`}
            </p>
            <ProgressLine
              value={download.total ? (download.downloaded * 100) / download.total : undefined}
              indeterminate={!download.total}
            />
          </div>
        )}
        {jobs.length > 0 && (
          <p className="text-center text-xs text-muted-500">
            {running
              ? `Cleaning… ${doneCount}/${jobs.length} done`
              : doneCount > 0 || errorCount > 0
                ? `${doneCount} cleaned${errorCount ? ` · ${errorCount} failed` : ""}`
                : `${jobs.length} file${jobs.length > 1 ? "s" : ""} ready`}
          </p>
        )}
        <div className="flex items-center gap-2">
          {jobs.length > 0 && !running && (
            <Button variant="ghost" onClick={() => setJobs([])}>
              Clear
            </Button>
          )}
          <Button
            size="lg"
            className="flex-1"
            loading={running}
            disabled={pendingCount === 0}
            onClick={runQueue}
          >
            {running
              ? "Working…"
              : pendingCount > 0
                ? `Remove music (${pendingCount})`
                : "Remove music"}
          </Button>
        </div>
      </div>
    </main>
  );
}

function JobRow({
  job,
  disabled,
  previewing,
  onPreview,
}: {
  job: Job;
  disabled: boolean;
  previewing: boolean;
  onPreview: () => void;
}) {
  const canPreview = job.status !== "running";
  return (
    <div
      className={cn(
        "flex items-center gap-3 rounded-xl border px-3 py-2.5 text-sm transition-colors",
        job.status === "running" ? "border-primary/30 bg-primary/5" : "field",
      )}
    >
      <span className="flex size-6 shrink-0 items-center justify-center">
        {job.status === "queued" && <span className="size-2 rounded-full bg-muted-400" />}
        {job.status === "running" && <Spinner size="sm" variant="primary" />}
        {job.status === "done" && <CheckIcon className="size-4 text-success" />}
        {job.status === "error" && <span className="font-bold text-destructive">!</span>}
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate font-medium">{job.name}</p>
        {job.status === "running" && (
          <div className="mt-1.5 space-y-1">
            <ProgressLine value={job.pct ?? undefined} indeterminate={job.pct == null} />
            <p className="text-xs font-medium text-primary">
              {STAGE_LABEL[job.stage ?? ""] ?? "Working…"}
              {job.pct != null ? ` ${job.pct}%` : ""}
            </p>
          </div>
        )}
        {job.status === "error" && (
          <p className="mt-0.5 truncate text-xs text-destructive">{job.error}</p>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        {canPreview && (
          <button
            type="button"
            disabled={disabled || previewing}
            onClick={onPreview}
            className="rounded-md bg-muted-200 px-2 py-0.5 text-xs text-muted-600 hover:bg-muted-300 hover:text-foreground disabled:opacity-50"
          >
            {previewing ? "Loading…" : "Preview"}
          </button>
        )}
        {job.status === "done" && (
          <>
            <button
              type="button"
              title="Show in folder"
              onClick={() => job.output && revealItemInDir(job.output).catch(() => {})}
              className="rounded-md bg-muted-200 px-2 py-0.5 text-xs text-muted-600 hover:bg-muted-300 hover:text-foreground"
            >
              Reveal
            </button>
            <button
              type="button"
              title="Open file"
              onClick={() => job.output && openPath(job.output).catch(() => {})}
              className="rounded-md bg-muted-200 px-2 py-0.5 text-xs text-muted-600 hover:bg-muted-300 hover:text-foreground"
            >
              Open
            </button>
          </>
        )}
      </div>
    </div>
  );
}

function PreviewPlayer({ paths, onClose }: { paths: PreviewPaths; onClose: () => void }) {
  const [side, setSide] = useState<"original" | "cleaned">("cleaned");
  const audioRef = useRef<HTMLAudioElement>(null);

  function switchTo(next: "original" | "cleaned") {
    if (next === side) return;
    const a = audioRef.current;
    if (a) {
      const time = a.currentTime;
      const wasPlaying = !a.paused;
      a.src = convertFileSrc(next === "cleaned" ? paths.cleaned : paths.original);
      const onLoaded = () => {
        a.currentTime = time;
        if (wasPlaying) a.play().catch(() => {});
        a.removeEventListener("loadedmetadata", onLoaded);
      };
      a.addEventListener("loadedmetadata", onLoaded);
      a.load();
    }
    setSide(next);
  }

  return (
    <div className="field shrink-0 space-y-2 rounded-xl border p-3">
      <div className="flex items-center justify-between">
        <div className="grid grid-cols-2 gap-0.5 rounded-lg bg-muted-100 p-0.5 text-sm">
          {(["original", "cleaned"] as const).map((s) => (
            <button
              key={s}
              type="button"
              onClick={() => switchTo(s)}
              className={cn(
                "rounded-md px-3 py-1 capitalize transition-colors",
                side === s
                  ? "bg-card text-foreground shadow-sm"
                  : "text-muted-500 hover:text-foreground",
              )}
            >
              {s}
            </button>
          ))}
        </div>
        <button
          type="button"
          onClick={onClose}
          className="text-sm text-muted-500 hover:text-foreground"
        >
          Close
        </button>
      </div>
      <audio ref={audioRef} controls className="w-full" src={convertFileSrc(paths.cleaned)}>
        <track kind="captions" />
      </audio>
      <p className="text-center text-xs text-muted-500">
        First 20 seconds · keeps position when you switch
      </p>
    </div>
  );
}
