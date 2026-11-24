import { useCallback, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { ArrowRight, Check, CircleAlert, Folder, FolderOpen, LoaderCircle, SlidersHorizontal, X } from "lucide-react";
import { ENGINE_INFO, formatBytes, installCommand } from "../lib/formats";
import { ease, quick, settle } from "../lib/motion";
import type { ConversionResult, FileInfo, TargetOption } from "../lib/tauri";
import { CopyCommand } from "./CopyButton";
import { FormatPicker } from "./FormatPicker";
import { Popover } from "./Popover";

export type ItemStatus = "idle" | "queued" | "running" | "done" | "error";

export interface QueueItem extends FileInfo {
  id: string;
  target: string | null;
  status: ItemStatus;
  progress: number | null;
  result?: ConversionResult;
  error?: string;
  /** Set when this file was merged with others into one PDF. */
  combined?: boolean;
}

export const isActive = (s: ItemStatus) => s === "queued" || s === "running";

function basename(path: string) {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

// ---------------------------------------------------------------------------
// Row pieces
// ---------------------------------------------------------------------------

function ExtBadge({ ext }: { ext: string }) {
  return (
    <span className="inline-flex h-5 w-[46px] shrink-0 items-center justify-center rounded border border-line font-mono text-[10.5px] tracking-wide text-muted uppercase">
      {ext ? ext.slice(0, 5) : "?"}
    </span>
  );
}

/** Circle draws in, then the tick — a small "morph" from progress to done. */
function DoneMark() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-label="Done">
      <motion.circle
        cx="8" cy="8" r="7"
        stroke="currentColor" strokeWidth="1.5"
        initial={{ pathLength: 0, rotate: -90 }}
        animate={{ pathLength: 1 }}
        transition={{ duration: 0.22, ease }}
        style={{ originX: "50%", originY: "50%" }}
      />
      <motion.path
        d="M5 8.3 7 10.2 11 6"
        stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"
        initial={{ pathLength: 0 }}
        animate={{ pathLength: 1 }}
        transition={{ delay: 0.14, duration: 0.2, ease }}
      />
    </svg>
  );
}

function RequiresBadge({ target }: { target: TargetOption }) {
  const cmd = installCommand(target.engine);
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      <span className="inline-flex h-5 shrink-0 items-center rounded border border-line px-1.5 font-sans text-[10.5px] text-fg">
        Requires {ENGINE_INFO[target.engine].name}
      </span>
      {cmd && <CopyCommand command={cmd} compact />}
    </span>
  );
}

function Subline({ item }: { item: QueueItem }) {
  const target = item.targets.find((t) => t.ext === item.target);
  if (item.targets.length === 0) return <span>Unsupported format · {formatBytes(item.size)}</span>;
  if (item.status === "error")
    return (
      <span className="selectable truncate font-sans text-danger" title={item.error}>
        {item.error}
      </span>
    );
  if (item.status === "done" && item.result) {
    const r = item.result;
    if (item.combined)
      return (
        <span title={r.outputPath}>
          Combined → {basename(r.outputPath)} · {formatBytes(r.outputSize)}
        </span>
      );
    return (
      <span title={r.outputPath}>
        {formatBytes(r.inputSize)} → <span className="text-fg">{formatBytes(r.outputSize)}</span>
      </span>
    );
  }
  if (target && !target.available && !isActive(item.status)) return <RequiresBadge target={target} />;
  return <span>{formatBytes(item.size)}</span>;
}

function StatusCell({ item, onRemove, onReveal }: { item: QueueItem; onRemove: () => void; onReveal: () => void }) {
  return (
    <div className="flex w-[128px] shrink-0 items-center justify-end gap-1.5 whitespace-nowrap">
      <AnimatePresence mode="popLayout" initial={false}>
        {item.status === "queued" && (
          <motion.span key="queued" {...fade} className="font-mono text-[11px] text-muted">
            Queued
          </motion.span>
        )}
        {item.status === "running" && (
          <motion.span key="running" {...fade} className="flex items-center gap-1.5 font-mono text-[11px] text-muted tabular">
            {item.progress != null ? `${Math.round(item.progress * 100)}%` : "Working"}
            <LoaderCircle size={14} className="animate-spin text-accent" />
          </motion.span>
        )}
        {item.status === "done" && item.result && (
          <motion.span key="done" {...fade} className="flex items-center gap-1">
            <button
              type="button"
              onClick={onReveal}
              className="flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px] text-muted transition-colors hover:bg-hover hover:text-fg"
              title={`Show ${basename(item.result.outputPath)} in folder`}
            >
              <FolderOpen size={14} />
              Open folder
            </button>
            <DoneMark />
          </motion.span>
        )}
        {item.status === "error" && (
          <motion.span key="error" {...fade} className="text-danger" title={item.error}>
            <CircleAlert size={15} />
          </motion.span>
        )}
      </AnimatePresence>
      {(item.status === "idle" || item.status === "error") && (
        <button
          type="button"
          onClick={onRemove}
          aria-label={`Remove ${item.name}`}
          className="grid size-7 place-items-center rounded-md text-muted opacity-0 transition group-hover:opacity-100 group-focus-within:opacity-100 hover:bg-hover hover:text-fg focus-visible:opacity-100"
        >
          <X size={14} />
        </button>
      )}
    </div>
  );
}

const fade = {
  initial: { opacity: 0, scale: 0.9 },
  animate: { opacity: 1, scale: 1 },
  exit: { opacity: 0, scale: 0.9 },
  transition: quick,
};

function QueueRow({
  item,
  onTargetChange,
  onRemove,
  onReveal,
}: {
  item: QueueItem;
  onTargetChange: (ext: string) => void;
  onRemove: () => void;
  onReveal: () => void;
}) {
  const progress = item.status === "done" ? 1 : item.progress ?? 0;
  return (
    <motion.li
      layout="position"
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, x: -8, transition: { duration: 0.14 } }}
      transition={settle}
      className="group relative flex h-[52px] items-center gap-3 border-b border-line px-4 last:border-b-0"
    >
      <ExtBadge ext={item.ext} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px]" title={item.path}>
          {item.name}
        </div>
        <div className="flex min-w-0 font-mono text-[11px] text-muted tabular">
          <Subline item={item} />
        </div>
      </div>
      <ArrowRight size={14} className="shrink-0 text-muted" />
      <FormatPicker
        value={item.target}
        targets={item.targets}
        onChange={onTargetChange}
        disabled={isActive(item.status)}
        ariaLabel={`Target format for ${item.name}`}
        sourceCategory={item.category}
      />
      <StatusCell item={item} onRemove={onRemove} onReveal={onReveal} />

      {/* Progress fill */}
      <AnimatePresence>
        {(item.status === "running" || item.status === "done") && (
          <motion.span
            key="bar"
            aria-hidden="true"
            className="absolute bottom-[-1px] left-0 h-[2px] w-full origin-left bg-accent"
            initial={{ scaleX: 0, opacity: 1 }}
            animate={{
              scaleX: item.progress == null && item.status === "running" ? 0.15 : progress,
              opacity: item.status === "done" ? 0 : 1,
            }}
            exit={{ opacity: 0 }}
            transition={{ scaleX: settle, opacity: { delay: 0.35, duration: 0.3 } }}
          />
        )}
      </AnimatePresence>
    </motion.li>
  );
}

// ---------------------------------------------------------------------------
// Footer controls
// ---------------------------------------------------------------------------

function OutputMenu({ dir, onSame, onChoose }: { dir: string | null; onSame: () => void; onChoose: () => void }) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  const close = useCallback(() => setOpen(false), []);
  const item = "flex w-full items-center gap-2 rounded-md px-2 h-8 text-left text-[12.5px] hover:bg-hover";

  return (
    <>
      <button
        ref={anchor}
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex h-8 max-w-[180px] min-w-0 items-center gap-1.5 rounded-md px-2 text-[12.5px] text-muted transition-colors hover:bg-hover hover:text-fg"
        title={dir ?? "Same folder as source"}
      >
        <Folder size={14} className="shrink-0" />
        <span className="truncate">{dir ? basename(dir) : "Same folder"}</span>
      </button>
      <Popover open={open} onClose={close} anchor={anchor} width={260} align="start" label="Output location">
        <button type="button" className={item} onClick={() => (onSame(), close())}>
          <span className="w-4">{!dir && <Check size={14} />}</span>
          Same folder as source
        </button>
        <button type="button" className={item} onClick={() => (close(), onChoose())}>
          <span className="w-4">{dir && <Check size={14} />}</span>
          {dir ? "Change folder…" : "Choose folder…"}
        </button>
        {dir && <p className="selectable truncate px-2 pt-1 pb-1.5 pl-8 font-mono text-[11px] text-muted" title={dir}>{dir}</p>}
        <p className="mt-1 border-t border-line px-2 pt-2 pb-1 text-[11px] text-muted">
          Originals are never overwritten. Existing names get a _1 suffix.
        </p>
      </Popover>
    </>
  );
}

export interface ImageOptions {
  quality: number;
  width: number | null;
}

function OptionsMenu({ value, onChange }: { value: ImageOptions; onChange: (v: ImageOptions) => void }) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  const close = useCallback(() => setOpen(false), []);
  const modified = value.quality !== 85 || value.width != null;

  return (
    <>
      <button
        ref={anchor}
        type="button"
        onClick={() => setOpen((o) => !o)}
        className={`relative grid size-8 place-items-center rounded-md transition-colors hover:bg-hover hover:text-fg ${open ? "bg-hover text-fg" : "text-muted"}`}
        title="Conversion options"
        aria-label="Conversion options"
      >
        <SlidersHorizontal size={14} />
        {modified && <span className="absolute top-1.5 right-1.5 size-1 rounded-full bg-accent" />}
      </button>
      <Popover open={open} onClose={close} anchor={anchor} width={256} align="start" label="Conversion options">
        <div className="flex flex-col gap-3 p-1.5">
          <label className="flex flex-col gap-1.5">
            <span className="flex items-baseline justify-between text-[12px]">
              Quality
              <span className="font-mono text-[11px] text-muted tabular">{value.quality}%</span>
            </span>
            <input
              type="range"
              min={60}
              max={100}
              step={1}
              value={value.quality}
              onChange={(e) => onChange({ ...value, quality: Number(e.target.value) })}
            />
            <span className="text-[11px] text-muted">JPG, WebP, AVIF. 100 = lossless WebP.</span>
          </label>
          <label className="flex flex-col gap-1.5">
            <span className="text-[12px]">Resize width</span>
            <div className="flex items-center gap-2">
              <input
                type="number"
                min={1}
                max={16384}
                placeholder="Original"
                value={value.width ?? ""}
                onChange={(e) => {
                  const n = parseInt(e.target.value, 10);
                  onChange({ ...value, width: Number.isFinite(n) && n > 0 ? n : null });
                }}
                className="selectable h-8 w-full rounded-md border border-line bg-canvas px-2 font-mono text-[12px] outline-none placeholder:text-muted focus:border-line-strong"
              />
              <span className="font-mono text-[11px] text-muted">px</span>
            </div>
            <span className="text-[11px] text-muted">Images and video. Aspect ratio is kept.</span>
          </label>
          {modified && (
            <button
              type="button"
              onClick={() => onChange({ quality: 85, width: null })}
              className="self-start text-[11.5px] text-muted underline-offset-2 hover:text-fg hover:underline"
            >
              Reset to defaults
            </button>
          )}
        </div>
      </Popover>
    </>
  );
}

// ---------------------------------------------------------------------------
// Queue
// ---------------------------------------------------------------------------

interface FileQueueProps {
  items: QueueItem[];
  onTargetChange: (id: string, ext: string) => void;
  onRemove: (id: string) => void;
  onReveal: (path: string) => void;

  batchTargets: TargetOption[];
  batchValue: string | null;
  onBatchChange: (ext: string) => void;

  canCombine: boolean;
  combine: boolean;
  onCombineChange: (v: boolean) => void;

  outputDir: string | null;
  onOutputSame: () => void;
  onOutputChoose: () => void;

  options: ImageOptions;
  onOptionsChange: (v: ImageOptions) => void;

  readyCount: number;
  busy: boolean;
  onConvert: () => void;
}

export function FileQueue(props: FileQueueProps) {
  const { items, busy, readyCount } = props;
  const showBatch = props.batchTargets.length > 0;

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={settle}
      className="mx-4 mb-4 flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-line bg-surface"
    >
      <ul className="scrollbar-thin min-h-0 flex-1 overflow-y-auto">
        <AnimatePresence initial={false}>
          {items.map((item) => (
            <QueueRow
              key={item.id}
              item={item}
              onTargetChange={(ext) => props.onTargetChange(item.id, ext)}
              onRemove={() => props.onRemove(item.id)}
              onReveal={() => item.result && props.onReveal(item.result.outputPath)}
            />
          ))}
        </AnimatePresence>
      </ul>

      <footer className="flex h-12 shrink-0 items-center gap-1 border-t border-line px-2">
        <AnimatePresence initial={false}>
          {showBatch && (
            <motion.div
              key="batch"
              layout
              initial={{ opacity: 0, width: 0 }}
              animate={{ opacity: 1, width: "auto" }}
              exit={{ opacity: 0, width: 0 }}
              transition={quick}
              className="flex items-center gap-2 overflow-hidden pr-1 pl-1.5"
            >
              <span className="text-[12px] whitespace-nowrap text-muted">Convert all to</span>
              <FormatPicker
                value={props.batchValue}
                targets={props.batchTargets}
                onChange={props.onBatchChange}
                disabled={busy}
                placeholder="Mixed"
                ariaLabel="Convert all files to"
              />
              {props.canCombine && (
                <label className="ml-1 flex h-8 items-center gap-1.5 rounded-md px-1.5 text-[12px] whitespace-nowrap text-muted hover:text-fg">
                  <input
                    type="checkbox"
                    checked={props.combine}
                    onChange={(e) => props.onCombineChange(e.target.checked)}
                    className="size-3.5 accent-[var(--accent)]"
                  />
                  One PDF
                </label>
              )}
              <span className="mx-1 h-4 w-px bg-line" aria-hidden="true" />
            </motion.div>
          )}
        </AnimatePresence>

        <motion.div layout="position" className="flex min-w-0 items-center gap-0.5">
          <OutputMenu dir={props.outputDir} onSame={props.onOutputSame} onChoose={props.onOutputChoose} />
          <OptionsMenu value={props.options} onChange={props.onOptionsChange} />
        </motion.div>

        <motion.button
          type="button"
          layout="position"
          onClick={props.onConvert}
          disabled={busy || readyCount === 0}
          whileTap={{ scale: 0.97 }}
          className="ml-auto flex h-8 shrink-0 items-center gap-2 rounded-md bg-fg px-3.5 text-[12.5px] font-medium text-canvas transition-opacity disabled:opacity-35"
        >
          {busy && <LoaderCircle size={14} className="animate-spin" />}
          {busy ? "Converting" : readyCount > 1 ? `Convert ${readyCount} files` : "Convert"}
        </motion.button>
      </footer>
    </motion.div>
  );
}
