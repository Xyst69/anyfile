import { useEffect, useState } from "react";
import { Check, Copy } from "lucide-react";

async function writeClipboard(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
  }
}

/** A command snippet with a one-click copy action. */
export function CopyCommand({ command, compact = false }: { command: string; compact?: boolean }) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const t = setTimeout(() => setCopied(false), 1400);
    return () => clearTimeout(t);
  }, [copied]);

  return (
    <button
      type="button"
      onClick={async (e) => {
        e.stopPropagation();
        await writeClipboard(command);
        setCopied(true);
      }}
      title={copied ? "Copied" : `Copy: ${command}`}
      className={`group/copy flex min-w-0 items-center gap-2 rounded-md border border-line bg-canvas font-mono text-muted transition-colors hover:border-line-strong hover:text-fg ${
        compact ? "h-5 px-1.5 text-[10.5px]" : "h-8 w-full px-2.5 text-[11.5px]"
      }`}
    >
      <span className="truncate">{compact ? (copied ? "Copied" : "Copy install command") : command}</span>
      <span className="ml-auto shrink-0">
        {copied ? <Check size={12} className="text-fg" /> : <Copy size={12} />}
      </span>
    </button>
  );
}
