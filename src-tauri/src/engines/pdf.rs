//! Built-in PDF writer: images → PDF (one page per image) and TXT → PDF.
//!
//! JPEGs are embedded as-is (DCTDecode, no re-encode); everything else is
//! flattened to RGB and Flate-compressed.

use std::path::Path;

use image::{DynamicImage, RgbImage};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

use super::{ConvertError, Result};

const A4: (f32, f32) = (595.28, 841.89);

fn pdf_err(e: impl std::fmt::Display) -> ConvertError {
    ConvertError::failed(format!("PDF error: {e}"))
}

enum Pixels {
    Jpeg { bytes: Vec<u8>, gray: bool },
    Rgb(RgbImage),
}

pub struct PdfImage {
    width: u32,
    height: u32,
    pixels: Pixels,
}

impl PdfImage {
    /// Uses the original JPEG bytes when possible, otherwise the decoded image.
    pub fn from_source(path: &Path, ext: &str, decoded: DynamicImage) -> Result<Self> {
        if ext == "jpg" {
            let bytes = std::fs::read(path)?;
            if let Some((w, h, components)) = jpeg_info(&bytes) {
                if components == 1 || components == 3 {
                    return Ok(Self { width: w, height: h, pixels: Pixels::Jpeg { bytes, gray: components == 1 } });
                }
            }
        }
        let rgb = super::image::flatten_to_rgb(&decoded);
        Ok(Self { width: rgb.width(), height: rgb.height(), pixels: Pixels::Rgb(rgb) })
    }

    pub fn open(path: &Path) -> Result<Self> {
        let ext = super::ext_of(path);
        let decoded = super::image::load(path, &ext, &Default::default())?;
        Self::from_source(path, &ext, decoded)
    }

    fn into_stream(self) -> Result<Stream> {
        let mut dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => self.width as i64,
            "Height" => self.height as i64,
            "BitsPerComponent" => 8,
        };
        Ok(match self.pixels {
            Pixels::Jpeg { bytes, gray } => {
                dict.set("ColorSpace", if gray { "DeviceGray" } else { "DeviceRGB" });
                dict.set("Filter", "DCTDecode");
                Stream::new(dict, bytes).with_compression(false)
            }
            Pixels::Rgb(rgb) => {
                dict.set("ColorSpace", "DeviceRGB");
                let mut stream = Stream::new(dict, rgb.into_raw());
                stream.compress().map_err(pdf_err)?;
                stream
            }
        })
    }
}

/// Reads width, height and component count from a baseline/progressive JPEG.
fn jpeg_info(b: &[u8]) -> Option<(u32, u32, u8)> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if marker == 0x01 || (0xD0..=0xD8).contains(&marker) {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        match marker {
            0xC0..=0xC2 => {
                if i + 9 >= b.len() {
                    return None;
                }
                let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
                let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
                return Some((w, h, b[i + 9]));
            }
            // Other SOF types (lossless, arithmetic) aren't safe to pass through.
            0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return None,
            _ => i += 2 + len,
        }
    }
    None
}

struct PdfBuilder {
    doc: Document,
    pages_id: ObjectId,
    kids: Vec<Object>,
}

impl PdfBuilder {
    fn new() -> Self {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        Self { doc, pages_id, kids: Vec::new() }
    }

    fn add_page(&mut self, size: (f32, f32), resources: lopdf::Dictionary, ops: Vec<Operation>) -> Result<()> {
        let content = Content { operations: ops }.encode().map_err(pdf_err)?;
        let content_id = self.doc.add_object(Stream::new(dictionary! {}, content));
        let page_id = self.doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => self.pages_id,
            "MediaBox" => vec![0.into(), 0.into(), size.0.into(), size.1.into()],
            "Contents" => content_id,
            "Resources" => resources,
        });
        self.kids.push(page_id.into());
        Ok(())
    }

    fn save(mut self, output: &Path) -> Result<()> {
        let count = self.kids.len() as i64;
        self.doc.objects.insert(
            self.pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => self.kids, "Count" => count }),
        );
        let catalog_id = self.doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => self.pages_id });
        self.doc.trailer.set("Root", catalog_id);
        self.doc.compress();
        self.doc.save(output).map_err(pdf_err)?;
        Ok(())
    }
}

/// One A4 page per image, orientation matched to the image, centred with a
/// margin. Images are never upscaled beyond 96 DPI.
pub fn images_to_pdf(images: Vec<PdfImage>, output: &Path) -> Result<()> {
    if images.is_empty() {
        return Err(ConvertError::failed("No images to combine"));
    }
    const MARGIN: f32 = 36.0;
    let mut pdf = PdfBuilder::new();
    for img in images {
        let (iw, ih) = (img.width as f32 * 0.75, img.height as f32 * 0.75);
        let page = if iw > ih { (A4.1, A4.0) } else { A4 };
        let scale = ((page.0 - 2.0 * MARGIN) / iw).min((page.1 - 2.0 * MARGIN) / ih).min(1.0);
        let (w, h) = (iw * scale, ih * scale);
        let (x, y) = ((page.0 - w) / 2.0, (page.1 - h) / 2.0);

        let image_id = pdf.doc.add_object(img.into_stream()?);
        let resources = dictionary! { "XObject" => dictionary! { "Im0" => image_id } };
        let ops = vec![
            Operation::new("q", vec![]),
            Operation::new("cm", vec![w.into(), 0.into(), 0.into(), h.into(), x.into(), y.into()]),
            Operation::new("Do", vec![Object::Name(b"Im0".to_vec())]),
            Operation::new("Q", vec![]),
        ];
        pdf.add_page(page, resources, ops)?;
    }
    pdf.save(output)
}

/// Maps a char to the WinAnsi (CP1252) encoding used by the standard fonts.
fn win_ansi(c: char) -> u8 {
    match c as u32 {
        0x20..=0x7E | 0xA0..=0xFF => c as u32 as u8,
        _ => match c {
            '€' => 0x80, '‚' => 0x82, 'ƒ' => 0x83, '„' => 0x84, '…' => 0x85, '†' => 0x86,
            '‡' => 0x87, 'ˆ' => 0x88, '‰' => 0x89, 'Š' => 0x8A, '‹' => 0x8B, 'Œ' => 0x8C,
            'Ž' => 0x8E, '‘' => 0x91, '’' => 0x92, '“' => 0x93, '”' => 0x94, '•' => 0x95,
            '–' => 0x96, '—' => 0x97, '˜' => 0x98, '™' => 0x99, 'š' => 0x9A, '›' => 0x9B,
            'œ' => 0x9C, 'ž' => 0x9E, 'Ÿ' => 0x9F,
            _ => b'?',
        },
    }
}

/// Hard-wraps a line at `width` chars, preferring to break after a space.
fn wrap(line: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= width {
        return vec![line.to_string()];
    }
    let mut out = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let mut end = (start + width).min(chars.len());
        if end < chars.len() {
            if let Some(space) = chars[start..end].iter().rposition(|&c| c == ' ') {
                if space > width / 3 {
                    end = start + space + 1;
                }
            }
        }
        out.push(chars[start..end].iter().collect::<String>().trim_end().to_string());
        start = end;
    }
    out
}

/// Typesets plain text in 10pt Courier on A4 pages. Form feeds start a new page.
pub fn text_to_pdf(bytes: &[u8], output: &Path) -> Result<()> {
    const MARGIN: f32 = 50.0;
    const SIZE: f32 = 10.0;
    const LEADING: f32 = 12.0;
    let cols = ((A4.0 - 2.0 * MARGIN) / (SIZE * 0.6)) as usize;
    let rows = ((A4.1 - 2.0 * MARGIN) / LEADING) as usize;

    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");

    // Split into pages of wrapped lines.
    let mut pages: Vec<Vec<String>> = vec![Vec::new()];
    for raw in text.split('\n') {
        let mut segments = raw.split('\x0c').peekable();
        while let Some(seg) = segments.next() {
            for line in wrap(&seg.replace('\t', "    "), cols) {
                if pages.last().unwrap().len() >= rows {
                    pages.push(Vec::new());
                }
                pages.last_mut().unwrap().push(line);
            }
            if segments.peek().is_some() {
                pages.push(Vec::new());
            }
        }
    }

    let mut pdf = PdfBuilder::new();
    let font_id = pdf.doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Courier",
        "Encoding" => "WinAnsiEncoding",
    });
    for lines in pages {
        let mut ops = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), SIZE.into()]),
            Operation::new("TL", vec![LEADING.into()]),
            Operation::new("Td", vec![MARGIN.into(), (A4.1 - MARGIN - SIZE).into()]),
        ];
        for line in lines {
            let encoded: Vec<u8> = line.chars().map(win_ansi).collect();
            ops.push(Operation::new("Tj", vec![Object::string_literal(encoded)]));
            ops.push(Operation::new("T*", vec![]));
        }
        ops.push(Operation::new("ET", vec![]));
        let resources = dictionary! { "Font" => dictionary! { "F1" => font_id } };
        pdf.add_page(A4, resources, ops)?;
    }
    pdf.save(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_to_pdf_paginates() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.pdf");
        let text: String = (0..150).map(|i| format!("Line {i} — café\n")).collect();
        text_to_pdf(text.as_bytes(), &out).unwrap();
        let doc = Document::load(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 3);
        let page1 = doc.extract_text(&[1]).unwrap();
        assert!(page1.contains("Line 0"), "{page1}");
    }

    #[test]
    fn png_and_jpg_to_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        let jpg = dir.path().join("b.jpg");
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(300, 200, image::Rgb([200, 30, 30])));
        img.save(&png).unwrap();
        img.save(&jpg).unwrap();
        let a = PdfImage::open(&png).unwrap();
        let b = PdfImage::open(&jpg).unwrap();
        assert!(matches!(b.pixels, Pixels::Jpeg { gray: false, .. }));
        let out = dir.path().join("combined.pdf");
        images_to_pdf(vec![a, b], &out).unwrap();
        let doc = Document::load(&out).unwrap();
        assert_eq!(doc.get_pages().len(), 2);
    }

    #[test]
    fn wrap_prefers_spaces() {
        let lines = wrap("aaaa bbbb cccc dddd", 10);
        assert_eq!(lines, vec!["aaaa bbbb", "cccc dddd"]);
    }
}
