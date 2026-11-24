//! Tier 2 document bridges: LibreOffice (headless) and Pandoc.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{command, run_to_completion, ConvertError, Result};

const OFFICE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// Percent-encodes a filesystem path as a `file://` URL.
fn file_url(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let mut url = String::from(if raw.starts_with('/') { "file://" } else { "file:///" });
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'.' | b'_' | b'~' | b'-' | b':' => url.push(b as char),
            _ => url.push_str(&format!("%{b:02X}")),
        }
    }
    url
}

/// `--convert-to` argument: extension plus an explicit filter where the
/// default would pick the wrong one or a non-UTF-8 encoding.
fn convert_to_spec(from: &str, to: &str) -> String {
    let calc = matches!(from, "xlsx" | "xls" | "ods" | "csv");
    match to {
        "txt" => "txt:Text (encoded):UTF8".into(),
        // 44 = comma, 34 = double quote, 76 = UTF-8
        "csv" => "csv:Text - txt - csv (StarCalc):44,34,76".into(),
        "html" if calc => "html:HTML (StarCalc)".into(),
        "html" => "html:HTML (StarWriter)".into(),
        "docx" if from == "pdf" => "docx:MS Word 2007 XML".into(),
        "odt" if from == "pdf" => "odt:writer8".into(),
        other => other.into(),
    }
}

/// Runs `soffice --headless --convert-to` into a private temp directory with
/// its own user profile, so several conversions can run in parallel and an
/// open LibreOffice window is never disturbed.
pub async fn libreoffice(soffice: &Path, input: &Path, output: &Path, from: &str, to: &str) -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let profile = tmp.path().join("profile");
    let outdir = tmp.path().join("out");
    std::fs::create_dir_all(&outdir)?;

    let mut cmd = command(soffice);
    cmd.arg(format!("-env:UserInstallation={}", file_url(&profile)))
        .args(["--headless", "--invisible", "--norestore", "--nolockcheck", "--nodefault", "--nofirststartwizard"]);
    if from == "pdf" {
        cmd.arg("--infilter=writer_pdf_import");
    }
    if from == "csv" {
        // Comma separated, double-quoted, UTF-8, first line as header.
        cmd.arg("--infilter=CSV:44,34,76,1");
    }
    cmd.arg("--convert-to")
        .arg(convert_to_spec(from, to))
        .arg("--outdir")
        .arg(&outdir)
        .arg(input);

    let log = run_to_completion(cmd, OFFICE_TIMEOUT, "LibreOffice").await?;

    // soffice can exit 0 without producing anything; look for the file.
    let produced = find_output(&outdir, to).ok_or_else(|| {
        let detail = super::tail(&log, 3);
        ConvertError::failed(if detail.is_empty() {
            "LibreOffice could not convert this file".to_string()
        } else {
            format!("LibreOffice could not convert this file: {detail}")
        })
    })?;
    move_into(&produced, output)
}

fn find_output(dir: &Path, ext: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)))
}

/// Copies the temp result over the reserved output path (works across volumes).
fn move_into(src: &Path, dst: &Path) -> Result<()> {
    if std::fs::rename(src, dst).is_err() {
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

fn pandoc_format(ext: &str) -> &str {
    match ext {
        "md" => "markdown",
        "tex" => "latex",
        "txt" => "plain",
        other => other,
    }
}

pub async fn pandoc(pandoc: &Path, input: &Path, output: &Path, from: &str, to: &str) -> Result<()> {
    let mut cmd = command(pandoc);
    cmd.arg(input)
        .arg("-f")
        .arg(pandoc_format(from))
        .arg("-t")
        .arg(pandoc_format(to))
        .arg("-o")
        .arg(output);
    if matches!(to, "html" | "tex" | "rtf") {
        cmd.arg("--standalone");
    }
    if to == "html" || to == "epub" {
        let title = input.file_stem().and_then(|s| s.to_str()).unwrap_or("Document");
        cmd.arg("--metadata").arg(format!("title={title}"));
    }
    if let Some(dir) = input.parent() {
        // Lets relative image links in Markdown/HTML resolve.
        cmd.arg("--resource-path").arg(dir);
    }
    run_to_completion(cmd, OFFICE_TIMEOUT, "Pandoc").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_urls_are_encoded() {
        if cfg!(windows) {
            assert_eq!(file_url(Path::new(r"C:\Temp\my dir\p")), "file:///C:/Temp/my%20dir/p");
        } else {
            assert_eq!(file_url(Path::new("/tmp/my dir/p")), "file:///tmp/my%20dir/p");
        }
    }

    #[test]
    fn convert_specs() {
        assert_eq!(convert_to_spec("docx", "pdf"), "pdf");
        assert_eq!(convert_to_spec("xlsx", "csv"), "csv:Text - txt - csv (StarCalc):44,34,76");
        assert_eq!(convert_to_spec("docx", "html"), "html:HTML (StarWriter)");
    }
}
