# CLI reference

The `sukoon` command. Source: [`packages/cli`](../../packages/cli).

```
sukoon <COMMAND> [OPTIONS]
```

Global: `-v` / `-vv` raise log verbosity. `--version`, `--help`.

## `sukoon clean`

Clean a single media file.

```
sukoon clean <INPUT> [--output <PATH>] [--engine <ENGINE>] [--mode <MODE>] [--no-cache]
```

| Option         | Default               | Notes                       |
| -------------- | --------------------- | --------------------------- |
| `<INPUT>`      | —                     | Video or audio file.        |
| `-o, --output` | `<input>.clean.<ext>` | Output path.                |
| `-e, --engine` | `hq`                  | `hq` / `fast` / `fallback`. |
| `-m, --mode`   | `remove-all`          | See [modes](#modes).        |
| `--no-cache`   | off                   | Disable the on-disk cache.  |

## `sukoon batch`

Clean every media file in a folder.

```
sukoon batch <INPUT_DIR> --out <OUTPUT_DIR> [--engine <ENGINE>] [--mode <MODE>]
```

`--engine` defaults to `hq` (MDX-Net), same as `clean`. Unreadable files are skipped with a logged
error, not fatal.

## `sukoon engines`

List engines, their models, sizes, and **licenses**.

## Engines

| `--engine`     | Model                         | Status                                     | Best for                          |
| -------------- | ----------------------------- | ------------------------------------------ | --------------------------------- |
| `hq` (default) | MDX-Net Kim Vocal 2           | ✅ ~4× CPU / ~12–15× GPU                   | Dense music, nasheeds, files      |
| `fast`         | DeepFilterNet 3               | ✅ ~180× real-time (CPU); `--features dfn` | Live/real-time speech enhancement |
| `fallback`     | MDX-Net UVR 9482 (`mdx-lite`) | ✅ low-RAM (~30 MB); lower quality         | Old / memory-constrained hardware |

> Speed is per-platform and automatic: GPU figures are with the platform accelerator (CoreML/
> DirectML/CUDA), which the core selects itself. The **Fast (DeepFilterNet)** engine is
> **CPU-preferred** (a small recurrent model where GPU is measurably slower) and requires building
> with `--features dfn`. The `fallback` alias also accepts `mdx-lite` / `mdx_q`. See
> [engines](../architecture/engines.md#acceleration) and the full
> [performance & device matrix](./performance.md).

On first use, the selected engine's weights download automatically (with a live progress indicator);
nothing is bundled and subsequent runs are offline.

## Modes

`remove-all` (default) and `keep-vocals` keep the voice and drop the instrumental (identical for the
2-stem engine). `keep-percussion` / `preserve-effects` are placeholders, not yet implemented —
see [halal-aware](../halal-aware/index.md).

## Environment

See [configuration](./config.md) for the full list: `SUKOON_FFMPEG` / `SUKOON_FFPROBE` (FFmpeg
binary paths), `SUKOON_MODELS_DIR` (where weights are downloaded/cached), `SUKOON_CACHE_DIR`
(cleaned-stem cache), and `SUKOON_CPU_ONLY=1` (force CPU inference).
