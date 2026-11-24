import type { ReactNode } from "react";
import { motion } from "framer-motion";
import { FileUp, Plus } from "lucide-react";
import { quick } from "../lib/motion";

interface DropZoneProps {
  compact: boolean;
  dragging: boolean;
  onBrowse: () => void;
  summary?: string;
  actions?: ReactNode;
}

/**
 * Full-canvas drop target in the empty state; collapses (shared layoutId)
 * into a slim "Add files" bar once the queue has files.
 */
export function DropZone({ compact, dragging, onBrowse, summary, actions }: DropZoneProps) {
  const border = dragging ? "var(--accent)" : "var(--line-strong)";

  if (!compact) {
    return (
      <motion.button
        type="button"
        layoutId="dropzone"
        onClick={onBrowse}
        animate={{ scale: dragging ? 1.008 : 1, borderColor: border }}
        transition={quick}
        className="m-4 flex flex-1 flex-col items-center justify-center gap-3 rounded-xl border border-dashed bg-surface/60 text-center outline-offset-4"
        style={{ borderColor: border }}
        aria-label="Drop any file here, or click to browse"
      >
        <motion.span
          layout="position"
          animate={{ y: dragging ? -2 : 0 }}
          transition={quick}
          className={`grid size-11 place-items-center rounded-lg border border-line bg-surface ${dragging ? "text-accent" : "text-muted"}`}
        >
          <FileUp size={20} />
        </motion.span>
        <motion.span layout="position" className="flex flex-col gap-1">
          <span className="text-[15px] font-medium tracking-[-0.01em]">
            {dragging ? "Release to add" : "Drop any file here"}
          </span>
          <span className="text-[12px] text-muted">or click to browse · 100% local</span>
        </motion.span>
      </motion.button>
    );
  }

  return (
    <motion.div
      layoutId="dropzone"
      animate={{ borderColor: border, scale: dragging ? 1.004 : 1 }}
      transition={quick}
      className="mx-4 mt-3 mb-2 flex h-11 shrink-0 items-center gap-3 rounded-lg border border-dashed bg-surface/60 pr-1.5 pl-1.5"
      style={{ borderColor: border }}
    >
      <motion.button
        layout="position"
        type="button"
        onClick={onBrowse}
        className="flex h-8 items-center gap-1.5 rounded-md px-2.5 text-[12.5px] font-medium transition-colors hover:bg-hover"
      >
        <Plus size={14} />
        Add files
      </motion.button>
      <motion.span layout="position" className="truncate text-[12px] text-muted">
        {dragging ? "Release to add to the queue" : summary}
      </motion.span>
      <div className="ml-auto flex items-center gap-1">{actions}</div>
    </motion.div>
  );
}
