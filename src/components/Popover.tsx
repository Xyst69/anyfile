import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import { AnimatePresence, motion } from "framer-motion";
import { quick } from "../lib/motion";

interface PopoverProps {
  open: boolean;
  onClose: () => void;
  anchor: RefObject<HTMLElement | null>;
  width?: number;
  align?: "start" | "end";
  children: ReactNode;
  label?: string;
}

interface Position {
  left: number;
  top?: number;
  bottom?: number;
  maxHeight: number;
  above: boolean;
}

/**
 * Lightweight anchored popover rendered in a portal so it is never clipped
 * by scrolling containers. Flips above the anchor when space is short.
 */
export function Popover({ open, onClose, anchor, width = 240, align = "end", children, label }: PopoverProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<Position | null>(null);

  useLayoutEffect(() => {
    if (!open || !anchor.current) return;
    const r = anchor.current.getBoundingClientRect();
    const below = window.innerHeight - r.bottom;
    const above = r.top > below && below < 280;
    const rawLeft = align === "end" ? r.right - width : r.left;
    const left = Math.max(8, Math.min(rawLeft, window.innerWidth - width - 8));
    setPos(
      above
        ? { left, bottom: window.innerHeight - r.top + 6, maxHeight: r.top - 16, above }
        : { left, top: r.bottom + 6, maxHeight: below - 16, above },
    );
  }, [open, anchor, width, align]);

  useEffect(() => {
    if (!open) return;
    const inside = (t: EventTarget | null) =>
      !!t && (ref.current?.contains(t as Node) || anchor.current?.contains(t as Node));
    const onPointer = (e: PointerEvent) => !inside(e.target) && onClose();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    const onScroll = (e: Event) => !inside(e.target) && onClose();
    window.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onClose);
    };
  }, [open, onClose, anchor]);

  return createPortal(
    <AnimatePresence>
      {open && pos && (
        <motion.div
          ref={ref}
          role="dialog"
          aria-label={label}
          initial={{ opacity: 0, y: pos.above ? 4 : -4, scale: 0.98 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, y: pos.above ? 4 : -4, scale: 0.98, transition: { duration: 0.12 } }}
          transition={quick}
          style={{
            position: "fixed",
            left: pos.left,
            top: pos.top,
            bottom: pos.bottom,
            width,
            maxHeight: pos.maxHeight,
            transformOrigin: pos.above ? "bottom" : "top",
          }}
          className="scrollbar-thin z-50 overflow-y-auto rounded-lg border border-line bg-surface p-1.5 shadow-pop"
        >
          {children}
        </motion.div>
      )}
    </AnimatePresence>,
    document.body,
  );
}
