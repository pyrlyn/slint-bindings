//! The canvas on its own crate's types, with no Slint involved: each test
//! draws a small frame and checks pixels well inside or well outside shapes,
//! where antialiasing does not reach.
//!
//! Without a GPU adapter the tests pass without drawing, like the comparison
//! tests.

use std::cell::Cell;
use std::error::Error;

use slintx_wgpu::path::Path;
use slintx_wgpu::{
    Brush, Canvas, Color, FillRule, GradientStop, Image, ImageFit, ImageStyle, PathPlacement,
    Pixels, Renderer,
};

type TestResult = Result<(), Box<dyn Error>>;

const SIZE: [u32; 2] = [40, 40];
const RED: Color = Color::rgba(255, 0, 0, 255);
const CLEAR: [u8; 4] = [0; 4];

fn renderer() -> Option<Renderer> {
    match Renderer::headless() {
        Ok(renderer) => Some(renderer),
        Err(err) => {
            eprintln!("skipping: {err}");
            None
        }
    }
}

/// Straight-alpha RGBA8 of a `SIZE` frame, or `None` without a GPU.
fn frame(draw: impl FnOnce(&mut Canvas<'_>)) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
    let Some(renderer) = renderer() else {
        return Ok(None);
    };
    Ok(Some(renderer.render_to_pixels(SIZE, draw)?))
}

fn at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE[0] + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

#[test]
fn rectangles_fill_and_stroke() -> TestResult {
    let Some(pixels) = frame(|canvas| {
        let red = canvas.paint(&RED.into());
        let blue = canvas.paint(&Color::rgba(0, 0, 255, 255).into());
        if let (Some(red), Some(blue)) = (red, blue) {
            canvas.fill_rect([4.0, 4.0, 16.0, 16.0], [0.0; 4], red);
            canvas.stroke_rect([20.0, 20.0, 36.0, 36.0], [0.0; 4], 4.0, blue);
        }
    })?
    else {
        return Ok(());
    };
    assert_eq!(at(&pixels, 10, 10), [255, 0, 0, 255]);
    assert_eq!(at(&pixels, 20, 28), [0, 0, 255, 255], "on the outline");
    assert_eq!(at(&pixels, 28, 28), CLEAR, "inside the ring");
    assert_eq!(at(&pixels, 2, 2), CLEAR);
    Ok(())
}

#[test]
fn transparent_brushes_paint_nothing() -> TestResult {
    let Some(pixels) = frame(|canvas| {
        assert!(canvas.paint(&Brush::default()).is_none());
    })?
    else {
        return Ok(());
    };
    assert!(pixels.iter().all(|&c| c == 0));
    Ok(())
}

#[test]
fn clip_translate_and_opacity_nest() -> TestResult {
    let Some(pixels) = frame(|canvas| {
        let Some(red) = canvas.paint(&RED.into()) else {
            return;
        };
        canvas.save();
        canvas.translate(10.0, 0.0);
        assert!(canvas.clip([0.0, 0.0, 10.0, 40.0], [0.0; 4]));
        canvas.apply_opacity(0.5);
        canvas.fill_rect([-10.0, 0.0, 30.0, 20.0], [0.0; 4], red);
        canvas.restore();
        canvas.fill_rect([0.0, 30.0, 40.0, 40.0], [0.0; 4], red);
    })?
    else {
        return Ok(());
    };
    let half = at(&pixels, 15, 10);
    assert_eq!(&half[..3], &[255, 0, 0]);
    assert!((half[3] as i32 - 128).abs() <= 1, "half opacity: {half:?}");
    assert_eq!(at(&pixels, 5, 10), CLEAR, "left of the clip");
    assert_eq!(at(&pixels, 25, 10), CLEAR, "right of the clip");
    assert_eq!(at(&pixels, 30, 35), [255, 0, 0, 255], "restored state");
    Ok(())
}

#[test]
fn linear_gradients_follow_their_line() -> TestResult {
    let gradient = Brush::Linear {
        start: [0.0, 0.0],
        end: [40.0, 0.0],
        stops: vec![
            GradientStop {
                color: Color::rgba(0, 0, 0, 255),
                position: 0.0,
            },
            GradientStop {
                color: Color::rgba(255, 255, 255, 255),
                position: 1.0,
            },
        ],
    };
    let Some(pixels) = frame(|canvas| {
        if let Some(paint) = canvas.paint(&gradient) {
            canvas.fill_rect([0.0, 0.0, 40.0, 40.0], [0.0; 4], paint);
        }
    })?
    else {
        return Ok(());
    };
    let (left, middle, right) = (at(&pixels, 2, 20), at(&pixels, 20, 20), at(&pixels, 37, 20));
    assert!(left[0] < 30 && right[0] > 225, "{left:?} .. {right:?}");
    assert!((middle[0] as i32 - 130).abs() <= 8, "{middle:?}");
    Ok(())
}

#[test]
fn images_are_cached_by_key() -> TestResult {
    let Some(renderer) = renderer() else {
        return Ok(());
    };
    // A 2×2 checker shown at 20×20: each source pixel covers a 10×10 block.
    let image = Image {
        key: Some(7),
        size: [2, 2],
        texture_size: [2, 2],
    };
    let fit = ImageFit {
        offset: [0.0, 0.0],
        size: [20.0, 20.0],
        source_clip: [0.0, 0.0, 2.0, 2.0],
        source_to_target: [10.0, 10.0],
        tiling: None,
    };
    let decoded = Cell::new(0);
    let decode = || {
        decoded.set(decoded.get() + 1);
        #[rustfmt::skip]
        let rgba = [
            255, 0, 0, 255,   0, 255, 0, 255,
            0, 0, 255, 255,   255, 255, 255, 128,
        ];
        Some(Pixels::from_straight_rgba(2, 2, &rgba))
    };
    let style = ImageStyle {
        pixelated: true,
        colorize: None,
    };
    let mut frames = Vec::new();
    for _ in 0..2 {
        frames.push(renderer.render_to_pixels(SIZE, |canvas| {
            canvas.draw_image(&image, &[fit], style, decode);
        })?);
    }
    assert_eq!(decoded.get(), 1, "the second frame reuses the texture");
    let pixels = &frames[1];
    assert_eq!(at(pixels, 5, 5), [255, 0, 0, 255]);
    assert_eq!(at(pixels, 15, 5), [0, 255, 0, 255]);
    assert_eq!(at(pixels, 5, 15), [0, 0, 255, 255]);
    assert_eq!(at(pixels, 15, 15), [255, 255, 255, 128]);
    assert_eq!(at(pixels, 30, 30), CLEAR);
    Ok(())
}

#[test]
fn images_with_the_wrong_pixel_count_are_skipped() -> TestResult {
    let image = Image {
        key: None,
        size: [2, 2],
        texture_size: [2, 2],
    };
    let fit = ImageFit {
        size: [20.0, 20.0],
        source_clip: [0.0, 0.0, 2.0, 2.0],
        source_to_target: [10.0, 10.0],
        ..ImageFit::default()
    };
    let Some(pixels) = frame(|canvas| {
        canvas.draw_image(&image, &[fit], ImageStyle::default(), || {
            Some(Pixels {
                width: 2,
                height: 2,
                data: vec![255; 4],
            })
        });
    })?
    else {
        return Ok(());
    };
    assert!(pixels.iter().all(|&c| c == 0));
    Ok(())
}

#[test]
fn paths_fill_in_their_own_units() -> TestResult {
    // A right triangle in units of 10 device pixels.
    let mut builder = Path::builder();
    builder.begin(lyon_point(0.0, 0.0));
    builder.line_to(lyon_point(3.0, 0.0));
    builder.line_to(lyon_point(0.0, 3.0));
    builder.close();
    let path = builder.build();
    let events: Vec<_> = path.iter().collect();
    let Some(pixels) = frame(|canvas| {
        if let Some(red) = canvas.paint(&RED.into()) {
            let placement = PathPlacement {
                offset: [0.5, 0.5],
                scale: 10.0,
            };
            canvas.fill_path(&events, placement, FillRule::NonZero, red);
        }
    })?
    else {
        return Ok(());
    };
    assert_eq!(at(&pixels, 10, 10), [255, 0, 0, 255], "inside");
    assert_eq!(at(&pixels, 30, 30), CLEAR, "past the hypotenuse");
    assert_eq!(at(&pixels, 2, 2), CLEAR, "before the offset");
    Ok(())
}

fn lyon_point(x: f32, y: f32) -> slintx_wgpu::path::math::Point {
    slintx_wgpu::path::math::point(x, y)
}
