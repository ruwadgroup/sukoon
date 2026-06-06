//! `sukoon` — remove background music from media while keeping speech.
//!
//! A thin CLI over [`sukoon_core`]. It does no separation itself; it builds a [`Pipeline`] and
//! drives it over one file or a folder.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use sukoon_core::engine::EngineKind;
use sukoon_core::{Pipeline, PipelineOptions, Progress, SeparationMode};

/// Render model-download progress as a single rewritten status line on stderr.
///
/// Weights download automatically on first use; this makes that visible so a multi-megabyte fetch
/// doesn't look like a hang. Installed globally before the pipeline is built.
fn print_download(id: &str, done: u64, total: Option<u64>) {
    use std::io::Write;
    let mb = |b: u64| b as f64 / 1_000_000.0;
    let line = match total {
        Some(t) if t > 0 => {
            let pct = (done.min(t) * 100 / t).min(100);
            format!(
                "downloading {id} model… {:.1}/{:.1} MB ({pct:>3}%)",
                mb(done),
                mb(t)
            )
        }
        _ => format!("downloading {id} model… {:.1} MB", mb(done)),
    };
    eprint!("\r\x1b[K  ⬇ {line}");
    let _ = std::io::stderr().flush();
}

/// Render a pipeline progress event as a single rewritten status line on stderr.
fn print_progress(p: Progress) {
    use std::io::Write;
    let msg = match p {
        Progress::Extract => "extracting audio…".to_string(),
        Progress::Separate { chunk, total } => {
            let pct = (chunk * 100).checked_div(total).unwrap_or(0);
            format!("separating… {pct:>3}% ({chunk}/{total})")
        }
        Progress::Encode => "encoding…".to_string(),
        Progress::Remux => "remuxing…".to_string(),
        Progress::Done => "done".to_string(),
    };
    // Carriage-return + clear-to-EOL so each event overwrites the previous line.
    eprint!("\r\x1b[K  {msg}");
    let _ = std::io::stderr().flush();
}

/// Remove background music from videos and audio while keeping speech clear.
#[derive(Parser)]
#[command(name = "sukoon", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Increase log verbosity (-v, -vv).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
}

#[derive(Subcommand)]
enum Command {
    /// Clean a single media file.
    Clean {
        /// Input video or audio file.
        input: PathBuf,
        /// Output path. Defaults to `<input>.clean.<ext>`.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Separation engine.
        #[arg(short, long, value_enum, default_value_t = EngineArg::Hq)]
        engine: EngineArg,
        /// Separation mode.
        #[arg(short, long, value_enum, default_value_t = ModeArg::RemoveAll)]
        mode: ModeArg,
        /// Disable the on-disk cache.
        #[arg(long)]
        no_cache: bool,
    },
    /// Clean every media file in a folder.
    Batch {
        /// Input folder.
        input: PathBuf,
        /// Output folder.
        #[arg(long)]
        out: PathBuf,
        #[arg(short, long, value_enum, default_value_t = EngineArg::Hq)]
        engine: EngineArg,
        #[arg(short, long, value_enum, default_value_t = ModeArg::RemoveAll)]
        mode: ModeArg,
    },
    /// List available engines and the models they use.
    Engines,
}

#[derive(Clone, Copy, ValueEnum)]
enum EngineArg {
    /// DeepFilterNet — fast, real-time speech enhancer (real with `--features dfn`).
    Fast,
    /// MDX-Net (Kim Vocal 2) — high-quality vocal/instrumental separation. Default.
    Hq,
    /// MDX-Net UVR 9482 (`mdx-lite`) — smaller, low-RAM fallback.
    Fallback,
}

impl From<EngineArg> for EngineKind {
    fn from(a: EngineArg) -> Self {
        match a {
            EngineArg::Fast => EngineKind::Fast,
            EngineArg::Hq => EngineKind::Hq,
            EngineArg::Fallback => EngineKind::Fallback,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum ModeArg {
    RemoveAll,
    KeepPercussion,
    KeepVocals,
    PreserveEffects,
}

impl From<ModeArg> for SeparationMode {
    fn from(m: ModeArg) -> Self {
        match m {
            ModeArg::RemoveAll => SeparationMode::RemoveAll,
            ModeArg::KeepPercussion => SeparationMode::KeepPercussion,
            ModeArg::KeepVocals => SeparationMode::KeepVocals,
            ModeArg::PreserveEffects => SeparationMode::PreserveEffects,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    // Show a progress line whenever weights are fetched on first use (any subcommand may trigger it).
    sukoon_core::registry::set_download_observer(Box::new(print_download));

    match cli.command {
        Command::Clean {
            input,
            output,
            engine,
            mode,
            no_cache,
        } => {
            let output = output.unwrap_or_else(|| default_output(&input));
            let pipeline = Pipeline::new(PipelineOptions {
                engine: engine.into(),
                mode: mode.into(),
                use_cache: !no_cache,
            })?
            .on_progress(print_progress);
            tracing::info!(engine = pipeline.engine_id(), input = %input.display(), "cleaning");
            pipeline
                .clean_file(&input, &output)
                .with_context(|| format!("failed to clean {}", input.display()))?;
            eprint!("\r\x1b[K"); // clear the progress line
            println!("✓ {} → {}", input.display(), output.display());
        }
        Command::Batch {
            input,
            out,
            engine,
            mode,
        } => {
            std::fs::create_dir_all(&out)?;
            let pipeline = Pipeline::new(PipelineOptions {
                engine: engine.into(),
                mode: mode.into(),
                use_cache: true,
            })?;
            let mut count = 0;
            for entry in std::fs::read_dir(&input)? {
                let path = entry?.path();
                if is_media(&path) {
                    let dest = out.join(default_output(&path).file_name().unwrap());
                    tracing::info!(file = %path.display(), "cleaning");
                    if let Err(e) = pipeline.clean_file(&path, &dest) {
                        tracing::error!(file = %path.display(), error = %e, "skipped");
                        continue;
                    }
                    count += 1;
                }
            }
            println!("✓ cleaned {count} file(s) into {}", out.display());
        }
        Command::Engines => {
            for model in sukoon_core::registry::Model::all() {
                println!(
                    "{:<14} {:<28} license: {:<28} (~{} MB)",
                    model.id,
                    model.name,
                    model.license.label(),
                    model.size_bytes / 1_000_000
                );
            }
        }
    }

    Ok(())
}

fn default_output(input: &Path) -> PathBuf {
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{stem}.clean.{ext}"))
}

fn is_media(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("mp4" | "mkv" | "mov" | "webm" | "avi" | "mp3" | "wav" | "m4a" | "aac" | "flac")
    )
}

fn init_tracing(verbose: u8) {
    let level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(format!("sukoon={level}")));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}
