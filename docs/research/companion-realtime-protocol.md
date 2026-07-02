# Companion realtime streaming protocol (v1)

Transport: the existing companion WebSocket (`ws://127.0.0.1:8765/?token=<pairing token>`).
Control messages are JSON text frames; audio is binary frames.
All binary values are little-endian.

Authentication: the persistent pairing token is the credential for the WebSocket.
Clients obtain it automatically: a plain-HTTP `GET http://127.0.0.1:8765/pairing` (same port as the WebSocket) returns `{"port":8765,"token":"<token>","version":"<app version>"}`.
`/pairing` answers only when the request's `Origin` header is absent (local tools) or starts with `chrome-extension://` or `moz-extension://`; any other origin gets an empty 403.
Browsers always attach an `Origin` to a cross-origin fetch, so a web page can never read the token.
The extension's realtime client connects from a content script, whose WebSocket carries the page's origin (e.g. `https://www.youtube.com`), not `chrome-extension://` - so the handshake must accept any `https` page origin or extension origin when the token matches, and reject only on a missing or wrong token.
The token is a random UUID stored in extension storage, so possession of it is proof of pairing; the server stays loopback-only.

## Roles

The extension (client) streams noisy 48 kHz stereo PCM to the desktop app (server).
The server runs the requested separation engine in a streaming session and pushes cleaned 48 kHz stereo PCM back.
The client owns playout timing; the server owns block assembly, resampling, and inference.

## Control messages

- `{"type":"hello"}` → `{"type":"ready","version":...,"engines":[...],"realtime":true,"rtSampleRate":48000,"rtChannels":2}`.
- `{"type":"rt_start","engine":"hq"}` → `{"type":"rt_started","engine":"hq","blockSamples":N,"latencySamples":M}` or `{"type":"error","error":...}`.
  `blockSamples` is the emit granularity in 48 kHz samples; `latencySamples` is the server's estimated end-to-end processing latency (block assembly + inference margin) in 48 kHz samples.
  One realtime session per connection.
- `{"type":"rt_flush","epoch":E}` → `{"type":"rt_flushed","epoch":E}`.
  Resets all accumulators and resampler state; subsequent input carries epoch `E` and `startSample` restarts at 0.
  The server must not emit any further audio from an older epoch after acking the flush.
- `{"type":"rt_stop"}` → `{"type":"rt_stopped"}`. Ends the session; the connection stays usable for `clean`/`hello`.
- Server may push `{"type":"rt_overloaded"}` and end the session if inference falls behind realtime beyond its input cap (~30 s).
  The client treats this as "fall back to the instant engine".

## Binary audio frames

16-byte header, then payload:

| offset | type | field                                                                     |
| ------ | ---- | ------------------------------------------------------------------------- |
| 0      | u8   | kind: 1 = noisy input (client→server), 2 = cleaned output (server→client) |
| 1      | u8   | channels (always 2)                                                       |
| 2      | u16  | epoch                                                                     |
| 4      | u32  | reserved (0)                                                              |
| 8      | u64  | startSample (48 kHz stream position within the epoch)                     |

Payload: interleaved f32 samples; sample count = payload bytes / (4 × channels).

Output frames cover exactly the same 48 kHz stream positions as the input that produced them: cleaned samples `[startSample, startSample + n)` correspond 1:1 to input samples `[startSample, startSample + n)`.
The server compensates internally for resampler and STFT group delay so this alignment holds within a few samples.

## Server session semantics

- Input is accumulated and resampled 48 kHz → engine rate (44.1 kHz for MDX).
- Blocks follow the engine's `ChunkPlan`: `context` samples of carry on each side, emit in whole `align` blocks; the first block is front-padded with `context` zeros (matching the offline `demix` padding).
- Emit block size is one `align` block (lowest latency), not the 32 MB batch plan.
- Cleaned engine-rate audio is resampled back to 48 kHz; the initial resampler delay is dropped so output stream position tracks input stream position.
- Stale epochs are discarded everywhere (input frames with an epoch other than the current one are dropped).
