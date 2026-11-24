//! AnyFile — Tauri commands and job orchestration.
//!
//! Jobs run concurrently on the async runtime (bounded by a semaphore), CPU
//! work happens on blocking threads, and every state change is reported to
//! the UI through the `conversion-progress` event.

mod engines;
mod types;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{RwLock, Semaphore};

use engines::{ext_of, normalize_ext, ConvertError, EngineRegistry};
use types::*;

const PROGRESS_EVENT: &str = "conversion-progress";

struct AppState {
    /// `None` until the first (lazy) engine detection completes.
    engines: RwLock<Option<EngineRegistry>>,
    permits: Arc<Semaphore>,
}

impl AppState {
    fn new() -> Self {
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        Self {
            engines: RwLock::new(None),
            permits: Arc::new(Semaphore::new((cores / 2).clamp(2, 4))),
        }
    }

    async fn engines(&self) -> EngineRegistry {
        if let Some(reg) = self.engines.read().await.as_ref() {
            return reg.clone();
        }
        let mut guard = self.engines.write().await;
        if guard.is_none() {
            *guard = Some(detect().await);
        }
        guard.clone().unwrap_or_default()
    }

    async fn redetect(&self) -> EngineRegistry {
        let reg = detect().await;
        *self.engines.write().await = Some(reg.clone());
        reg
    }
}

async fn detect() -> EngineRegistry {
    tauri::async_runtime::spawn_blocking(EngineRegistry::detect)
        .await
        .unwrap_or_default()
}

fn emit(app: &AppHandle, id: &str, status: JobStatus, progress: Option<f32>) {
    let _ = app.emit(
        PROGRESS_EVENT,
        ProgressPayload { id: id.to_string(), status, progress, result: None, error: None },
    );
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn get_engine_status(state: State<'_, AppState>, refresh: Option<bool>) -> Result<Vec<EngineStatus>, String> {
    let reg = if refresh.unwrap_or(false) { state.redetect().await } else { state.engines().await };
    Ok(reg.statuses())
}

/// Inspects dropped/picked paths and returns size, category and valid targets.
/// Directories and unreadable paths are skipped.
#[tauri::command]
async fn get_supported_targets(state: State<'_, AppState>, paths: Vec<String>) -> Result<Vec<FileInfo>, String> {
    let reg = state.engines().await;
    Ok(paths
        .into_iter()
        .filter_map(|p| {
            let path = PathBuf::from(&p);
            let meta = std::fs::metadata(&path).ok().filter(|m| m.is_file())?;
            let ext = ext_of(&path);
            Some(FileInfo {
                name: path.file_name()?.to_string_lossy().into_owned(),
                category: engines::category_of(&ext),
                targets: engines::targets_for(&ext, &reg),
                size: meta.len(),
                ext,
                path: p,
            })
        })
        .collect())
}

#[tauri::command]
async fn convert_file(app: AppHandle, state: State<'_, AppState>, request: ConversionRequest) -> Result<ConversionResult, String> {
    let reg = state.engines().await;
    run_job(app, reg, state.permits.clone(), request).await
}

/// Starts every job at once (the semaphore bounds real concurrency) and
/// resolves when all have finished. Per-job updates arrive as events.
#[tauri::command]
async fn convert_batch(app: AppHandle, state: State<'_, AppState>, requests: Vec<ConversionRequest>) -> Result<Vec<JobOutcome>, String> {
    let reg = state.engines().await;
    let handles: Vec<_> = requests
        .into_iter()
        .map(|req| {
            let id = req.id.clone();
            let task = tauri::async_runtime::spawn(run_job(app.clone(), reg.clone(), state.permits.clone(), req));
            (id, task)
        })
        .collect();

    let mut outcomes = Vec::with_capacity(handles.len());
    for (id, task) in handles {
        let outcome = match task.await {
            Ok(Ok(result)) => JobOutcome { id, result: Some(result), error: None },
            Ok(Err(error)) => JobOutcome { id, result: None, error: Some(error) },
            Err(e) => JobOutcome { id, result: None, error: Some(e.to_string()) },
        };
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

/// Merges several images, in the given order, into one multi-page PDF.
#[tauri::command]
async fn combine_to_pdf(app: AppHandle, state: State<'_, AppState>, request: CombineRequest) -> Result<ConversionResult, String> {
    let permits = state.permits.clone();
    let id = request.id.clone();
    emit(&app, &id, JobStatus::Queued, Some(0.0));
    let _permit = permits.acquire_owned().await.map_err(|e| e.to_string())?;
    emit(&app, &id, JobStatus::Running, Some(0.0));

    let app2 = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || combine_blocking(&app2, request))
        .await
        .map_err(|e| ConvertError::failed(e.to_string()))
        .and_then(|r| r);
    finish(&app, &id, result)
}

#[tauri::command]
fn reveal_in_folder(path: String) -> Result<(), String> {
    tauri_plugin_opener::reveal_item_in_dir(PathBuf::from(path)).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Job execution
// ---------------------------------------------------------------------------

fn finish(app: &AppHandle, id: &str, result: engines::Result<ConversionResult>) -> Result<ConversionResult, String> {
    let payload = match &result {
        Ok(r) => ProgressPayload { id: id.into(), status: JobStatus::Done, progress: Some(1.0), result: Some(r.clone()), error: None },
        Err(e) => ProgressPayload { id: id.into(), status: JobStatus::Error, progress: None, result: None, error: Some(e.to_string()) },
    };
    let _ = app.emit(PROGRESS_EVENT, payload);
    result.map_err(|e| e.to_string())
}

async fn run_job(app: AppHandle, reg: EngineRegistry, permits: Arc<Semaphore>, req: ConversionRequest) -> Result<ConversionResult, String> {
    let id = req.id.clone();
    emit(&app, &id, JobStatus::Queued, Some(0.0));
    let result = async {
        let _permit = permits
            .acquire_owned()
            .await
            .map_err(|_| ConvertError::failed("Converter is shutting down"))?;
        emit(&app, &id, JobStatus::Running, None);
        execute(&app, &reg, req).await
    }
    .await;
    finish(&app, &id, result)
}

fn output_dir_for(input: &Path, requested: Option<&str>) -> engines::Result<PathBuf> {
    match requested.filter(|d| !d.is_empty()) {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => input
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| ConvertError::failed("Cannot determine output folder")),
    }
}

async fn execute(app: &AppHandle, reg: &EngineRegistry, req: ConversionRequest) -> engines::Result<ConversionResult> {
    let started = Instant::now();
    let input = PathBuf::from(&req.path);
    let meta = std::fs::metadata(&input).map_err(|e| ConvertError::failed(format!("Cannot read file: {e}")))?;
    if !meta.is_file() {
        return Err(ConvertError::failed("Not a file"));
    }

    let from = ext_of(&input);
    let to = normalize_ext(&req.target);
    let engine = engines::resolve(&from, &to, reg)?;

    let out_dir = output_dir_for(&input, req.output_dir.as_deref())?;
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "output".into());
    let output = engines::reserve_output_path(&out_dir, &stem, &to)?;

    let outcome = match engine {
        EngineId::Builtin => {
            let (i, o, f, t, opts) = (input.clone(), output.clone(), from.clone(), to.clone(), req.options.clone());
            tauri::async_runtime::spawn_blocking(move || engines::convert_builtin(&i, &o, &f, &t, &opts))
                .await
                .map_err(|e| ConvertError::failed(e.to_string()))
                .and_then(|r| r)
        }
        EngineId::Libreoffice => {
            let soffice = reg.soffice.as_deref().ok_or(ConvertError::EngineMissing("LibreOffice"))?;
            engines::office::libreoffice(soffice, &input, &output, &from, &to).await
        }
        EngineId::Pandoc => {
            let pandoc = reg.pandoc.as_deref().ok_or(ConvertError::EngineMissing("Pandoc"))?;
            engines::office::pandoc(pandoc, &input, &output, &from, &to).await
        }
        EngineId::Ffmpeg => {
            let ffmpeg = reg.ffmpeg.as_deref().ok_or(ConvertError::EngineMissing("FFmpeg"))?;
            let (app, id) = (app.clone(), req.id.clone());
            let last = AtomicI32::new(-1);
            engines::media::convert(ffmpeg, &input, &output, &from, &to, &req.options, move |p| {
                // Throttle to whole-percent changes.
                let pct = (p * 100.0) as i32;
                if last.swap(pct, Ordering::Relaxed) != pct {
                    emit(&app, &id, JobStatus::Running, Some(p));
                }
            })
            .await
        }
    };

    let output_size = outcome.and_then(|_| Ok(std::fs::metadata(&output)?.len())).and_then(|size| {
        if size == 0 {
            Err(ConvertError::failed("The converter produced an empty file"))
        } else {
            Ok(size)
        }
    });
    let output_size = match output_size {
        Ok(size) => size,
        Err(e) => {
            // Release the reserved placeholder so failed jobs leave no trace.
            let _ = std::fs::remove_file(&output);
            return Err(e);
        }
    };

    Ok(ConversionResult {
        id: req.id,
        output_path: output.to_string_lossy().into_owned(),
        input_size: meta.len(),
        output_size,
        engine,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

fn combine_blocking(app: &AppHandle, req: CombineRequest) -> engines::Result<ConversionResult> {
    let started = Instant::now();
    let first = PathBuf::from(req.paths.first().ok_or_else(|| ConvertError::failed("No images selected"))?);
    let total = req.paths.len();
    let mut images = Vec::with_capacity(total);
    let mut input_size = 0;
    for (i, p) in req.paths.iter().enumerate() {
        let path = PathBuf::from(p);
        input_size += std::fs::metadata(&path)?.len();
        images.push(engines::pdf::PdfImage::open(&path)?);
        emit(app, &req.id, JobStatus::Running, Some((i + 1) as f32 / (total + 1) as f32));
    }

    let out_dir = output_dir_for(&first, req.output_dir.as_deref())?;
    let stem = first.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "images".into());
    let output = engines::reserve_output_path(&out_dir, &format!("{stem}_combined"), "pdf")?;
    if let Err(e) = engines::pdf::images_to_pdf(images, &output) {
        let _ = std::fs::remove_file(&output);
        return Err(e);
    }
    Ok(ConversionResult {
        id: req.id,
        output_size: std::fs::metadata(&output)?.len(),
        output_path: output.to_string_lossy().into_owned(),
        input_size,
        engine: EngineId::Builtin,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .setup(|app| {
            // Warm the engine cache in the background so the first drop is instant.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                handle.state::<AppState>().engines().await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_engine_status,
            get_supported_targets,
            convert_file,
            convert_batch,
            combine_to_pdf,
            reveal_in_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running AnyFile");
}
