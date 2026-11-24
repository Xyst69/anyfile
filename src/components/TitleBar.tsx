import { Monitor, Moon, Sun } from "lucide-react";
import type { EngineStatus } from "../lib/tauri";
import type { ThemePref } from "../lib/theme";

interface TitleBarProps {
  engines: EngineStatus[];
  theme: ThemePref;
  onCycleTheme: () => void;
  onOpenEngines: () => void;
}

export function Logo({ size = 18 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 1024 1024" aria-hidden="true">
      <rect x="64" y="64" width="896" height="896" rx="208" fill="var(--fg)" />
      <rect x="292" y="232" width="336" height="432" rx="40" fill="none" stroke="var(--canvas)" strokeOpacity="0.42" strokeWidth="36" />
      <rect x="396" y="360" width="336" height="432" rx="40" fill="var(--canvas)" />
      <path d="M480 576 H640 M584 520 L640 576 L584 632" fill="none" stroke="var(--fg)" strokeWidth="40" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

const THEME_ICON = { system: Monitor, light: Sun, dark: Moon } as const;
const THEME_NEXT = { system: "light", light: "dark", dark: "system" } as const;

export function TitleBar({ engines, theme, onCycleTheme, onOpenEngines }: TitleBarProps) {
  const ready = engines.filter((e) => e.available).length;
  const total = engines.length || 4;
  const allReady = engines.length > 0 && ready === total;
  const ThemeIcon = THEME_ICON[theme];

  return (
    <header className="flex h-11 shrink-0 items-center justify-between border-b border-line px-4">
      <div className="flex items-center gap-2">
        <Logo />
        <span className="text-[13px] font-semibold tracking-[-0.01em]">AnyFile</span>
      </div>

      <div className="flex items-center gap-1.5">
        <button
          type="button"
          onClick={onOpenEngines}
          className="flex h-7 items-center gap-2 rounded-full border border-line px-2.5 font-mono text-[11px] text-muted transition-colors hover:border-line-strong hover:text-fg"
          title="Conversion engines"
        >
          <span
            className={`size-1.5 rounded-full ${allReady ? "bg-fg" : "border border-muted"}`}
            aria-hidden="true"
          />
          <span className="tabular">
            {engines.length === 0 ? "checking engines" : `${ready}/${total} engines ready`}
          </span>
        </button>

        <button
          type="button"
          onClick={onCycleTheme}
          className="grid size-7 place-items-center rounded-md text-muted transition-colors hover:bg-hover hover:text-fg"
          title={`Theme: ${theme} (switch to ${THEME_NEXT[theme]})`}
          aria-label={`Theme: ${theme}. Switch to ${THEME_NEXT[theme]}.`}
        >
          <ThemeIcon size={15} />
        </button>
      </div>
    </header>
  );
}
