import { useEffect } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { ExternalLink, RotateCw, X } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ENGINE_INFO, installCommand } from "../lib/formats";
import { settle } from "../lib/motion";
import type { EngineStatus } from "../lib/tauri";
import { CopyCommand } from "./CopyButton";

interface EngineDrawerProps {
  open: boolean;
  engines: EngineStatus[];
  checking: boolean;
  onClose: () => void;
  onRecheck: () => void;
}

function EngineRow({ engine }: { engine: EngineStatus }) {
  const info = ENGINE_INFO[engine.id];
  const cmd = installCommand(engine.id);

  return (
    <li className="flex flex-col gap-2 border-b border-line px-4 py-3.5 last:border-b-0">
      <div className="flex items-center gap-2">
        <span
          className={`size-1.5 shrink-0 rounded-full ${engine.available ? "bg-fg" : "border border-muted"}`}
          aria-hidden="true"
        />
        <span className="text-[13px] font-medium">{info.name}</span>
        <span className="ml-auto font-mono text-[11px] text-muted">
          {engine.available ? (engine.version ? `v${engine.version}` : "Ready") : "Not found"}
        </span>
      </div>
      <p className="pl-3.5 text-[11.5px] leading-relaxed text-muted">{info.handles}</p>
      {engine.available && engine.path && (
        <p className="selectable truncate pl-3.5 font-mono text-[10.5px] text-muted" title={engine.path}>
          {engine.path}
        </p>
      )}
      {!engine.available && cmd && (
        <div className="flex flex-col gap-1.5 pl-3.5">
          <CopyCommand command={cmd} />
          {info.url && (
            <button
              type="button"
              onClick={() => openUrl(info.url!).catch(() => {})}
              className="flex items-center gap-1 self-start text-[11px] text-muted hover:text-fg"
            >
              Other install options <ExternalLink size={11} />
            </button>
          )}
        </div>
      )}
    </li>
  );
}

/** Right-hand drawer listing built-in and external conversion engines. */
export function EngineDrawer({ open, engines, checking, onClose, onRecheck }: EngineDrawerProps) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  const ready = engines.filter((e) => e.available).length;

  return (
    <AnimatePresence>
      {open && (
        <>
          <motion.div
            key="scrim"
            className="fixed inset-0 top-11 z-30 bg-black/10 dark:bg-black/40"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={settle}
            onClick={onClose}
          />
          <motion.aside
            key="drawer"
            role="dialog"
            aria-label="Conversion engines"
            className="fixed top-11 right-0 bottom-0 z-40 flex w-[320px] max-w-[calc(100vw-48px)] flex-col border-l border-line bg-surface shadow-pop"
            initial={{ x: 24, opacity: 0 }}
            animate={{ x: 0, opacity: 1 }}
            exit={{ x: 24, opacity: 0 }}
            transition={settle}
          >
            <div className="flex h-12 shrink-0 items-center gap-2 border-b border-line pr-2 pl-4">
              <span className="text-[13px] font-medium">Engines</span>
              <span className="font-mono text-[11px] text-muted tabular">
                {ready}/{engines.length}
              </span>
              <button
                type="button"
                onClick={onRecheck}
                disabled={checking}
                className="ml-auto flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px] text-muted transition-colors hover:bg-hover hover:text-fg disabled:opacity-60"
              >
                <RotateCw size={13} className={checking ? "animate-spin" : ""} />
                Re-check
              </button>
              <button
                type="button"
                onClick={onClose}
                aria-label="Close"
                className="grid size-7 place-items-center rounded-md text-muted hover:bg-hover hover:text-fg"
              >
                <X size={14} />
              </button>
            </div>

            <ul className="scrollbar-thin min-h-0 flex-1 overflow-y-auto">
              {engines.map((e) => (
                <EngineRow key={e.id} engine={e} />
              ))}
            </ul>

            <p className="shrink-0 border-t border-line px-4 py-3 text-[11px] leading-relaxed text-muted">
              External engines are optional and detected on your PATH. Install one, then press Re-check. Nothing
              ever leaves this machine.
            </p>
          </motion.aside>
        </>
      )}
    </AnimatePresence>
  );
}
