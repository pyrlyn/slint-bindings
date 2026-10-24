//! A parley layout drawn through the canvas, with no Slint involved. Fonts
//! come from the system; without a GPU adapter or a system font the test
//! passes without drawing.

use std::error::Error;

use slintx_wgpu::parley::{self, FontContext, LayoutContext, StyleProperty};
use slintx_wgpu::{Brush, Color, Renderer};

const SIZE: [u32; 2] = [160, 48];
const OFFSET: [f32; 2] = [8.0, 6.0];

#[test]
fn layouts_draw_inside_their_bounds() -> Result<(), Box<dyn Error>> {
    let mut fonts = FontContext::new();
    let mut layouts = LayoutContext::<Brush>::new();
    let text = "Hamburgefonstiv";
    let mut builder = layouts.ranged_builder(&mut fonts, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(24.0));
    builder.push_default(StyleProperty::Brush(Brush::Solid(Color::rgba(
        255, 0, 0, 255,
    ))));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    let has_glyphs = layout
        .lines()
        .flat_map(|line| line.items())
        .any(|item| matches!(item, parley::PositionedLayoutItem::GlyphRun(_)));
    if !has_glyphs || layout.width() <= 0.0 {
        eprintln!("skipping: no system font");
        return Ok(());
    }
    let renderer = match Renderer::headless() {
        Ok(renderer) => renderer,
        Err(err) => {
            eprintln!("skipping: {err}");
            return Ok(());
        }
    };
    let pixels = renderer.render_to_pixels(SIZE, |canvas| canvas.draw_layout(&layout, OFFSET))?;

    // Ink may overhang the advance and the line box by a pixel or two.
    let bounds = [
        OFFSET[0] - 3.0,
        OFFSET[1] - 3.0,
        OFFSET[0] + layout.width() + 3.0,
        OFFSET[1] + layout.height() + 3.0,
    ];
    let mut solid = 0;
    for (i, px) in pixels.as_chunks::<4>().0.iter().enumerate() {
        let (x, y) = ((i as u32 % SIZE[0]) as f32, (i as u32 / SIZE[0]) as f32);
        let inside = x >= bounds[0] && y >= bounds[1] && x < bounds[2] && y < bounds[3];
        if !inside {
            assert_eq!(px[3], 0, "ink outside the layout at ({x}, {y})");
        }
        if px[3] == 255 {
            assert_eq!(&px[..3], &[255, 0, 0], "the run's brush at ({x}, {y})");
            solid += 1;
        }
    }
    assert!(solid > 50, "only {solid} fully covered pixels");
    Ok(())
}
