# AnyFile

Convert anything, locally. No uploads, no limits.

AnyFile is a small desktop converter for documents, images, data files, audio and video. Drop files in, pick a format, done. Everything runs on your machine: there's no server, no account, no telemetry and no file size cap.

**[Download the latest release →](https://github.com/Xyst69/anyfile/releases/latest)** · **[Website](https://xyst69.github.io/anyfile/)**

## Download and run

**1. Download** the file for your system from the [latest release](https://github.com/Xyst69/anyfile/releases/latest) (under "Assets"):

| System  | File to download                                   |
| ------- | -------------------------------------------------- |
| Windows | `AnyFile_<version>_x64-setup.exe` (or the `.msi`)  |
| macOS   | `AnyFile_<version>_universal.dmg` (Apple Silicon and Intel) |
| Linux   | `AnyFile_<version>_amd64.AppImage` (any distro), `.deb` (Ubuntu/Debian) or `.rpm` (Fedora) |

**2. Install and open it.**

- **Windows:** run the `-setup.exe`. The installer isn't code-signed yet, so Windows may show "Windows protected your PC". Click **More info → Run anyway**. Then open **AnyFile** from the Start menu.
- **macOS:** open the `.dmg` and drag AnyFile into Applications. The first time, right-click the app and choose **Open → Open** (it isn't notarized yet). If macOS says the app is damaged, run `xattr -cr /Applications/AnyFile.app` in Terminal and try again.
- **Linux:** `chmod +x AnyFile_*.AppImage && ./AnyFile_*.AppImage`, or `sudo apt install ./AnyFile_*.deb`.

**3. Convert.** Drag files onto the window (or click it to browse), pick the format you want on each row, and press **Convert**. Results are saved next to the original files.

**4. Optional: unlock every format.** Images, data files, Markdown/HTML and text-to-PDF work immediately. Office documents, e-books, audio and video need three free tools. Install them once:

```bash
# Windows (PowerShell)
winget install JohnMacFarlane.Pandoc
winget install Gyan.FFmpeg
winget install TheDocumentFoundation.LibreOffice

# macOS
brew install pandoc ffmpeg && brew install --cask libreoffice

# Linux (Debian/Ubuntu)
sudo apt install pandoc ffmpeg libreoffice
```

Then restart AnyFile, or click the **engines** pill at the top right and press **Re-check**. It should say `4/4 engines ready`.

## Supported formats

Formats marked **Built-in** work out of the box. The rest use a free tool that AnyFile detects on your system. If it's missing, the app shows the one-line install command.

| Category  | From                                    | To                                          | Engine      |
| --------- | --------------------------------------- | ------------------------------------------- | ----------- |
| Images    | PNG, JPG, WebP, BMP, ICO, TIFF, GIF     | PNG, JPG, WebP, BMP, ICO, TIFF, GIF, AVIF, PDF | Built-in |
| Images    | SVG                                     | PNG, JPG, WebP, PDF                         | Built-in    |
| Images    | Several images                          | One combined PDF                            | Built-in    |
| Data      | JSON, YAML, TOML, XML, CSV              | JSON, YAML, TOML, XML, CSV                  | Built-in    |
| Text      | Markdown                                | HTML, TXT                                   | Built-in    |
| Text      | HTML                                    | Markdown, TXT                               | Built-in    |
| Text      | TXT                                     | PDF, HTML                                   | Built-in    |
| Documents | DOCX, DOC, RTF, ODT                     | PDF, DOCX, DOC, ODT, RTF, HTML, TXT         | LibreOffice |
| Documents | XLSX, XLS, ODS                          | PDF, XLSX, XLS, ODS, CSV, HTML              | LibreOffice |
| Documents | PPTX, PPT, ODP                          | PDF, PPTX, PPT, ODP                         | LibreOffice |
| Documents | PDF                                     | DOCX, ODT                                   | LibreOffice |
| Documents | MD, HTML, DOCX, ODT, EPUB, RST, LaTeX, RTF | DOCX, MD, HTML, EPUB, ODT, RST, LaTeX, RTF, TXT | Pandoc |
| Video     | MP4, MKV, MOV, WebM, AVI, GIF           | MP4, MKV, MOV, WebM, AVI, GIF               | FFmpeg      |
| Video     | MP4, MKV, MOV, WebM, AVI                | MP3, WAV, FLAC, M4A (extract audio)         | FFmpeg      |
| Audio     | MP3, WAV, FLAC, OGG, M4A, AAC, Opus     | MP3, WAV, FLAC, OGG, M4A, AAC, Opus         | FFmpeg      |

### Optional engines

| Engine      | Windows                                             | macOS                              | Linux (Debian/Ubuntu)         |
| ----------- | --------------------------------------------------- | ---------------------------------- | ----------------------------- |
| LibreOffice | `winget install TheDocumentFoundation.LibreOffice`  | `brew install --cask libreoffice`  | `sudo apt install libreoffice` |
| Pandoc      | `winget install JohnMacFarlane.Pandoc`              | `brew install pandoc`              | `sudo apt install pandoc`     |
| FFmpeg      | `winget install Gyan.FFmpeg`                        | `brew install ffmpeg`              | `sudo apt install ffmpeg`     |

## How it works

- **Output:** files are written next to the source by default, or to a folder you choose. Originals are never overwritten. If a name is taken, `_1`, `_2`… is appended.
- **Options:** quality (JPG / WebP / AVIF) and resize width (images and video).
- **Concurrency:** conversions run in parallel on background threads, so the UI stays responsive. FFmpeg jobs report live progress.
- **Engines:** discovered on `PATH` plus common install locations (e.g. `C:\Program Files\LibreOffice`, Homebrew, WinGet links), since GUI apps on macOS don't inherit your shell `PATH`.

## Build from source

### Prerequisites

- [Rust](https://rustup.rs) (stable) and [Node.js](https://nodejs.org) 20+
- **Windows:** Microsoft C++ Build Tools ("Desktop development with C++") with the MSVC Rust toolchain (`rustup default stable-msvc`). WebView2 ships with Windows 10/11.
- **macOS:** Xcode Command Line Tools (`xcode-select --install`)
- **Linux:** `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf build-essential`

See the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for details.

### Commands

```bash
npm install
npm run tauri dev      # run the app with hot reload
npm run tauri build    # produce installers in src-tauri/target/release/bundle/
cd src-tauri && cargo test   # backend unit tests
```

### Releasing

Bump the version in `src-tauri/tauri.conf.json`, then push a tag:

```bash
git tag v1.0.0 && git push origin v1.0.0
```

`.github/workflows/release.yml` builds Windows, macOS and Linux installers and publishes them to GitHub Releases. `.github/workflows/deploy-web.yml` deploys `/website` to GitHub Pages on every push to `main` (enable Pages → Source: GitHub Actions once).

## Project layout

```text
src/                React + TypeScript UI
  components/       DropZone, FileQueue, FormatPicker, EngineDrawer, TitleBar
  lib/              Tauri bindings, format metadata, theme, motion presets
src-tauri/src/
  lib.rs            Tauri commands and job orchestration
  types.rs          Types shared with the UI
  engines/          mod.rs (routing + discovery), image, data, pdf, office, media
website/            Static download page for GitHub Pages
```

## Tech stack

Tauri 2 · Rust · React 19 · TypeScript · Vite · Tailwind CSS 4 · Framer Motion · Lucide

Rust crates: `image`, `resvg`, `webp`, `lopdf`, `pulldown-cmark`, `htmd`, `serde_json`, `serde_norway`, `toml`, `csv`, `quick-xml`.

## Privacy

AnyFile makes no network requests. Files are read from and written to your disk only.

## License

[MIT](LICENSE)
