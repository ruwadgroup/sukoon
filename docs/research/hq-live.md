# HQ Live: desktop-grade separation on a live page

Status: implemented (extension + desktop companion), pending real-browser validation.
Related: [extension-trials.md](./extension-trials.md) (why in-browser separators failed live), [companion-realtime-protocol.md](./companion-realtime-protocol.md) (the wire protocol).

## The idea

The extension's instant engine (DeepFilterNet) is causal: it hears 10 ms at a time and can never use future context, which is why it is an enhancer and not a separator.
True separators (MDX) need seconds of lookahead, and a live `<video>` has no future audio to give them - unless we make some.

HQ Live buffers `D` seconds of the live stream (audio in the AudioWorklet, video through a WebCodecs delay line) and plays both back `D` seconds late, in lip sync.
Inside that buffer the audio has real lookahead, so the desktop companion's resident MDX engine can process it in its native ~5.8 s blocks.
The user pays `D` seconds of "initial lag" when playback starts (and after seeks); after that everything is realtime, just uniformly shifted.

The earlier trials (B and C) failed because they synced a slow separator against an **undelayed** live element.
HQ Live removes exactly that failure mode: the delay is explicit and owned, and the media element stays the single audio source (same `createMediaElementSource` tap as the instant engine).

## Architecture

```
media element ──► dfn-processor worklet (mode "hq") ──► loudness ──► speakers
                    │            ▲                          (audio, D late)
                    │ hq-input   │ hq-audio
                    ▼            │
              content script ◄──►│ WebSocket (localhost:8765, token-paired)
                    │            │
                    ▼            │
              desktop app: RealtimeSeparator (resample 48k↔44.1k, MDX blocks)

media element ──► rVFC + VideoFrame ──► VideoEncoder ──► chunk ring ──► VideoDecoder ──► canvas
                                                                        (picture, D late)
```

- **Worklet delay line** (`apps/extension/worklet/processor.js`): two rings, `D` samples deep.
  The **bed ring** holds DFN-enhanced audio (written live, hop by hop); the **hq ring** holds the companion's cleaned audio, landed at the same stream positions.
  Playout reads `D` behind the write head and blends hq-over-bed with a ~10 ms smoothed gain, so a late or missing HQ block degrades to instant-engine quality instead of clicking.
- **Companion streaming** (`apps/extension/src/companion-client.ts`, `apps/desktop/src-tauri/src/companion.rs`, `packages/core/src/stream.rs`): interleaved f32 PCM over the existing companion WebSocket, epoch-stamped so seeks discard stale audio.
  The server assembles engine-rate blocks with STFT context carry and compensates resampler delay, so cleaned samples come back 1:1 aligned with input positions.
- **Video delay** (`apps/extension/src/video-delay.ts`): frames are captured with `requestVideoFrameCallback` + `new VideoFrame(video)`, hardware re-encoded (raw frames for ~10 s would exhaust GPU frame pools), ring-buffered, decoded, and drawn to a canvas overlaying the hidden element.
  Chunks are version-tagged so mid-stream resolution changes (YouTube quality switches) reconfigure the decoder exactly at the boundary.
- **Sync** (`apps/extension/src/hq-live.ts`): the audio is the clock.
  The worklet reports "now playing audio captured at context time C"; the content script converts C to a media time via a rolling (contextTime, video.currentTime) correlation log and slaves the video renderer to it.
  Keying on media time makes pauses freeze naturally and rules out cumulative drift.

## Lifecycle rules

- **Pause** freezes both delay heads (`hold`); nothing drains, nothing drifts.
  A pause at the very end of the video does _not_ hold, so the buffered tail plays out.
- **Seek / stall / src change** flushes everything under a new epoch; the user sees the frozen frame plus a spinner while the line refills (~`D` seconds), the same cost as the initial lag.
- **Companion drops mid-play**: the delay line keeps playing the DFN bed (instant-engine quality, still in sync) and reconnects every 10 s; on reconnect the stream resumes position-offset, no rebuffer.
- **Video pipeline failure** (WebCodecs unavailable, capture SecurityError on non-MSE cross-origin media): HQ Live tears down entirely and the graph returns to the live instant engine - never a black player.
- **Playback rate** (0.25x-2x) just works: audio samples arrive at 48 kHz regardless of rate, the separator processes whatever it is fed, and the video renderer advances its media-time target at the element's rate.
- **Ads never use HQ**: when the site adapter reports an ad break (YouTube: the player's `ad-showing` class), HQ Live suspends - the ad plays live on the instant engine with the real player visible (skip buttons line up) - and re-engages when content resumes (one refill).

## Pairing

Pairing is automatic: the companion serves `GET /pairing` (loopback only), which returns the WebSocket token to requests carrying an extension origin (web pages are refused by their `Origin` header).
The extension's background worker fetches it on every engage attempt, so the user only toggles "HQ Live" once; the desktop app just needs to be running (it keeps serving from the tray after its window is closed).
The token persists across desktop app restarts.

## Known quirks (accepted for v1)

- The page's own UI runs live while the picture is delayed: the progress bar and captions are `D` seconds ahead of what the user sees.
- Hidden tabs capture video at ~1 fps (rVFC stops firing), so the first `D` seconds after refocus can be choppy; audio is unaffected.
- Every seek costs a `D`-second refill.
  `D` is sized from the server's block latency (MDX: one ~5.8 s block + margin, so ~7-10 s).

## Manual E2E test plan

Prereqs: desktop app running (HQ model downloaded), extension built (`pnpm --filter @sukoon/extension build:all`) and loaded unpacked, HQ Live toggled on in the popup (no pairing step - it is automatic).

1. Play a YouTube video with heavy background music and speech.
   Expect: status "buffering" with a frozen frame + spinner for ~8 s, then delayed playback in lip sync with clearly better music removal than instant mode (compare by toggling HQ Live off).
2. Pause mid-video, wait 30 s, resume.
   Expect: instant freeze/resume, no drift, no audio gap or replayed content.
3. Seek forward and backward.
   Expect: spinner + refill each time, then clean delayed playback; no stale audio from before the seek.
4. Let the video play to the end.
   Expect: the final `D` seconds still play out after the element reaches its end.
5. Quit the desktop app mid-playback, wait 15 s, relaunch it.
   Expect: seamless degradation to instant-engine quality (no gap, no jump), then HQ quality resumes within ~10-20 s of relaunch.
6. Change YouTube quality (e.g. 1080p → 480p) mid-playback.
   Expect: the delayed picture continues across the switch (decoder reconfigures at the boundary).
7. Background the tab for a minute, refocus.
   Expect: audio uninterrupted; picture may be choppy for up to `D` seconds, then smooth.
8. Set playback speed to 1.5x and 2x mid-playback, then back to 1x.
   Expect: HQ playback continues at each rate, in lip sync, with no flush or fallback.
9. Toggle the extension off during HQ playback.
   Expect: immediate return to the live original (picture jumps forward `D` seconds - expected).
10. Play a video with a pre-roll or mid-roll ad.
    Expect: the ad plays live and instantly (real player visible, skip button usable, never blocked); after the ad, one "Enhancing…" refill and HQ resumes.
11. Quit the desktop app window (not the tray).
    Expect: HQ keeps working - the companion runs in the background; only "Quit Sukoon" from the tray stops it.

Cross-checks: verify no console errors from the content script, stable memory in the tab (video ring is bounded), and `chrome://media-internals` shows no decoder churn.
