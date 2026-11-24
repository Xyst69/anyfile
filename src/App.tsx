import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { LayoutGroup, MotionConfig } from "framer-motion";
import { LucideProvider } from "lucide-react";
import { DropZone } from "./components/DropZone";
import { EngineDrawer } from "./components/EngineDrawer";
import { FileQueue, isActive, type ImageOptions, type QueueItem } from "./components/FileQueue";
import { TitleBar } from "./components/TitleBar";
import { commonTargets, defaultTarget, formatBytes } from "./lib/formats";
import { quick } from "./lib/motion";
import { api, type ConversionRequest, type EngineStatus, type FileInfo, type ProgressPayload } from "./lib/tauri";
import { useTheme } from "./lib/theme";

const OUTPUT_KEY = "anyfile.outputDir";

function readOutputDir(): string | null {
  try {
    return localStorage.getItem(OUTPUT_KEY);
  } catch {
    return null;
  }
}

function newId() {
  return crypto.randomUUID();
}

function errorText(e: unknown) {
  return typeof e === "string" ? e : e instanceof Error ? e.message : "Conversion failed";
}

export default function App() {
  const { pref, cycle } = useTheme();
  const [items, setItems] = useState<QueueItem[]>([]);
  const [engines, setEngines] = useState<EngineStatus[]>([]);
  const [checking, setChecking] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [outputDir, setOutputDir] = useState<string | null>(readOutputDir);
  const [options, setOptions] = useState<ImageOptions>({ quality: 85, width: null });
  const [combine, setCombine] = useState(false);

  // Job id → queue item ids (one-to-one, except for "combine into one PDF").
  const jobs = useRef(new Map<string, string[]>());
  const itemsRef = useRef(items);
  itemsRef.current = items;

  const busy = items.some((i) => isActive(i.status));

  // --- Engines ----------------------------------------------------------------

  const refreshTargets = useCallback(async () => {
    const idle = itemsRef.current.filter((i) => !isActive(i.status));
    if (idle.length === 0) return;
    const infos = await api.inspect(idle.map((i) => i.path));
    const byPath = new Map(infos.map((f) => [f.path, f]));
    setItems((prev) =>
      prev.map((it) => {
        const info = byPath.get(it.path);
        if (!info || isActive(it.status)) return it;
        const keep = info.targets.some((t) => t.ext === it.target);
        return { ...it, targets: info.targets, target: keep ? it.target : defaultTarget(info) };
      }),
    );
  }, []);

  const recheck = useCallback(async () => {
    setChecking(true);
    try {
      setEngines(await api.engineStatus(true));
      await refreshTargets();
    } finally {
      setChecking(false);
    }
  }, [refreshTargets]);

  useEffect(() => {
    api.engineStatus().then(setEngines).catch(() => {});
  }, []);

  // --- Adding files -----------------------------------------------------------

  const addPaths = useCallback(async (paths: string[]) => {
    const existing = new Set(itemsRef.current.map((i) => i.path));
    const fresh = [...new Set(paths)].filter((p) => !existing.has(p));
    if (fresh.length === 0) return;
    let infos: FileInfo[];
    try {
      infos = await api.inspect(fresh);
    } catch {
      return;
    }
    setItems((prev) => {
      // New files follow the current batch choice when it applies to them.
      const shared = new Set(prev.filter((i) => !isActive(i.status)).map((i) => i.target));
      const batch = shared.size === 1 ? [...shared][0] : null;
      return [
        ...prev,
        ...infos.map<QueueItem>((info) => ({
          ...info,
          id: newId(),
          target: defaultTarget(info, batch),
          status: "idle",
          progress: null,
        })),
      ];
    });
  }, []);

  const browse = useCallback(async () => {
    const paths = await api.pickFiles().catch(() => []);
    if (paths.length) addPaths(paths);
  }, [addPaths]);

  // Native drag & drop gives real filesystem paths.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    api
      .onDragDrop((e) => {
        if (e.type === "enter") setDragging(true);
        else if (e.type === "leave") setDragging(false);
        else if (e.type === "drop") {
          setDragging(false);
          addPaths(e.paths);
        }
      })
      .then((fn) => (unlisten = fn))
      .catch(() => {});
    return () => unlisten?.();
  }, [addPaths]);

  // Ctrl/Cmd + O to browse.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "o") {
        e.preventDefault();
        browse();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [browse]);

  // --- Progress events --------------------------------------------------------

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    api
      .onProgress((p: ProgressPayload) => {
        const ids = jobs.current.get(p.id);
        if (!ids) return;
        const combined = ids.length > 1;
        setItems((prev) =>
          prev.map((it) =>
            ids.includes(it.id)
              ? {
                  ...it,
                  status: p.status,
                  progress: p.progress ?? (p.status === "running" ? it.progress : null),
                  result: p.result ?? it.result,
                  error: p.error ?? undefined,
                  combined,
                }
              : it,
          ),
        );
        if (p.status === "done" || p.status === "error") jobs.current.delete(p.id);
      })
      .then((fn) => (unlisten = fn))
      .catch(() => {});
    return () => unlisten?.();
  }, []);

  // --- Queue edits --------------------------------------------------------------

  const resetIfFinished = (it: QueueItem): QueueItem =>
    it.status === "done" || it.status === "error"
      ? { ...it, status: "idle", progress: null, result: undefined, error: undefined, combined: false }
      : it;

  const setTarget = useCallback((id: string, ext: string) => {
    setItems((prev) => prev.map((it) => (it.id === id ? { ...resetIfFinished(it), target: ext } : it)));
  }, []);

  const setAllTargets = useCallback((ext: string) => {
    setItems((prev) =>
      prev.map((it) =>
        !isActive(it.status) && it.targets.some((t) => t.ext === ext) ? { ...resetIfFinished(it), target: ext } : it,
      ),
    );
  }, []);

  const remove = useCallback((id: string) => setItems((prev) => prev.filter((i) => i.id !== id)), []);

  const clearAll = useCallback(() => {
    if (itemsRef.current.some((i) => isActive(i.status))) return;
    setItems([]);
    setCombine(false);
  }, []);

  const chooseOutput = useCallback(async () => {
    const dir = await api.pickFolder().catch(() => null);
    if (!dir) return;
    setOutputDir(dir);
    try {
      localStorage.setItem(OUTPUT_KEY, dir);
    } catch {
      /* ignore */
    }
  }, []);

  const sameFolder = useCallback(() => {
    setOutputDir(null);
    try {
      localStorage.removeItem(OUTPUT_KEY);
    } catch {
      /* ignore */
    }
  }, []);

  // --- Derived state ------------------------------------------------------------

  /** Files that would be converted by pressing Convert. */
  const ready = useMemo(
    () =>
      items.filter(
        (i) =>
          (i.status === "idle" || i.status === "error") &&
          i.target &&
          i.targets.some((t) => t.ext === i.target && t.available),
      ),
    [items],
  );

  const pending = items.filter((i) => !isActive(i.status) && i.targets.length > 0);
  const batchTargets = pending.length >= 2 ? commonTargets(pending) : [];
  const batchTargetSet = new Set(pending.map((i) => i.target));
  const batchValue = batchTargetSet.size === 1 ? [...batchTargetSet][0] : null;

  const canCombine = ready.length >= 2 && ready.every((i) => i.category === "image" && i.target === "pdf");
  const willCombine = canCombine && combine;

  // --- Convert ------------------------------------------------------------------

  const convert = useCallback(async () => {
    if (ready.length === 0) return;
    const ids = new Set(ready.map((i) => i.id));
    setItems((prev) =>
      prev.map((it) =>
        ids.has(it.id) ? { ...it, status: "queued", progress: 0, result: undefined, error: undefined } : it,
      ),
    );

    const markFailed = (failed: string[], e: unknown) =>
      setItems((prev) =>
        prev.map((it) =>
          failed.includes(it.id) && isActive(it.status) ? { ...it, status: "error", error: errorText(e) } : it,
        ),
      );

    if (willCombine) {
      const jobId = newId();
      const memberIds = ready.map((i) => i.id);
      jobs.current.set(jobId, memberIds);
      api
        .combineToPdf({ id: jobId, paths: ready.map((i) => i.path), outputDir })
        .catch((e) => markFailed(memberIds, e));
      return;
    }

    const requests: ConversionRequest[] = ready.map((i) => {
      jobs.current.set(i.id, [i.id]);
      return {
        id: i.id,
        path: i.path,
        target: i.target!,
        outputDir,
        options: { quality: options.quality, width: options.width ?? undefined },
      };
    });
    // Per-file results arrive through progress events; this only catches
    // failures of the batch call itself.
    api.convertBatch(requests).catch((e) => markFailed(requests.map((r) => r.id), e));
  }, [ready, willCombine, outputDir, options]);

  const reveal = useCallback((path: string) => {
    api.revealInFolder(path).catch(() => {});
  }, []);

  const totalSize = items.reduce((n, i) => n + i.size, 0);
  const summary = `${items.length} ${items.length === 1 ? "file" : "files"} · ${formatBytes(totalSize)}`;

  return (
    <LucideProvider strokeWidth={1.5} size={14}>
      <MotionConfig reducedMotion="user" transition={quick}>
        <div className="flex h-full flex-col">
          <TitleBar
            engines={engines}
            theme={pref}
            onCycleTheme={cycle}
            onOpenEngines={() => setDrawerOpen(true)}
          />

          <main className="flex min-h-0 flex-1 flex-col">
            <LayoutGroup>
              {items.length === 0 ? (
                <DropZone compact={false} dragging={dragging} onBrowse={browse} />
              ) : (
                <>
                  <DropZone
                    compact
                    dragging={dragging}
                    onBrowse={browse}
                    summary={summary}
                    actions={
                      <button
                        type="button"
                        onClick={clearAll}
                        disabled={busy}
                        className="h-8 rounded-md px-2.5 text-[12px] text-muted transition-colors hover:bg-hover hover:text-fg disabled:opacity-40"
                      >
                        Clear
                      </button>
                    }
                  />
                  <FileQueue
                    items={items}
                    onTargetChange={setTarget}
                    onRemove={remove}
                    onReveal={reveal}
                    batchTargets={batchTargets}
                    batchValue={batchValue}
                    onBatchChange={setAllTargets}
                    canCombine={canCombine}
                    combine={combine}
                    onCombineChange={setCombine}
                    outputDir={outputDir}
                    onOutputSame={sameFolder}
                    onOutputChoose={chooseOutput}
                    options={options}
                    onOptionsChange={setOptions}
                    readyCount={willCombine ? 1 : ready.length}
                    busy={busy}
                    onConvert={convert}
                  />
                </>
              )}
            </LayoutGroup>
          </main>

          <EngineDrawer
            open={drawerOpen}
            engines={engines}
            checking={checking}
            onClose={() => setDrawerOpen(false)}
            onRecheck={recheck}
          />
        </div>
      </MotionConfig>
    </LucideProvider>
  );
}
