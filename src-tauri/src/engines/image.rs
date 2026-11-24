//! Built-in raster image conversion (and SVG rasterisation).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use image::codecs::avif::AvifEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader, RgbImage, RgbaImage};

use super::{ConvertError, Result};
use crate::types::ConversionOptions;

const DEFAULT_QUALITY: u8 = 85;
/// SVGs smaller than this are upscaled so icons don't rasterise to 24×24.
const SVG_MIN_LONG_SIDE: f32 = 1024.0;

pub fn load(path: &Path, ext: &str, opts: &ConversionOptions) -> Result<DynamicImage> {
    if ext == "svg" {
        return render_svg(path, opts);
    }
    let reader = ImageReader::open(path)?
        .with_guessed_format()
        .map_err(ConvertError::from)?;
    reader
        .decode()
        .map_err(|e| ConvertError::failed(format!("Could not read image: {e}")))
}

/// Rasterises an SVG. If a width/height is requested it renders directly at
/// that size (crisp), otherwise small SVGs are scaled up to a usable size.
fn render_svg(path: &Path, opts: &ConversionOptions) -> Result<DynamicImage> {
    use resvg::{tiny_skia, usvg};

    let data = std::fs::read(path)?;
    let mut options = usvg::Options {
        resources_dir: path.parent().map(Path::to_path_buf),
        ..Default::default()
    };
    options.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_data(&data, &options)
        .map_err(|e| ConvertError::failed(format!("Invalid SVG: {e}")))?;

    let size = tree.size();
    let (w, h) = (size.width(), size.height());
    let scale = match (opts.width, opts.height) {
        (Some(tw), Some(th)) => (tw as f32 / w).min(th as f32 / h),
        (Some(tw), None) => tw as f32 / w,
        (None, Some(th)) => th as f32 / h,
        (None, None) => (SVG_MIN_LONG_SIDE / w.max(h)).max(1.0),
    };
    let pw = (w * scale).round().max(1.0) as u32;
    let ph = (h * scale).round().max(1.0) as u32;
    let mut pixmap = tiny_skia::Pixmap::new(pw, ph)
        .ok_or_else(|| ConvertError::failed("SVG is too large to render"))?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());

    // tiny-skia stores premultiplied alpha; image expects straight alpha.
    let mut rgba = Vec::with_capacity((pw * ph * 4) as usize);
    for px in pixmap.pixels() {
        let c = px.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    let img = RgbaImage::from_raw(pw, ph, rgba)
        .ok_or_else(|| ConvertError::failed("SVG render buffer mismatch"))?;
    Ok(DynamicImage::ImageRgba8(img))
}

/// Resizes preserving aspect ratio. With both dimensions it fits inside the box.
pub fn resize(img: DynamicImage, opts: &ConversionOptions) -> DynamicImage {
    let (w, h) = (img.width(), img.height());
    let (tw, th) = match (opts.width, opts.height) {
        (None, None) => return img,
        (Some(tw), Some(th)) => (tw, th),
        (Some(tw), None) => (tw, scale_dim(h, tw, w)),
        (None, Some(th)) => (scale_dim(w, th, h), th),
    };
    if (tw, th) == (w, h) || tw == 0 || th == 0 {
        return img;
    }
    img.resize(tw, th, FilterType::Lanczos3)
}

fn scale_dim(other: u32, target: u32, base: u32) -> u32 {
    ((other as u64 * target as u64) as f64 / base.max(1) as f64).round().max(1.0) as u32
}

/// Composites transparent pixels onto white (for formats without alpha).
pub fn flatten_to_rgb(img: &DynamicImage) -> RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (dst, src) in out.pixels_mut().zip(rgba.pixels()) {
        let a = src[3] as u32;
        for c in 0..3 {
            dst[c] = ((src[c] as u32 * a + 255 * (255 - a) + 127) / 255) as u8;
        }
    }
    out
}

pub fn save(img: DynamicImage, output: &Path, to: &str, quality: Option<u8>) -> Result<()> {
    let quality = quality.unwrap_or(DEFAULT_QUALITY).clamp(1, 100);
    let mut writer = BufWriter::new(File::create(output)?);
    let encode_err = |e: image::ImageError| ConvertError::failed(format!("Encoding failed: {e}"));

    match to {
        "jpg" => {
            let rgb = DynamicImage::ImageRgb8(flatten_to_rgb(&img));
            rgb.write_with_encoder(JpegEncoder::new_with_quality(&mut writer, quality))
                .map_err(encode_err)?;
        }
        "webp" => {
            let rgba = img.to_rgba8();
            let encoder = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
            let data = if quality >= 100 { encoder.encode_lossless() } else { encoder.encode(quality as f32) };
            writer.write_all(&data)?;
        }
        "avif" => {
            let rgba = DynamicImage::ImageRgba8(img.to_rgba8());
            rgba.write_with_encoder(AvifEncoder::new_with_speed_quality(&mut writer, 6, quality))
                .map_err(encode_err)?;
        }
        "ico" => {
            // ICO entries are limited to 256×256.
            let icon = if img.width() > 256 || img.height() > 256 {
                img.resize(256, 256, FilterType::Lanczos3)
            } else {
                img
            };
            DynamicImage::ImageRgba8(icon.to_rgba8())
                .write_to(&mut writer, ImageFormat::Ico)
                .map_err(encode_err)?;
        }
        "png" | "bmp" | "tiff" | "gif" => {
            let format = match to {
                "png" => ImageFormat::Png,
                "bmp" => ImageFormat::Bmp,
                "tiff" => ImageFormat::Tiff,
                _ => ImageFormat::Gif,
            };
            let normalized = match img {
                DynamicImage::ImageRgb8(_) | DynamicImage::ImageRgba8(_) | DynamicImage::ImageLuma8(_) => img,
                other if other.color().has_alpha() => DynamicImage::ImageRgba8(other.to_rgba8()),
                other => DynamicImage::ImageRgb8(other.to_rgb8()),
            };
            let normalized = if format == ImageFormat::Gif {
                DynamicImage::ImageRgba8(normalized.to_rgba8())
            } else {
                normalized
            };
            normalized.write_to(&mut writer, format).map_err(encode_err)?;
        }
        other => return Err(ConvertError::failed(format!("Unsupported image target: {other}"))),
    }
    writer.flush()?;
    Ok(())
}

impl From<image::ImageError> for ConvertError {
    fn from(e: image::ImageError) -> Self {
        ConvertError::failed(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn sample(dir: &Path) -> std::path::PathBuf {
        let mut img = RgbaImage::new(64, 32);
        for (x, _, px) in img.enumerate_pixels_mut() {
            *px = Rgba([x as u8 * 4, 80, 160, if x < 8 { 0 } else { 255 }]);
        }
        let path = dir.join("sample.png");
        img.save(&path).unwrap();
        path
    }

    #[test]
    fn png_to_jpg() {
        let dir = tempfile::tempdir().unwrap();
        let src = sample(dir.path());
        let out = dir.path().join("out.jpg");
        let img = load(&src, "png", &Default::default()).unwrap();
        save(img, &out, "jpg", Some(80)).unwrap();
        let back = image::open(&out).unwrap();
        assert_eq!((back.width(), back.height()), (64, 32));
        // Transparent pixels are flattened onto white.
        assert!(back.to_rgb8().get_pixel(0, 0)[0] > 240);
    }

    #[test]
    fn resize_preserves_aspect() {
        let dir = tempfile::tempdir().unwrap();
        let img = load(&sample(dir.path()), "png", &Default::default()).unwrap();
        let opts = ConversionOptions { width: Some(32), ..Default::default() };
        let r = resize(img, &opts);
        assert_eq!((r.width(), r.height()), (32, 16));
    }

    #[test]
    fn png_to_webp_ico_bmp_tiff_gif() {
        let dir = tempfile::tempdir().unwrap();
        let src = sample(dir.path());
        for to in ["webp", "ico", "bmp", "tiff", "gif"] {
            let out = dir.path().join(format!("out.{to}"));
            save(load(&src, "png", &Default::default()).unwrap(), &out, to, None).unwrap();
            let back = image::open(&out).unwrap_or_else(|e| panic!("{to}: {e}"));
            assert_eq!(back.width(), 64, "{to}");
        }
    }

    #[test]
    fn png_to_avif() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.avif");
        save(load(&sample(dir.path()), "png", &Default::default()).unwrap(), &out, "avif", Some(70)).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        // ISO-BMFF header: size, "ftyp", major brand "avif".
        assert_eq!(&bytes[4..12], b"ftypavif");
    }

    #[test]
    fn svg_to_png() {
        let dir = tempfile::tempdir().unwrap();
        let svg = dir.path().join("icon.svg");
        std::fs::write(&svg, r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><rect width="24" height="24" fill="#2563EB"/></svg>"##).unwrap();
        let img = load(&svg, "svg", &Default::default()).unwrap();
        assert_eq!(img.width(), 1024);
        let out = dir.path().join("icon.png");
        save(img, &out, "png", None).unwrap();
        let px = image::open(&out).unwrap().to_rgba8().get_pixel(10, 10).0;
        assert_eq!(px, [0x25, 0x63, 0xEB, 255]);
    }
}
