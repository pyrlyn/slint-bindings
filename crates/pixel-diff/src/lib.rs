//! RGBA8 frames for screenshot tests: PNG files and pixel comparison.
//!
//! Frames are tightly packed, row-major, 4 bytes per pixel.

/// Encodes a frame as an sRGB PNG.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, png::EncodingError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // The samples are already sRGB. The chunk stops a viewer from converting them.
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    writer.finish()?;
    Ok(out)
}

/// Decodes a PNG into `(width, height, rgba)`.
pub fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), png::DecodingError> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buf)?;
    buf.truncate(info.line_size * info.height as usize);
    Ok((info.width, info.height, buf))
}

/// The largest difference of any channel between two pixels.
pub fn channel_delta(a: [u8; 4], b: [u8; 4]) -> u8 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

/// Pixels whose largest channel difference is above `threshold`. Zero
/// counts every pixel that is not identical.
pub fn differing_pixels(left: &[u8], right: &[u8], threshold: u8) -> usize {
    let (left, _) = left.as_chunks::<4>();
    let (right, _) = right.as_chunks::<4>();
    left.iter()
        .zip(right)
        .filter(|(a, b)| channel_delta(**a, **b) > threshold)
        .count()
}

/// A frame that shows `left` faded, with pixels that differ past `threshold`
/// in opaque red.
pub fn diff_image(left: &[u8], right: &[u8], threshold: u8) -> Vec<u8> {
    let (left, _) = left.as_chunks::<4>();
    let (right, _) = right.as_chunks::<4>();
    left.iter()
        .zip(right)
        .flat_map(|(a, b)| {
            if channel_delta(*a, *b) > threshold {
                [255, 0, 0, 255]
            } else {
                let grey = ((u16::from(a[0]) + u16::from(a[1]) + u16::from(a[2])) / 3) as u8;
                let faded = 255 - (255 - grey) / 4;
                [faded, faded, faded, 255]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        let rgba = [1, 2, 3, 4, 250, 251, 252, 253];
        let png = encode_png(2, 1, &rgba)?;
        assert_eq!(decode_png(&png)?, (2, 1, rgba.to_vec()));
        Ok(())
    }

    #[test]
    fn threshold_ignores_small_differences() {
        let left = [10, 10, 10, 255, 10, 10, 10, 255];
        let right = [12, 10, 10, 255, 10, 40, 10, 255];
        assert_eq!(differing_pixels(&left, &right, 0), 2);
        assert_eq!(differing_pixels(&left, &right, 2), 1);
        assert_eq!(differing_pixels(&left, &right, 30), 0);
    }

    #[test]
    fn diff_marks_only_differing_pixels() {
        let left = [0, 0, 0, 255, 0, 0, 0, 255];
        let right = [0, 0, 0, 255, 200, 0, 0, 255];
        let diff = diff_image(&left, &right, 8);
        assert_ne!(&diff[..4], &[255, 0, 0, 255]);
        assert_eq!(&diff[4..], &[255, 0, 0, 255]);
    }
}
