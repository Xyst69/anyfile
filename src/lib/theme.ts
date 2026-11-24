import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type ThemePref = "system" | "light" | "dark";

const KEY = "anyfile.theme";

function readPref(): ThemePref {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch {
    /* storage unavailable */
  }
  return "system";
}

/** System-aware theme with a manual override, persisted per machine. */
export function useTheme() {
  const [pref, setPref] = useState<ThemePref>(readPref);

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = pref === "dark" || (pref === "system" && mq.matches);
      document.documentElement.dataset.theme = dark ? "dark" : "light";
    };
    apply();
    mq.addEventListener("change", apply);
    try {
      localStorage.setItem(KEY, pref);
    } catch {
      /* ignore */
    }
    // Keep the native title bar in step with the app theme.
    getCurrentWindow()
      .setTheme(pref === "system" ? null : pref)
      .catch(() => {});
    return () => mq.removeEventListener("change", apply);
  }, [pref]);

  const cycle = useCallback(
    () => setPref((p) => (p === "system" ? "light" : p === "light" ? "dark" : "system")),
    [],
  );

  return { pref, cycle };
}
