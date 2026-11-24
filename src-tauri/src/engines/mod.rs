//! Engine discovery, the conversion capability graph, and routing.
//!
//! Tier 1 (`image`, `data`, `pdf`) is compiled into the app.
//! Tier 2 (`office`, `media`) shells out to LibreOffice, Pandoc and FFmpeg
//! when they are found on the machine.

pub mod data;
pub mod image;
pub mod media;
pub mod office;
pub mod pdf;

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use crate::types::{Category, ConversionOptions, EngineId, EngineStatus, TargetOption};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("Unsupported conversion: {from} → {to}")]
    Unsupported { from: String, to: String },
    #[error("Requires {0}")]
    EngineMissing(&'static str),
    #[error("{0}")]
    Failed(String),
}

impl ConvertError {
    pub fn failed(msg: impl Into<String>) -> Self {
        ConvertError::Failed(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, ConvertError>;

// ---------------------------------------------------------------------------
// Formats
// ---------------------------------------------------------------------------

/// Lower-cases an extension and folds common aliases onto one canonical name.
pub fn normalize_ext(ext: &str) -> String {
    let e = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    match e.as_str() {
        "jpeg" | "jpe" | "jfif" => "jpg".into(),
        "tif" => "tiff".into(),
        "yml" => "yaml".into(),
        "markdown" | "mdown" | "mkd" => "md".into(),
        "htm" | "xhtml" => "html".into(),
        "latex" => "tex".into(),
        "text" => "txt".into(),
        _ => e,
    }
}

pub fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(normalize_ext)
        .unwrap_or_default()
}

pub fn category_of(ext: &str) -> Option<Category> {
    Some(match ext {
        "png" | "jpg" | "webp" | "bmp" | "ico" | "tiff" | "gif" | "avif" | "svg" => Category::Image,
        "json" | "yaml" | "toml" | "xml" | "csv" => Category::Data,
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus" => Category::Audio,
        "mp4" | "mkv" | "mov" | "webm" | "avi" => Category::Video,
        "pdf" | "txt" | "md" | "html" | "docx" | "doc" | "rtf" | "odt" | "xlsx" | "xls"
        | "ods" | "pptx" | "ppt" | "odp" | "epub" | "rst" | "tex" => Category::Document,
        _ => return None,
    })
}

/// A directed set of conversions handled by one engine.
struct Route {
    engine: EngineId,
    from: &'static [&'static str],
    to: &'static [&'static str],
}

const RASTER: &[&str] = &["png", "jpg", "webp", "bmp", "ico", "tiff", "gif"];
const DATA: &[&str] = &["json", "yaml", "toml", "xml", "csv"];
const WRITER: &[&str] = &["docx", "doc", "rtf", "odt"];
const CALC: &[&str] = &["xlsx", "xls", "ods"];
const IMPRESS: &[&str] = &["pptx", "ppt", "odp"];
const PANDOC_IN: &[&str] = &["docx", "md", "html", "epub", "rst", "tex", "rtf", "odt"];
const PANDOC_OUT: &[&str] = &["docx", "md", "html", "epub", "rst", "tex", "rtf", "odt", "txt"];
const VIDEO: &[&str] = &["mp4", "mkv", "mov", "webm", "avi", "gif"];
const AUDIO: &[&str] = &["mp3", "wav", "flac", "ogg", "m4a", "aac", "opus"];

/// Ordered by preference: the first route whose engine is available wins.
const ROUTES: &[Route] = &[
    // Tier 1 — built in
    Route { engine: EngineId::Builtin, from: RASTER, to: &["png", "jpg", "webp", "bmp", "ico", "tiff", "gif", "avif", "pdf"] },
    Route { engine: EngineId::Builtin, from: &["svg"], to: &["png", "jpg", "webp", "pdf"] },
    Route { engine: EngineId::Builtin, from: DATA, to: DATA },
    Route { engine: EngineId::Builtin, from: &["md"], to: &["html", "txt"] },
    Route { engine: EngineId::Builtin, from: &["html"], to: &["md", "txt"] },
    Route { engine: EngineId::Builtin, from: &["txt"], to: &["pdf", "html"] },
    // Tier 2 — LibreOffice
    Route { engine: EngineId::Libreoffice, from: WRITER, to: &["pdf", "docx", "doc", "odt", "rtf", "html", "txt"] },
    Route { engine: EngineId::Libreoffice, from: CALC, to: &["pdf", "xlsx", "xls", "ods", "csv", "html"] },
    Route { engine: EngineId::Libreoffice, from: IMPRESS, to: &["pdf", "pptx", "ppt", "odp"] },
    Route { engine: EngineId::Libreoffice, from: &["csv"], to: &["xlsx", "ods"] },
    Route { engine: EngineId::Libreoffice, from: &["pdf"], to: &["docx", "odt"] },
    Route { engine: EngineId::Libreoffice, from: &["html"], to: &["pdf"] },
    // Tier 2 — Pandoc
    Route { engine: EngineId::Pandoc, from: PANDOC_IN, to: PANDOC_OUT },
    // Tier 2 — FFmpeg
    Route { engine: EngineId::Ffmpeg, from: VIDEO, to: &["mp4", "mkv", "mov", "webm", "avi", "gif", "mp3", "wav", "flac", "m4a"] },
    Route { engine: EngineId::Ffmpeg, from: AUDIO, to: AUDIO },
];

/// Every valid target for `from`, each tagged with the engine that would run it.
pub fn targets_for(from: &str, engines: &EngineRegistry) -> Vec<TargetOption> {
    let mut out: Vec<TargetOption> = Vec::new();
    for route in ROUTES.iter().filter(|r| r.from.contains(&from)) {
        let available = engines.is_available(route.engine);
        for &to in route.to.iter().filter(|&&t| t != from) {
            match out.iter_mut().find(|t| t.ext == to) {
                Some(existing) if !existing.available && available => {
                    existing.engine = route.engine;
                    existing.available = true;
                }
                Some(_) => {}
                None => out.push(TargetOption {
                    ext: to.to_string(),
                    category: category_of(to).unwrap_or(Category::Document),
                    engine: route.engine,
                    available,
                }),
            }
        }
    }
    out
}

/// Picks the engine for `from → to`, preferring installed ones.
pub fn resolve(from: &str, to: &str, engines: &EngineRegistry) -> Result<EngineId> {
    let candidates: Vec<EngineId> = ROUTES
        .iter()
        .filter(|r| r.from.contains(&from) && r.to.contains(&to) && from != to)
        .map(|r| r.engine)
        .collect();
    if let Some(&engine) = candidates.iter().find(|&&e| engines.is_available(e)) {
        return Ok(engine);
    }
    match candidates.first() {
        Some(&engine) => Err(ConvertError::EngineMissing(engine.display_name())),
        None => Err(ConvertError::Unsupported { from: from.to_uppercase(), to: to.to_uppercase() }),
    }
}

/// Runs a Tier 1 conversion. Blocking — call from `spawn_blocking`.
pub fn convert_builtin(
    input: &Path,
    output: &Path,
    from: &str,
    to: &str,
    opts: &ConversionOptions,
) -> Result<()> {
    if category_of(from) == Some(Category::Image) {
        let img = image::load(input, from, opts)?;
        return if to == "pdf" {
            pdf::images_to_pdf(vec![pdf::PdfImage::from_source(input, from, img)?], output)
        } else {
            image::save(image::resize(img, opts), output, to, opts.quality)
        };
    }
    if (from, to) == ("txt", "pdf") {
        return pdf::text_to_pdf(&std::fs::read(input)?, output);
    }
    data::convert(input, output, from, to)
}

// ---------------------------------------------------------------------------
// Engine discovery
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct EngineRegistry {
    pub soffice: Option<PathBuf>,
    pub pandoc: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
    pandoc_version: Option<String>,
    ffmpeg_version: Option<String>,
}

impl EngineRegistry {
    /// Probes PATH and well-known install locations. Blocking.
    pub fn detect() -> Self {
        let soffice = find_soffice();
        let pandoc = find_binary("pandoc", &pandoc_candidates());
        let ffmpeg = find_binary("ffmpeg", &ffmpeg_candidates());
        let pandoc_version = pandoc.as_deref().and_then(|p| probe_version(p, "--version"));
        let ffmpeg_version = ffmpeg.as_deref().and_then(|p| probe_version(p, "-version"));
        Self { soffice, pandoc, ffmpeg, pandoc_version, ffmpeg_version }
    }

    pub fn is_available(&self, engine: EngineId) -> bool {
        match engine {
            EngineId::Builtin => true,
            EngineId::Libreoffice => self.soffice.is_some(),
            EngineId::Pandoc => self.pandoc.is_some(),
            EngineId::Ffmpeg => self.ffmpeg.is_some(),
        }
    }

    pub fn statuses(&self) -> Vec<EngineStatus> {
        let status = |id: EngineId, path: &Option<PathBuf>, version: Option<String>| EngineStatus {
            id,
            name: id.display_name().to_string(),
            available: path.is_some(),
            path: path.as_ref().map(|p| p.display().to_string()),
            version,
        };
        vec![
            EngineStatus {
                id: EngineId::Builtin,
                name: EngineId::Builtin.display_name().into(),
                available: true,
                path: None,
                version: Some(env!("CARGO_PKG_VERSION").into()),
            },
            status(EngineId::Libreoffice, &self.soffice, None),
            status(EngineId::Pandoc, &self.pandoc, self.pandoc_version.clone()),
            status(EngineId::Ffmpeg, &self.ffmpeg, self.ffmpeg_version.clone()),
        ]
    }
}

fn find_binary(name: &str, candidates: &[PathBuf]) -> Option<PathBuf> {
    which::which(name)
        .ok()
        .or_else(|| candidates.iter().find(|p| p.is_file()).cloned())
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

fn find_soffice() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Some(base) = env_path(var) {
                candidates.push(base.join("LibreOffice").join("program").join("soffice.exe"));
            }
        }
        candidates.push(PathBuf::from(r"C:\Program Files\LibreOffice\program\soffice.exe"));
    } else if cfg!(target_os = "macos") {
        candidates.push("/Applications/LibreOffice.app/Contents/MacOS/soffice".into());
        if let Some(home) = env_path("HOME") {
            candidates.push(home.join("Applications/LibreOffice.app/Contents/MacOS/soffice"));
        }
        candidates.push("/opt/homebrew/bin/soffice".into());
        candidates.push("/usr/local/bin/soffice".into());
    } else {
        for p in ["/usr/bin/soffice", "/usr/bin/libreoffice", "/usr/local/bin/soffice", "/snap/bin/libreoffice", "/opt/libreoffice/program/soffice"] {
            candidates.push(p.into());
        }
    }

    let found = which::which("soffice")
        .or_else(|_| which::which("libreoffice"))
        .ok()
        .or_else(|| candidates.iter().find(|p| p.is_file()).cloned())?;

    // On Windows, soffice.com is the console launcher: it blocks until the
    // conversion finishes, which soffice.exe does not reliably do.
    if cfg!(windows) {
        let com = found.with_extension("com");
        if com.is_file() {
            return Some(com);
        }
    }
    Some(found)
}

fn pandoc_candidates() -> Vec<PathBuf> {
    let mut c = Vec::new();
    if cfg!(windows) {
        if let Some(local) = env_path("LOCALAPPDATA") {
            c.push(local.join("Pandoc").join("pandoc.exe"));
            c.push(local.join("Microsoft").join("WinGet").join("Links").join("pandoc.exe"));
        }
        if let Some(pf) = env_path("ProgramFiles") {
            c.push(pf.join("Pandoc").join("pandoc.exe"));
        }
        c.extend(windows_package_managers("pandoc.exe"));
    } else {
        c.extend(unix_bin_dirs("pandoc"));
    }
    c
}

fn ffmpeg_candidates() -> Vec<PathBuf> {
    let mut c = Vec::new();
    if cfg!(windows) {
        if let Some(local) = env_path("LOCALAPPDATA") {
            c.push(local.join("Microsoft").join("WinGet").join("Links").join("ffmpeg.exe"));
            // winget's Gyan.FFmpeg package unpacks into a versioned folder.
            let pkgs = local.join("Microsoft").join("WinGet").join("Packages");
            if let Ok(entries) = std::fs::read_dir(&pkgs) {
                for entry in entries.flatten() {
                    if entry.file_name().to_string_lossy().starts_with("Gyan.FFmpeg") {
                        if let Ok(inner) = std::fs::read_dir(entry.path()) {
                            for build in inner.flatten() {
                                c.push(build.path().join("bin").join("ffmpeg.exe"));
                            }
                        }
                    }
                }
            }
        }
        c.push(PathBuf::from(r"C:\ffmpeg\bin\ffmpeg.exe"));
        if let Some(pf) = env_path("ProgramFiles") {
            c.push(pf.join("ffmpeg").join("bin").join("ffmpeg.exe"));
        }
        c.extend(windows_package_managers("ffmpeg.exe"));
    } else {
        c.extend(unix_bin_dirs("ffmpeg"));
    }
    c
}

fn windows_package_managers(exe: &str) -> Vec<PathBuf> {
    let mut c = Vec::new();
    if let Some(pd) = env_path("ProgramData") {
        c.push(pd.join("chocolatey").join("bin").join(exe));
    }
    if let Some(home) = env_path("USERPROFILE") {
        c.push(home.join("scoop").join("shims").join(exe));
    }
    c
}

/// GUI apps on macOS don't inherit the shell PATH, so check Homebrew etc.
fn unix_bin_dirs(name: &str) -> Vec<PathBuf> {
    ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/snap/bin", "/opt/local/bin"]
        .iter()
        .map(|d| Path::new(d).join(name))
        .collect()
}

fn probe_version(bin: &Path, flag: &str) -> Option<String> {
    let mut cmd = std::process::Command::new(bin);
    cmd.arg(flag).stdin(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let first = text.lines().next()?.trim();
    // "ffmpeg version 7.1-full_build-www.gyan.dev Copyright…" / "pandoc 3.6.2"
    first
        .split_whitespace()
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(|v| v.split('-').next().unwrap_or(v).to_string())
}

// ---------------------------------------------------------------------------
// Process helpers (Tier 2)
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A child-process command that never flashes a console window on Windows
/// and is killed if the job future is dropped.
pub fn command(program: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Runs a command to completion with a timeout; returns stderr+stdout text on failure.
pub async fn run_to_completion(mut cmd: tokio::process::Command, timeout: Duration, tool: &str) -> Result<String> {
    let child = cmd
        .spawn()
        .map_err(|e| ConvertError::failed(format!("Could not start {tool}: {e}")))?;
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| ConvertError::failed(format!("{tool} timed out after {}s", timeout.as_secs())))??;
    let log = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    if output.status.success() {
        Ok(log)
    } else {
        Err(ConvertError::failed(format!("{tool} failed: {}", tail(&log, 6))))
    }
}

/// The last `n` non-empty lines of a log, for compact error messages.
pub fn tail(log: &str, n: usize) -> String {
    let lines: Vec<&str> = log.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join(" · ")
}

// ---------------------------------------------------------------------------
// Output paths
// ---------------------------------------------------------------------------

/// Atomically claims `dir/stem.ext`, falling back to `stem_1.ext`, `stem_2.ext`…
/// Creates an empty placeholder so concurrent jobs can never pick the same
/// name, and existing files are never overwritten.
pub fn reserve_output_path(dir: &Path, stem: &str, ext: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    for n in 0..10_000u32 {
        let name = if n == 0 { format!("{stem}.{ext}") } else { format!("{stem}_{n}.{ext}") };
        let candidate = dir.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(_) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(ConvertError::failed("Could not find a free output file name"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> EngineRegistry {
        EngineRegistry::default()
    }

    #[test]
    fn aliases_normalize() {
        assert_eq!(normalize_ext(".JPEG"), "jpg");
        assert_eq!(normalize_ext("yml"), "yaml");
        assert_eq!(normalize_ext("Markdown"), "md");
    }

    #[test]
    fn builtin_routes_resolve_without_external_engines() {
        let e = none();
        assert_eq!(resolve("png", "jpg", &e).unwrap(), EngineId::Builtin);
        assert_eq!(resolve("json", "yaml", &e).unwrap(), EngineId::Builtin);
        assert_eq!(resolve("md", "html", &e).unwrap(), EngineId::Builtin);
        assert_eq!(resolve("txt", "pdf", &e).unwrap(), EngineId::Builtin);
    }

    #[test]
    fn missing_engines_are_reported_by_name() {
        let e = none();
        assert!(matches!(resolve("docx", "pdf", &e), Err(ConvertError::EngineMissing("LibreOffice"))));
        assert!(matches!(resolve("mp4", "mp3", &e), Err(ConvertError::EngineMissing("FFmpeg"))));
        assert!(matches!(resolve("png", "mp3", &e), Err(ConvertError::Unsupported { .. })));
    }

    #[test]
    fn targets_exclude_source_and_flag_availability() {
        let targets = targets_for("docx", &none());
        assert!(targets.iter().all(|t| t.ext != "docx"));
        assert!(targets.iter().all(|t| !t.available));
        let md = targets_for("md", &none());
        assert!(md.iter().any(|t| t.ext == "html" && t.available && t.engine == EngineId::Builtin));
        assert!(md.iter().any(|t| t.ext == "docx" && !t.available && t.engine == EngineId::Pandoc));
    }

    /// Real conversions through the external engines that are installed.
    /// Run with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "needs LibreOffice, Pandoc and FFmpeg installed"]
    async fn external_engines_convert() {
        let reg = EngineRegistry::detect();
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let check = |p: &Path| {
            let len = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            assert!(len > 0, "{} is empty or missing", p.display());
            println!("ok  {} ({len} bytes)", p.file_name().unwrap().to_string_lossy());
        };

        let html = d.join("page.html");
        std::fs::write(&html, "<h1>Hello</h1><p>From <b>AnyFile</b>.</p>").unwrap();

        let pandoc = reg.pandoc.as_deref().expect("pandoc not found");
        for to in ["rtf", "docx", "epub", "odt"] {
            let out = d.join(format!("page.{to}"));
            office::pandoc(pandoc, &html, &out, "html", to).await.unwrap();
            check(&out);
        }

        let soffice = reg.soffice.as_deref().expect("LibreOffice not found");
        for (input, from) in [(d.join("page.docx"), "docx"), (html.clone(), "html")] {
            let out = d.join(format!("from_{from}.pdf"));
            office::libreoffice(soffice, &input, &out, from, "pdf").await.unwrap();
            check(&out);
        }

        let ffmpeg = reg.ffmpeg.as_deref().expect("FFmpeg not found");
        let gen = |args: &[&str]| {
            let status = std::process::Command::new(ffmpeg).args(args).status().unwrap();
            assert!(status.success());
        };
        let wav = d.join("tone.wav");
        let mp4 = d.join("clip.mp4");
        gen(&["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "sine=duration=2", wav.to_str().unwrap()]);
        gen(&["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=15",
              "-f", "lavfi", "-i", "sine=duration=2", "-shortest", "-pix_fmt", "yuv420p", mp4.to_str().unwrap()]);
        let last = std::sync::Mutex::new(0.0f32);
        for (input, from, to) in [(&wav, "wav", "mp3"), (&mp4, "mp4", "webm"), (&mp4, "mp4", "gif"), (&mp4, "mp4", "mp3")] {
            let out = d.join(format!("{from}_to.{to}"));
            media::convert(ffmpeg, input, &out, from, to, &Default::default(), |p| *last.lock().unwrap() = p)
                .await
                .unwrap();
            check(&out);
        }
        assert!(*last.lock().unwrap() > 0.5, "FFmpeg progress was never reported");
    }

    #[test]
    fn reserve_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let a = reserve_output_path(dir.path(), "out", "txt").unwrap();
        let b = reserve_output_path(dir.path(), "out", "txt").unwrap();
        assert_eq!(a.file_name().unwrap(), "out.txt");
        assert_eq!(b.file_name().unwrap(), "out_1.txt");
    }
}
