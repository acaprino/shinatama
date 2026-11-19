//! C44.1: bounded artifact previews (FRD-0001 R39).
//!
//! Images are accepted only as PNG, JPEG, GIF or WebP recognized by their signature, at most
//! 16 MiB and 40 megapixels as declared by their header, so a hostile file cannot make the
//! viewer decode an unbounded bitmap. The bytes go to the page as a `data:` URL; nothing is
//! fetched. SVG is shown as source text (it can carry scripts). DOCX has no qualified reader:
//! it is reported unavailable, never approximated.
use crate::navigator::{NavError, Root};
use serde::Serialize;
use std::fs;
use std::io::Read;

pub const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 40_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
}
impl ImageFormat {
    fn mime(self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Webp => "image/webp",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePreview {
    pub path: String,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub data_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum PreviewError {
    TooLarge { bytes: u64 },
    TooManyPixels { width: u32, height: u32 },
    Unsupported,
    Malformed,
    /// A format with no qualified reader in this build (DOCX).
    ReaderUnavailable { format: String },
    Navigation { message: String },
}
impl From<NavError> for PreviewError {
    fn from(error: NavError) -> Self {
        PreviewError::Navigation { message: error.to_string() }
    }
}
impl std::fmt::Display for PreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreviewError::TooLarge { bytes } => write!(f, "The image is {bytes} bytes; previews stop at 16 MiB."),
            PreviewError::TooManyPixels { width, height } => write!(f, "The image declares {width} x {height} pixels; previews stop at 40 megapixels."),
            PreviewError::Unsupported => write!(f, "This file type has no preview."),
            PreviewError::Malformed => write!(f, "The image header is malformed; it is not shown."),
            PreviewError::ReaderUnavailable { format } => write!(f, "{format} preview is not available: no reader has passed its fidelity fixtures yet."),
            PreviewError::Navigation { message } => write!(f, "{message}"),
        }
    }
}

fn be16(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes(b.get(i..i + 2)?.try_into().ok()?)))
}
fn le16(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?)))
}
fn be32(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?))
}
fn le24(b: &[u8], i: usize) -> Option<u32> {
    let s = b.get(i..i + 3)?;
    Some(u32::from(s[0]) | u32::from(s[1]) << 8 | u32::from(s[2]) << 16)
}

/// Format and declared dimensions from the header only.
pub fn sniff(bytes: &[u8]) -> Result<(ImageFormat, u32, u32), PreviewError> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        if bytes.get(12..16) != Some(b"IHDR") {
            return Err(PreviewError::Malformed);
        }
        return Ok((ImageFormat::Png, be32(bytes, 16).ok_or(PreviewError::Malformed)?, be32(bytes, 20).ok_or(PreviewError::Malformed)?));
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Ok((ImageFormat::Gif, le16(bytes, 6).ok_or(PreviewError::Malformed)?, le16(bytes, 8).ok_or(PreviewError::Malformed)?));
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        for _ in 0..4096 {
            while bytes.get(i) == Some(&0xFF) && bytes.get(i + 1) == Some(&0xFF) {
                i += 1;
            }
            if bytes.get(i) != Some(&0xFF) {
                return Err(PreviewError::Malformed);
            }
            let marker = *bytes.get(i + 1).ok_or(PreviewError::Malformed)?;
            let length = be16(bytes, i + 2).ok_or(PreviewError::Malformed)? as usize;
            if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                let height = be16(bytes, i + 5).ok_or(PreviewError::Malformed)?;
                let width = be16(bytes, i + 7).ok_or(PreviewError::Malformed)?;
                return Ok((ImageFormat::Jpeg, width, height));
            }
            if length < 2 {
                return Err(PreviewError::Malformed);
            }
            i += 2 + length;
        }
        return Err(PreviewError::Malformed);
    }
    if bytes.get(0..4) == Some(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return match bytes.get(12..16) {
            Some(b"VP8 ") => Ok((ImageFormat::Webp, le16(bytes, 26).ok_or(PreviewError::Malformed)? & 0x3FFF, le16(bytes, 28).ok_or(PreviewError::Malformed)? & 0x3FFF)),
            Some(b"VP8L") => {
                let bits = u32::from_le_bytes(bytes.get(21..25).ok_or(PreviewError::Malformed)?.try_into().map_err(|_| PreviewError::Malformed)?);
                Ok((ImageFormat::Webp, (bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
            }
            Some(b"VP8X") => Ok((ImageFormat::Webp, le24(bytes, 24).ok_or(PreviewError::Malformed)? + 1, le24(bytes, 27).ok_or(PreviewError::Malformed)? + 1)),
            _ => Err(PreviewError::Malformed),
        };
    }
    Err(PreviewError::Unsupported)
}

fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { A[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { A[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn image(root: &Root, relative: &str) -> Result<ImagePreview, PreviewError> {
    let lower = relative.to_ascii_lowercase();
    if lower.ends_with(".docx") || lower.ends_with(".doc") {
        return Err(PreviewError::ReaderUnavailable { format: "DOCX".into() });
    }
    let path = root.resolve(relative)?;
    let meta = fs::metadata(&path).map_err(|_| PreviewError::from(NavError::Unreadable))?;
    if !meta.is_file() {
        return Err(NavError::NotAFile.into());
    }
    if meta.len() > MAX_IMAGE_BYTES {
        return Err(PreviewError::TooLarge { bytes: meta.len() });
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    fs::File::open(&path)
        .and_then(|f| f.take(MAX_IMAGE_BYTES).read_to_end(&mut bytes))
        .map_err(|_| PreviewError::from(NavError::Unreadable))?;
    let (format, width, height) = sniff(&bytes)?;
    if width == 0 || height == 0 {
        return Err(PreviewError::Malformed);
    }
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(PreviewError::TooManyPixels { width, height });
    }
    Ok(ImagePreview {
        path: relative.replace('\\', "/"),
        format,
        width,
        height,
        bytes: bytes.len() as u64,
        data_url: format!("data:{};base64,{}", format.mime(), base64(&bytes)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        b.extend_from_slice(&width.to_be_bytes());
        b.extend_from_slice(&height.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        b
    }

    #[test]
    fn sniffs_formats_and_dimensions_from_headers() {
        assert_eq!(sniff(&png(3, 2)).unwrap(), (ImageFormat::Png, 3, 2));
        assert_eq!(sniff(b"GIF89a\x05\x00\x07\x00").unwrap(), (ImageFormat::Gif, 5, 7));
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x0A, 0x00, 0x14];
        assert_eq!(sniff(&jpeg).unwrap(), (ImageFormat::Jpeg, 20, 10));
        let mut webp = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\0\0\0\0".to_vec();
        webp.extend_from_slice(&[9, 0, 0, 4, 0, 0]);
        assert_eq!(sniff(&webp).unwrap(), (ImageFormat::Webp, 10, 5));
        assert_eq!(sniff(b"<svg onload=alert(1)>"), Err(PreviewError::Unsupported));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x01]), Err(PreviewError::Malformed));
        assert_eq!(sniff(&[0xFF, 0xD8, 0x00]), Err(PreviewError::Malformed));
    }

    #[test]
    fn hostile_and_unsupported_inputs_are_refused_before_decoding() {
        let dir = std::env::temp_dir().join(format!("shinatama-preview-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("bomb.png"), png(100_000, 100_000)).unwrap();
        fs::write(dir.join("ok.png"), png(4, 4)).unwrap();
        fs::write(dir.join("zero.png"), png(0, 4)).unwrap();
        fs::write(dir.join("big.gif"), vec![0u8; (MAX_IMAGE_BYTES + 1) as usize]).unwrap();
        fs::write(dir.join("report.docx"), b"PK\x03\x04").unwrap();
        let root = Root::open(&dir).unwrap();
        assert_eq!(image(&root, "bomb.png"), Err(PreviewError::TooManyPixels { width: 100_000, height: 100_000 }));
        assert_eq!(image(&root, "zero.png"), Err(PreviewError::Malformed));
        assert_eq!(image(&root, "big.gif"), Err(PreviewError::TooLarge { bytes: MAX_IMAGE_BYTES + 1 }));
        assert!(matches!(image(&root, "report.docx"), Err(PreviewError::ReaderUnavailable { .. })));
        assert!(matches!(image(&root, "../x.png"), Err(PreviewError::Navigation { .. })));
        let ok = image(&root, "ok.png").unwrap();
        assert!(ok.data_url.starts_with("data:image/png;base64,iVBORw0KGgo"));
        assert_eq!((ok.width, ok.height), (4, 4));
        fs::remove_dir_all(dir).unwrap();
    }
}
