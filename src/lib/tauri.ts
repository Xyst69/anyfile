// Typed wrappers around Tauri commands and events.
// Types mirror src-tauri/src/types.rs (camelCase on the wire).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";

export type Category = "image" | "document" | "data" | "audio" | "video";
export type EngineId = "builtin" | "libreoffice" | "pandoc" | "ffmpeg";
export type JobStatus = "queued" | "running" | "done" | "error";

export interface EngineStatus {
  id: EngineId;
  name: string;
  available: boolean;
  path: string | null;
  version: string | null;
}

export interface TargetOption {
  ext: string;
  category: Category;
  engine: EngineId;
  available: boolean;
}

export interface FileInfo {
  path: string;
  name: string;
  ext: string;
  size: number;
  category: Category | null;
  targets: TargetOption[];
}

export interface ConversionOptions {
  quality?: number;
  width?: number;
  height?: number;
}

export interface ConversionRequest {
  id: string;
  path: string;
  target: string;
  outputDir: string | null;
  options: ConversionOptions;
}

export interface CombineRequest {
  id: string;
  paths: string[];
  outputDir: string | null;
}

export interface ConversionResult {
  id: string;
  outputPath: string;
  inputSize: number;
  outputSize: number;
  engine: EngineId;
  elapsedMs: number;
}

export interface ProgressPayload {
  id: string;
  status: JobStatus;
  progress: number | null;
  result: ConversionResult | null;
  error: string | null;
}

export interface JobOutcome {
  id: string;
  result: ConversionResult | null;
  error: string | null;
}

export type DragEvent =
  | { type: "enter"; paths: string[] }
  | { type: "over" }
  | { type: "drop"; paths: string[] }
  | { type: "leave" };

export const api = {
  engineStatus: (refresh = false) => invoke<EngineStatus[]>("get_engine_status", { refresh }),

  inspect: (paths: string[]) => invoke<FileInfo[]>("get_supported_targets", { paths }),

  convertFile: (request: ConversionRequest) => invoke<ConversionResult>("convert_file", { request }),

  convertBatch: (requests: ConversionRequest[]) => invoke<JobOutcome[]>("convert_batch", { requests }),

  combineToPdf: (request: CombineRequest) => invoke<ConversionResult>("combine_to_pdf", { request }),

  revealInFolder: (path: string) => invoke<void>("reveal_in_folder", { path }),

  onProgress: (handler: (payload: ProgressPayload) => void): Promise<UnlistenFn> =>
    listen<ProgressPayload>("conversion-progress", (e) => handler(e.payload)),

  onDragDrop: (handler: (event: DragEvent) => void): Promise<UnlistenFn> =>
    getCurrentWebview().onDragDropEvent((e) => handler(e.payload as DragEvent)),

  pickFiles: async (): Promise<string[]> => {
    const picked = await open({ multiple: true, directory: false, title: "Add files" });
    if (!picked) return [];
    return Array.isArray(picked) ? picked : [picked];
  },

  pickFolder: async (): Promise<string | null> => {
    const picked = await open({ directory: true, multiple: false, title: "Choose output folder" });
    return typeof picked === "string" ? picked : null;
  },
};
