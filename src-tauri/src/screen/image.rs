//! Preparing a screenshot for a vision model: shrink it so the long edge is at
//! most [`MAX_LONG_EDGE`] (about what Claude works at; more only adds cost and
//! latency), then keep whichever of JPEG or PNG is smaller. Photos and video
//! come out smaller as JPEG; text-heavy windows usually as PNG, which also
//! keeps text crisp.

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{ExtendedColorType, ImageEncoder, RgbImage};

pub const MAX_LONG_EDGE: u32 = 1568;
const JPEG_QUALITY: u8 = 85;

/// An encoded screenshot, ready to send.
pub struct PreparedImage {
    /// "image/png" or "image/jpeg".
    pub media_type: &'static str,
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Size after shrinking to fit `max` on the long edge; never enlarges.
pub fn fitted_size(width: u32, height: u32, max: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= max {
        return (width, height);
    }
    let scale = max as f64 / long as f64;
    let fit = |v: u32| ((v as f64 * scale).round() as u32).clamp(1, max);
    (fit(width), fit(height))
}

fn encode_png(image: &RgbImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    Ok(out)
}

fn encode_jpeg(image: &RgbImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|e| format!("JPEG encoding failed: {e}"))?;
    Ok(out)
}

pub fn prepare(image: &RgbImage) -> Result<PreparedImage, String> {
    let (width, height) = fitted_size(image.width(), image.height(), MAX_LONG_EDGE);
    let resized;
    let image = if (width, height) == image.dimensions() {
        image
    } else {
        // Catmull-Rom keeps small text legible when shrinking.
        resized = image::imageops::resize(image, width, height, FilterType::CatmullRom);
        &resized
    };

    let png = encode_png(image)?;
    let jpeg = encode_jpeg(image)?;
    let (media_type, data) = if png.len() <= jpeg.len() {
        ("image/png", png)
    } else {
        ("image/jpeg", jpeg)
    };
    Ok(PreparedImage {
        media_type,
        data,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn long_edge_is_capped_keeping_the_aspect_ratio() {
        assert_eq!(fitted_size(3840, 2160, 1568), (1568, 882));
        assert_eq!(fitted_size(1080, 1920, 1568), (882, 1568));
        assert_eq!(fitted_size(1568, 400, 1568), (1568, 400));
        // Small windows are never enlarged.
        assert_eq!(fitted_size(800, 600, 1568), (800, 600));
        // Extreme shapes keep at least one pixel.
        assert_eq!(fitted_size(20000, 3, 1568), (1568, 1));
    }

    /// Black text-like strokes on white, like a document window.
    fn document(width: u32, height: u32) -> RgbImage {
        RgbImage::from_fn(width, height, |x, y| {
            if y % 24 < 3 && x % 9 < 6 {
                Rgb([20, 20, 20])
            } else {
                Rgb([255, 255, 255])
            }
        })
    }

    /// Smooth gradients with noise, like a photo.
    fn photo(width: u32, height: u32) -> RgbImage {
        let mut seed = 12345u32;
        RgbImage::from_fn(width, height, |x, y| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let noise = (seed >> 24) as u8 / 8;
            Rgb([
                (x * 255 / width) as u8 ^ noise,
                (y * 255 / height) as u8 ^ noise,
                ((x + y) % 256) as u8 ^ noise,
            ])
        })
    }

    #[test]
    fn large_screenshots_are_downscaled() {
        let prepared = prepare(&document(2560, 1440)).unwrap();
        assert_eq!((prepared.width, prepared.height), (1568, 882));
        let decoded = image::load_from_memory(&prepared.data).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1568, 882));
    }

    #[test]
    fn text_windows_become_png_and_photos_jpeg() {
        let text = prepare(&document(1200, 800)).unwrap();
        assert_eq!(text.media_type, "image/png");
        assert!(text.data.starts_with(b"\x89PNG"));

        let photo = prepare(&photo(1200, 800)).unwrap();
        assert_eq!(photo.media_type, "image/jpeg");
        assert!(photo.data.starts_with(&[0xFF, 0xD8]));
    }

    #[test]
    fn the_smaller_encoding_wins() {
        for image in [document(640, 480), photo(640, 480)] {
            let prepared = prepare(&image).unwrap();
            let png = encode_png(&image).unwrap().len();
            let jpeg = encode_jpeg(&image).unwrap().len();
            assert_eq!(prepared.data.len(), png.min(jpeg));
        }
    }
}
