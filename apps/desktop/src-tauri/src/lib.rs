//! Sukoon desktop — a Tauri shell over `sukoon-core`.
//!
//! Two jobs:
//!   1. A drag-and-drop GUI to clean files (Fast locally; HQ locally if the hardware allows).
//!   2. (Later) the browser extension's local companion: a localhost bridge for real-time desktop
//!      filtering — see ROADMAP.md. Both front-ends drive the same resident [`EngineService`].
//!
//! All separation is delegated to `sukoon-core`; this shell never reimplements it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use sukoon_core::engine::{Engine, EngineKind};
use sukoon_core::{Pipeline, Progress, SeparationMode};
use tauri::{AppHandle, Emitter, Manager, State};

mod companion;

/// Keeps engines **resident**: each model is loaded once and reused across every job (and, later,
/// the companion's real-time stream), so a batch doesn't reload weights per file. Keyed by engine
/// id; the inner [`Engine`] has its own interior locking, so concurrent jobs serialize on inference.
#[derive(Default)]
struct EngineService {
    engines: Mutex<HashMap<&'static str, Arc<dyn Engine>>>,
}

impl EngineService {
    fn engine(&self, kind: EngineKind) -> Result<Arc<dyn Engine>, String> {
        let mut map = self
            .engines
            .lock()
            .map_err(|_| "engine cache poisoned".to_string())?;
        if let Some(engine) = map.get(kind.id()) {
            return Ok(engine.clone());
        }
        let engine: Arc<dyn Engine> = Arc::from(kind.build().map_err(|e| e.to_string())?);
        map.insert(kind.id(), engine.clone());
        Ok(engine)
    }
}

/// Pipeline-stage progress pushed to the UI, tagged with the queue item it belongs to.
#[derive(Clone, serde::Serialize)]
struct ProgressEvent {
    job_id: String,
    stage: &'static str,
    chunk: u32,
    total: u32,
}

/// Model-weight download progress pushed to the UI (first run only).
#[derive(Clone, serde::Serialize)]
struct DownloadEvent {
    id: String,
    downloaded: u64,
    total: Option<u64>,
}

fn parse_mode(mode: &str) -> SeparationMode {
    match mode {
        "keep-vocals" => SeparationMode::KeepVocals,
        "keep-percussion" => SeparationMode::KeepPercussion,
        "preserve-effects" => SeparationMode::PreserveEffects,
        _ => SeparationMode::RemoveAll,
    }
}

/// Clean one file: remove background music, keep speech. Returns the output path on success.
///
/// Runs against the resident engine on a blocking thread so the UI stays responsive; progress is
/// delivered via `clean://progress` tagged with `job_id`.
#[tauri::command]
async fn clean_file(
    app: AppHandle,
    service: State<'_, EngineService>,
    job_id: String,
    input: String,
    output: String,
    engine: String,
    mode: String,
) -> Result<String, String> {
    let kind = EngineKind::from_id(&engine).unwrap_or(EngineKind::Fast);
    let sep_mode = parse_mode(&mode);
    let engine = service.engine(kind)?;
    tauri::async_runtime::spawn_blocking(move || {
        run_clean(app, engine, sep_mode, job_id, input, output)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn run_clean(
    app: AppHandle,
    engine: Arc<dyn Engine>,
    mode: SeparationMode,
    job_id: String,
    input: String,
    output: String,
) -> Result<String, String> {
    let pipeline = Pipeline::from_engine(engine, mode, true).on_progress(move |p| {
        let (stage, chunk, total) = match p {
            Progress::Extract => ("extract", 0, 0),
            Progress::Separate { chunk, total } => ("separate", chunk as u32, total as u32),
            Progress::Encode => ("encode", 0, 0),
            Progress::Remux => ("remux", 0, 0),
            Progress::Done => ("done", 0, 0),
        };
        let _ = app.emit(
            "clean://progress",
            ProgressEvent {
                job_id: job_id.clone(),
                stage,
                chunk,
                total,
            },
        );
    });

    pipeline
        .clean_file(&input, &output)
        .map_err(|e| e.to_string())?;
    Ok(output)
}

/// Paths to a short original/cleaned audio pair for A/B preview.
#[derive(Clone, serde::Serialize)]
struct PreviewPaths {
    original: String,
    cleaned: String,
}

fn ffmpeg_bin() -> String {
    std::env::var("SUKOON_FFMPEG").unwrap_or_else(|_| "ffmpeg".to_string())
}

/// Build an A/B preview: extract a short slice as the "original", clean it on the resident engine,
/// and return both as playable WAVs so the user can verify quality before committing the full job.
#[tauri::command]
async fn preview(
    service: State<'_, EngineService>,
    input: String,
    engine: String,
) -> Result<PreviewPaths, String> {
    let kind = EngineKind::from_id(&engine).unwrap_or(EngineKind::Fast);
    let engine = service.engine(kind)?;
    tauri::async_runtime::spawn_blocking(move || make_preview(engine, input))
        .await
        .map_err(|e| e.to_string())?
}

fn make_preview(engine: Arc<dyn Engine>, input: String) -> Result<PreviewPaths, String> {
    let dir = std::env::temp_dir().join("sukoon-preview");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let original = dir.join("original.wav");
    let cleaned = dir.join("cleaned.wav");

    let status = std::process::Command::new(ffmpeg_bin())
        .args(["-y", "-hide_banner", "-loglevel", "error", "-nostdin"])
        .args([
            "-i",
            &input,
            "-t",
            "20",
            "-vn",
            "-ac",
            "2",
            "-ar",
            "44100",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&original)
        .status()
        .map_err(|e| format!("ffmpeg: {e}"))?;

    if !status.success() {
        return Err("could not extract a preview slice".into());
    }

    // No cache: a preview shouldn't pollute the content-hash cache.
    Pipeline::from_engine(engine, SeparationMode::RemoveAll, false)
        .clean_file(&original, &cleaned)
        .map_err(|e| e.to_string())?;

    Ok(PreviewPaths {
        original: original.to_string_lossy().into_owned(),
        cleaned: cleaned.to_string_lossy().into_owned(),
    })
}

/// Companion bridge status (loopback port + pairing token) for the UI and extension pairing.
#[tauri::command]
fn companion_status(companion: State<'_, companion::Companion>) -> companion::Companion {
    companion.inner().clone()
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(EngineService::default())
        .setup(|app| {
            // Surface model-weight downloads (first run) to the UI. The observer is global in core
            // because downloads happen during engine construction, before any pipeline callback.
            let handle = app.handle().clone();
            sukoon_core::registry::set_download_observer(Box::new(move |id, downloaded, total| {
                let _ = handle.emit(
                    "clean://download",
                    DownloadEvent {
                        id: id.to_string(),
                        downloaded,
                        total,
                    },
                );
            }));

            // Companion bridge for the browser extension + a menu-bar presence.
            app.manage(companion::start(app.handle().clone()));

            let show =
                tauri::menu::MenuItem::with_id(app, "show", "Show Sukoon", true, None::<&str>)?;
            let quit = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&show, &quit])?;
            // Menu-bar (tray) icon: a simple monochrome white glyph, flagged as a macOS template
            // image so the system renders it correctly (white on a dark bar, dark on a light bar) —
            // unlike the full-colour app/dock icon, which looks out of place in the menu bar.
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray@2x.png"))
                .expect("valid tray icon");
            tauri::tray::TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .icon_as_template(true)
                .tooltip("Sukoon")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            clean_file,
            preview,
            companion_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
