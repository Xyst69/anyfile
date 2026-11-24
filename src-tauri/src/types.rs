//! Serde types shared between the Rust backend and the React frontend.
//! Field names are camelCase on the wire to match TypeScript conventions.

use serde::{Deserialize, Serialize};

/// A conversion engine. `Builtin` ships inside the app; the others are
/// external CLI tools discovered on the user's machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineId {
    Builtin,
    Libreoffice,
    Pandoc,
    Ffmpeg,
}

impl EngineId {
    pub fn display_name(self) -> &'static str {
        match self {
            EngineId::Builtin => "Built-in",
            EngineId::Libreoffice => "LibreOffice",
            EngineId::Pandoc => "Pandoc",
            EngineId::Ffmpeg => "FFmpeg",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Image,
    Document,
    Data,
    Audio,
    Video,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub id: EngineId,
    pub name: String,
    pub available: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

/// One valid output format for a given input file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetOption {
    pub ext: String,
    pub category: Category,
    /// The preferred engine for this conversion (an available one if any).
    pub engine: EngineId,
    pub available: bool,
}

/// Metadata about a dropped/picked file plus every format it can become.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub size: u64,
    pub category: Option<Category>,
    pub targets: Vec<TargetOption>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionOptions {
    /// 1–100. Applies to lossy image formats (JPG, WebP, AVIF).
    pub quality: Option<u8>,
    /// Resize: target width in pixels (aspect ratio is preserved).
    pub width: Option<u32>,
    /// Resize: target height in pixels (aspect ratio is preserved).
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionRequest {
    /// Client-generated job id, echoed back in progress events.
    pub id: String,
    pub path: String,
    pub target: String,
    /// `None` means "same folder as the source file".
    pub output_dir: Option<String>,
    #[serde(default)]
    pub options: ConversionOptions,
}

/// Combine several images (in order) into a single multi-page PDF.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CombineRequest {
    pub id: String,
    pub paths: Vec<String>,
    pub output_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionResult {
    pub id: String,
    pub output_path: String,
    pub input_size: u64,
    pub output_size: u64,
    pub engine: EngineId,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Error,
}

/// Payload of the `conversion-progress` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPayload {
    pub id: String,
    pub status: JobStatus,
    /// 0.0–1.0, or `None` when the engine cannot report progress.
    pub progress: Option<f32>,
    pub result: Option<ConversionResult>,
    pub error: Option<String>,
}

/// Final outcome of one job inside `convert_batch`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobOutcome {
    pub id: String,
    pub result: Option<ConversionResult>,
    pub error: Option<String>,
}
