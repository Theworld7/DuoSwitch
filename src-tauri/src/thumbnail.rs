//! 壁纸缩略图：解码 → 缩小 → JPEG → data URL。
//!
//! 不让 webview 直接读文件（那要放开 asset protocol 的路径白名单），
//! 改由 Rust 侧解码后回一个小尺寸 data URL，前端当普通图片用。

use std::io::Cursor;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::ExtendedColorType;

/// 缩略图目标宽度。界面里卡片宽约 380px，2x 下 320 宽足够清晰。
const THUMB_WIDTH: u32 = 320;
const JPEG_QUALITY: u8 = 72;

pub fn data_url(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }

    let source = image::open(path).ok()?;
    let (width, height) = (source.width(), source.height());
    if width == 0 || height == 0 {
        return None;
    }

    // 只缩不放，避免把小图放大成糊的
    let scale = (THUMB_WIDTH as f32 / width as f32).min(1.0);
    let target_width = ((width as f32 * scale).round() as u32).max(1);
    let target_height = ((height as f32 * scale).round() as u32).max(1);

    let thumb = source
        .resize_exact(target_width, target_height, FilterType::Triangle)
        .to_rgb8();

    let mut buffer = Vec::new();
    JpegEncoder::new_with_quality(&mut Cursor::new(&mut buffer), JPEG_QUALITY)
        .encode(&thumb, target_width, target_height, ExtendedColorType::Rgb8)
        .ok()?;

    Some(format!("data:image/jpeg;base64,{}", encode_base64(&buffer)))
}

const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// 标准 base64（RFC 4648，带填充）。自己写省一个依赖，逻辑够短且可测。
pub fn encode_base64(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);

    for chunk in input.chunks(3) {
        let first = u32::from(chunk[0]);
        let second = chunk.get(1).copied().map_or(0, u32::from);
        let third = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (first << 16) | (second << 8) | third;

        out.push(char::from(ALPHABET[((triple >> 18) & 0x3F) as usize]));
        out.push(char::from(ALPHABET[((triple >> 12) & 0x3F) as usize]));
        if chunk.len() > 1 {
            out.push(char::from(ALPHABET[((triple >> 6) & 0x3F) as usize]));
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(char::from(ALPHABET[(triple & 0x3F) as usize]));
        } else {
            out.push('=');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64(b"foob"), "Zm9vYg==");
        assert_eq!(encode_base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode_base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_handles_high_bytes() {
        assert_eq!(encode_base64(&[0xFF, 0xFE, 0xFD]), "//79");
        assert_eq!(encode_base64(&[0x00, 0x00, 0x00]), "AAAA");
    }

    #[test]
    fn missing_file_yields_none() {
        assert!(data_url(Path::new(r"C:\definitely\not\here.jpg")).is_none());
    }
}
