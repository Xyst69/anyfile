import { useCallback, useId, useRef, useState } from "react";
import { motion } from "framer-motion";
import { ChevronDown } from "lucide-react";
import { CATEGORY_LABEL, ENGINE_INFO, groupTargets } from "../lib/formats";
import { snappy } from "../lib/motion";
import type { Category, TargetOption } from "../lib/tauri";
import { Popover } from "./Popover";

interface FormatPickerProps {
  value: string | null;
  targets: TargetOption[];
  onChange: (ext: string) => void;
  disabled?: boolean;
  placeholder?: string;
  ariaLabel?: string;
  /** Category of the source file; its group is listed first. */
  sourceCategory?: Category | null;
}

/** Grouped target-format selector. Only formats valid for the file are listed. */
export function FormatPicker({
  value,
  targets,
  onChange,
  disabled,
  placeholder = "—",
  ariaLabel,
  sourceCategory,
}: FormatPickerProps) {
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  const highlightId = useId();
  const close = useCallback(() => setOpen(false), []);

  const current = targets.find((t) => t.ext === value);
  const missingEngines = [...new Set(targets.filter((t) => !t.available).map((t) => ENGINE_INFO[t.engine].name))];

  const choose = (ext: string) => {
    onChange(ext);
    // Let the highlight glide to the new choice before closing.
    setTimeout(close, 140);
  };

  return (
    <>
      <button
        ref={anchor}
        type="button"
        disabled={disabled || targets.length === 0}
        onClick={() => setOpen((o) => !o)}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={ariaLabel ?? `Convert to ${value ?? "…"}`}
        className={`flex h-7 min-w-[76px] items-center justify-between gap-1.5 rounded-md border px-2 font-mono text-[11.5px] uppercase transition-colors disabled:opacity-40 ${
          open ? "border-line-strong bg-hover" : "border-line hover:border-line-strong"
        } ${current && !current.available ? "text-muted" : ""}`}
      >
        <span>{value ?? placeholder}</span>
        <ChevronDown size={12} className={`text-muted transition-transform duration-150 ${open ? "rotate-180" : ""}`} />
      </button>

      <Popover open={open} onClose={close} anchor={anchor} width={248} label="Target format">
        <div className="flex flex-col gap-2 p-1">
          {groupTargets(targets, sourceCategory).map((group) => (
            <div key={group.category}>
              <div className="mb-1 px-0.5 text-[10.5px] font-medium tracking-wide text-muted uppercase">
                {CATEGORY_LABEL[group.category]}
              </div>
              <div className="grid grid-cols-4 gap-1">
                {group.targets.map((t) => {
                  const selected = t.ext === value;
                  return (
                    <button
                      key={t.ext}
                      type="button"
                      onClick={() => choose(t.ext)}
                      title={t.available ? `Convert to ${t.ext.toUpperCase()}` : `Requires ${ENGINE_INFO[t.engine].name}`}
                      className={`relative h-7 rounded-md font-mono text-[11.5px] uppercase transition-colors ${
                        selected ? "text-white" : t.available ? "hover:bg-hover" : "text-muted hover:bg-hover"
                      }`}
                    >
                      {selected && (
                        <motion.span
                          layoutId={highlightId}
                          transition={snappy}
                          className="absolute inset-0 rounded-md bg-accent"
                        />
                      )}
                      <span className="relative">{t.ext}</span>
                      {!t.available && (
                        <span className="absolute top-1 right-1 size-1 rounded-full bg-muted" aria-hidden="true" />
                      )}
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
          {missingEngines.length > 0 && (
            <p className="border-t border-line px-0.5 pt-2 text-[11px] text-muted">
              <span className="mr-1.5 inline-block size-1 translate-y-[-2px] rounded-full bg-muted" />
              Requires {missingEngines.join(" or ")}
            </p>
          )}
        </div>
      </Popover>
    </>
  );
}
