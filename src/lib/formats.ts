// Format metadata for the UI.
//
// The Rust backend (src-tauri/src/engines/mod.rs → ROUTES) is the single
// source of truth for which conversions exist; `get_supported_targets`
// returns them per file. This module adds the presentation layer on top:
// category labels, default choices, engine install hints and formatting.

import type { Category, EngineId, FileInfo, TargetOption } from "./tauri";

export const CATEGORY_ORDER: Category[] = ["document", "image", "data", "audio", "video"];

export const CATEGORY_LABEL: Record<Category, string> = {
  document: "Document",
  image: "Image",
  data: "Data",
  audio: "Audio",
  video: "Video",
};

/** Preferred default target per source category, in order. */
const DEFAULT_PREFERENCE: Record<Category, string[]> = {
  image: ["png", "jpg", "webp"],
  document: ["pdf", "docx", "html", "md", "txt"],
  data: ["json", "yaml", "csv"],
  audio: ["mp3", "wav", "m4a"],
  video: ["mp4", "webm", "gif"],
};

/** Lossy targets that honour the quality slider. */
export const LOSSY_IMAGE_TARGETS = new Set(["jpg", "webp", "avif"]);

export type ExternalEngine = Exclude<EngineId, "builtin">;
export type Platform = "windows" | "macos" | "linux";

export const ENGINE_INFO: Record<
  EngineId,
  { name: string; handles: string; install?: Record<Platform, string>; url?: string }
> = {
  builtin: {
    name: "Built-in",
    handles: "Images, SVG, JSON · YAML · TOML · XML · CSV, Markdown · HTML, TXT → PDF",
  },
  libreoffice: {
    name: "LibreOffice",
    handles: "DOCX · XLSX · PPTX · ODF → PDF, PDF → DOCX",
    install: {
      windows: "winget install TheDocumentFoundation.LibreOffice",
      macos: "brew install --cask libreoffice",
      linux: "sudo apt install libreoffice",
    },
    url: "https://www.libreoffice.org/download/",
  },
  pandoc: {
    name: "Pandoc",
    handles: "Markdown · DOCX · EPUB · LaTeX · RST · RTF",
    install: {
      windows: "winget install JohnMacFarlane.Pandoc",
      macos: "brew install pandoc",
      linux: "sudo apt install pandoc",
    },
    url: "https://pandoc.org/installing.html",
  },
  ffmpeg: {
    name: "FFmpeg",
    handles: "MP4 · MKV · MOV · WebM · GIF, MP3 · WAV · FLAC · OGG",
    install: {
      windows: "winget install Gyan.FFmpeg",
      macos: "brew install ffmpeg",
      linux: "sudo apt install ffmpeg",
    },
    url: "https://ffmpeg.org/download.html",
  },
};

export function currentPlatform(): Platform {
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes("windows")) return "windows";
  if (ua.includes("mac")) return "macos";
  return "linux";
}

export function installCommand(engine: EngineId): string | null {
  return ENGINE_INFO[engine].install?.[currentPlatform()] ?? null;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  const digits = value >= 100 ? 0 : value >= 10 ? 1 : 1;
  return `${value.toFixed(digits).replace(/\.0$/, "")} ${units[unit]}`;
}

/** Groups targets by category; the source file's own category comes first. */
export function groupTargets(
  targets: TargetOption[],
  primary?: Category | null,
): { category: Category; targets: TargetOption[] }[] {
  const order = primary ? [primary, ...CATEGORY_ORDER.filter((c) => c !== primary)] : CATEGORY_ORDER;
  return order.map((category) => ({
    category,
    targets: targets.filter((t) => t.category === category),
  })).filter((g) => g.targets.length > 0);
}

export function defaultTarget(file: FileInfo, preferred?: string | null): string | null {
  const { targets } = file;
  if (targets.length === 0) return null;
  const has = (ext: string) => targets.some((t) => t.ext === ext);
  const ready = (ext: string) => targets.some((t) => t.ext === ext && t.available);

  if (preferred && has(preferred)) return preferred;
  const prefs = file.category ? DEFAULT_PREFERENCE[file.category] : [];
  return prefs.find(ready) ?? targets.find((t) => t.available)?.ext ?? prefs.find(has) ?? targets[0].ext;
}

/** Targets that every given file supports, in the first file's order. */
export function commonTargets(files: { targets: TargetOption[] }[]): TargetOption[] {
  if (files.length === 0) return [];
  const [first, ...rest] = files;
  return first.targets
    .filter((t) => rest.every((f) => f.targets.some((o) => o.ext === t.ext)))
    .map((t) => ({
      ...t,
      available: files.every((f) => f.targets.find((o) => o.ext === t.ext)?.available),
    }));
}
