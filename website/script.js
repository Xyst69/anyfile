// ─────────────────────────────────────────────────────────────────────────────
// CONFIG — replace with your GitHub "owner/repo" before deploying.
const REPO = "Xyst69/anyfile";
// ─────────────────────────────────────────────────────────────────────────────

const REPO_URL = `https://github.com/${REPO}`;
const RELEASES_URL = `${REPO_URL}/releases`;
const IS_PLACEHOLDER = REPO.startsWith("YOUR_USERNAME");

const PLATFORMS = {
  windows: { label: "Windows", match: [/x64-setup\.exe$/i, /\.exe$/i, /\.msi$/i] },
  macos: { label: "macOS", match: [/universal\.dmg$/i, /aarch64\.dmg$/i, /\.dmg$/i] },
  linux: { label: "Linux", match: [/\.AppImage$/i, /\.deb$/i, /\.rpm$/i] },
};

// Mirrors the routing table in src-tauri/src/engines/mod.rs.
const FORMATS = [
  { cat: "documents", from: ["docx", "doc", "rtf", "odt"], to: ["pdf", "docx", "doc", "odt", "rtf", "html", "txt"], engine: "LibreOffice" },
  { cat: "documents", from: ["xlsx", "xls", "ods"], to: ["pdf", "xlsx", "xls", "ods", "csv", "html"], engine: "LibreOffice" },
  { cat: "documents", from: ["pptx", "ppt", "odp"], to: ["pdf", "pptx", "ppt", "odp"], engine: "LibreOffice" },
  { cat: "documents", from: ["pdf"], to: ["docx", "odt"], engine: "LibreOffice" },
  { cat: "documents", from: ["md", "html", "docx", "odt", "epub", "rst", "tex", "rtf"], to: ["docx", "md", "html", "epub", "odt", "rst", "tex", "rtf", "txt"], engine: "Pandoc" },
  { cat: "documents", from: ["md"], to: ["html", "txt"], engine: "Built-in" },
  { cat: "documents", from: ["html"], to: ["md", "txt"], engine: "Built-in" },
  { cat: "documents", from: ["txt"], to: ["pdf", "html"], engine: "Built-in" },
  { cat: "images", from: ["png", "jpg", "webp", "bmp", "ico", "tiff", "gif"], to: ["png", "jpg", "webp", "bmp", "ico", "tiff", "gif", "avif", "pdf"], engine: "Built-in" },
  { cat: "images", from: ["svg"], to: ["png", "jpg", "webp", "pdf"], engine: "Built-in" },
  { cat: "data", from: ["json", "yaml", "toml", "xml", "csv"], to: ["json", "yaml", "toml", "xml", "csv"], engine: "Built-in" },
  { cat: "data", from: ["csv"], to: ["xlsx", "ods"], engine: "LibreOffice" },
  { cat: "audio", from: ["mp3", "wav", "flac", "ogg", "m4a", "aac", "opus"], to: ["mp3", "wav", "flac", "ogg", "m4a", "aac", "opus"], engine: "FFmpeg" },
  { cat: "video", from: ["mp4", "mkv", "mov", "webm", "avi", "gif"], to: ["mp4", "mkv", "mov", "webm", "avi", "gif"], engine: "FFmpeg" },
  { cat: "video", from: ["mp4", "mkv", "mov", "webm", "avi"], to: ["mp3", "wav", "flac", "m4a"], engine: "FFmpeg" },
];

const TABS = [
  ["all", "All"],
  ["documents", "Documents"],
  ["images", "Images"],
  ["data", "Data"],
  ["audio", "Audio"],
  ["video", "Video"],
];

// ── Helpers ──────────────────────────────────────────────────────────────────

function el(tag, attrs = {}, children = []) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "text") node.textContent = v;
    else node.setAttribute(k, v);
  }
  for (const child of [].concat(children)) node.append(child);
  return node;
}

function formatBytes(bytes) {
  if (!bytes) return "";
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  let n = bytes;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(n >= 10 || i === 0 ? 0 : 1)} ${units[i]}`;
}

function detectOS() {
  const platform = (navigator.userAgentData?.platform || navigator.platform || "").toLowerCase();
  const ua = navigator.userAgent.toLowerCase();
  if (platform.includes("win") || ua.includes("windows")) return "windows";
  if (platform.includes("mac") || ua.includes("mac os")) return "macos";
  if (platform.includes("linux") || ua.includes("linux") || ua.includes("x11")) return "linux";
  return "windows";
}

function findAsset(assets, os) {
  for (const re of PLATFORMS[os].match) {
    const hit = assets.find((a) => re.test(a.name));
    if (hit) return hit;
  }
  return null;
}

// ── Links ────────────────────────────────────────────────────────────────────

const LINKS = {
  repo: REPO_URL,
  releases: RELEASES_URL,
  license: `${REPO_URL}/blob/main/LICENSE`,
};

document.querySelectorAll("[data-link]").forEach((a) => {
  a.href = LINKS[a.dataset.link] || REPO_URL;
  a.rel = "noopener";
});

// ── Download button ──────────────────────────────────────────────────────────

const os = detectOS();
const button = document.getElementById("download");
const label = document.getElementById("download-label");
const meta = document.getElementById("release-meta");
const alt = document.getElementById("alt-links");

function renderAltLinks(release) {
  alt.replaceChildren();
  const others = Object.keys(PLATFORMS).filter((p) => p !== os);
  alt.append("Also for ");
  for (const p of others) {
    const asset = release ? findAsset(release.assets, p) : null;
    alt.append(el("a", { href: asset ? asset.browser_download_url : RELEASES_URL, text: PLATFORMS[p].label }), " · ");
  }
  alt.append(el("a", { href: RELEASES_URL, text: "All releases" }));
}

function renderFallback(message) {
  label.textContent = `Download for ${PLATFORMS[os].label}`;
  button.href = RELEASES_URL;
  meta.textContent = message;
  renderAltLinks(null);
}

async function loadRelease() {
  label.textContent = `Download for ${PLATFORMS[os].label}`;
  if (IS_PLACEHOLDER) {
    renderFallback("Set REPO in script.js to show the latest release");
    return;
  }
  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!res.ok) throw new Error(String(res.status));
    const release = await res.json();
    const asset = findAsset(release.assets || [], os);
    if (asset) {
      button.href = asset.browser_download_url;
      const ext = asset.name.split(".").pop();
      meta.textContent = [release.tag_name, formatBytes(asset.size), `.${ext}`].filter(Boolean).join(" · ");
    } else {
      button.href = release.html_url || RELEASES_URL;
      meta.textContent = `${release.tag_name} · see release page`;
    }
    renderAltLinks(release);
  } catch {
    renderFallback("No published release yet · see GitHub");
  }
}

loadRelease();

// ── Formats matrix ───────────────────────────────────────────────────────────

const tabs = document.querySelector(".tabs");
const matrix = document.getElementById("matrix");

function pills(list) {
  return el("div", { class: "pills" }, list.map((f) => el("span", { text: f })));
}

function renderMatrix(cat) {
  const rows = FORMATS.filter((r) => cat === "all" || r.cat === cat);
  matrix.replaceChildren(
    ...rows.map((r, i) => {
      const row = el("div", { class: "row" }, [
        pills(r.from),
        el("span", { class: "to-arrow", text: "→", "aria-hidden": "true" }),
        pills(r.to.filter((t) => !(r.from.length === 1 && r.from[0] === t))),
        el("span", { class: `engine ${r.engine === "Built-in" ? "builtin" : ""}`, text: r.engine }),
      ]);
      row.style.animationDelay = `${Math.min(i * 18, 180)}ms`;
      return row;
    }),
  );
}

TABS.forEach(([id, text]) => {
  const tab = el("button", { class: "tab", type: "button", role: "tab", "aria-selected": String(id === "all"), text });
  tab.addEventListener("click", () => {
    tabs.querySelectorAll(".tab").forEach((t) => t.setAttribute("aria-selected", "false"));
    tab.setAttribute("aria-selected", "true");
    renderMatrix(id);
  });
  tabs.append(tab);
});

renderMatrix("all");

// ── Theme toggle ─────────────────────────────────────────────────────────────

document.querySelector(".theme-toggle").addEventListener("click", () => {
  const root = document.documentElement;
  const systemDark = matchMedia("(prefers-color-scheme: dark)").matches;
  const current = root.dataset.theme || (systemDark ? "dark" : "light");
  const next = current === "dark" ? "light" : "dark";
  root.dataset.theme = next;
  try {
    localStorage.setItem("anyfile.site.theme", next);
  } catch {
    /* storage unavailable */
  }
});
