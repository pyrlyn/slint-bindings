//! Slint's item renderer and glyph renderer over a [`Canvas`]: logical
//! lengths become device pixels, and Slint's brushes, images and paths become
//! the crate's own.

use std::pin::Pin;

use i_slint_core::graphics::{self as slint, FitResult, IntRect, SharedImageBuffer, euclid};
use i_slint_core::item_rendering::{
    BorderRectLayout, CachedRenderingData, ItemRenderer, RenderBorderRectangle, RenderImage,
    RenderRectangle, RenderText,
};
use i_slint_core::item_tree::ItemRc;
use i_slint_core::items::{self, BoxShadow, ImageRendering, ImageTiling, Path, TextInput};
use i_slint_core::lengths::{
    LogicalBorderRadius, LogicalPoint, LogicalRect, LogicalSize, LogicalVector, PhysicalPx,
    ScaleFactor,
};
use i_slint_core::textlayout::sharedparley::{
    self, GlyphRenderer, PhysicalLength, PhysicalRect, RectangleBorder, TextLayoutCache, fontique,
    parley,
};
use i_slint_core::window::WindowInner;

use crate::canvas::{Canvas, GlyphRun, ImageStyle, Paint};
use crate::types::{
    Brush, Color, FillRule, GradientStop, Image, ImageFit, LineCap, LineJoin, PathPlacement,
    Pixels, Rect, Stroke, Tiling,
};

/// The paint a glyph run is drawn with; strokes rasterize outlines.
#[derive(Clone, Copy)]
pub(crate) struct TextBrush {
    paint: Paint,
    stroke: Option<f32>,
}

pub(crate) struct Frame<'c, 'a> {
    canvas: &'c mut Canvas<'a>,
    window: &'c WindowInner,
    text_cache: &'c TextLayoutCache,
    scale: f32,
}

impl<'c, 'a> Frame<'c, 'a> {
    pub fn new(
        canvas: &'c mut Canvas<'a>,
        window: &'c WindowInner,
        text_cache: &'c TextLayoutCache,
        scale: f32,
    ) -> Self {
        Self {
            canvas,
            window,
            text_cache,
            scale,
        }
    }

    fn logical_size(&self, size: LogicalSize) -> [f32; 2] {
        [size.width * self.scale, size.height * self.scale]
    }

    /// `size` is the device-pixel box gradients are laid out in.
    fn paint(&mut self, brush: &slint::Brush, size: [f32; 2]) -> Option<Paint> {
        if brush.is_transparent() {
            return None;
        }
        let brush = brush_of(brush, size, self.scale);
        self.canvas.paint(&brush)
    }

    fn draw_fit(
        &mut self,
        image: &slint::ImageInner,
        fit: &FitResult,
        source_size: slint::IntSize,
        rendering: ImageRendering,
        colorize: Option<Paint>,
        tiling: (ImageTiling, ImageTiling),
    ) {
        // Vector images render at the size they are shown, then are sampled
        // as if they had their intrinsic size.
        let texture_size = if image.is_svg() {
            let w = (source_size.width as f32 * fit.source_to_target_x).ceil() as u32;
            let h = (source_size.height as f32 * fit.source_to_target_y).ceil() as u32;
            [w.max(1), h.max(1)]
        } else {
            [source_size.width, source_size.height]
        };
        let key = slint::ImageCacheKey::new(image).and_then(|key| match key {
            slint::ImageCacheKey::Invalid => None,
            key => Some(hash_of(&key)),
        });
        let size = euclid::Size2D::new(texture_size[0], texture_size[1]);
        self.canvas.draw_image(
            &Image {
                key,
                size: [source_size.width, source_size.height],
                texture_size,
            },
            &[fit_of(fit, tiling)],
            ImageStyle {
                pixelated: rendering == ImageRendering::Pixelated,
                colorize,
            },
            || image.render_to_buffer(Some(size)).map(|b| pixels_of(&b)),
        );
    }
}

fn rect_of(rect: euclid::Rect<f32, PhysicalPx>) -> Rect {
    [rect.min_x(), rect.min_y(), rect.max_x(), rect.max_y()]
}
fn color_of(color: slint::Color) -> Color {
    Color::rgba(color.red(), color.green(), color.blue(), color.alpha())
}

fn stops_of<'a>(stops: impl Iterator<Item = &'a slint::GradientStop>) -> Vec<GradientStop> {
    stops
        .map(|stop| GradientStop {
            color: color_of(stop.color),
            position: stop.position,
        })
        .collect()
}

/// Resolves Slint's angle and default centres against a `size` box.
fn brush_of(brush: &slint::Brush, size: [f32; 2], scale: f32) -> Brush {
    match brush {
        slint::Brush::SolidColor(color) => Brush::Solid(color_of(*color)),
        slint::Brush::LinearGradient(gradient) => {
            let (start, end) =
                slint::line_for_angle(gradient.angle(), euclid::Size2D::new(size[0], size[1]));
            Brush::Linear {
                start: [start.x, start.y],
                end: [end.x, end.y],
                stops: stops_of(gradient.stops()),
            }
        }
        slint::Brush::RadialGradient(gradient) => {
            let (cx, cy) = gradient.center_or_default_scaled(size[0], size[1], scale);
            Brush::Radial {
                center: [cx, cy],
                radius: gradient.radius_or_default_scaled(size[0], size[1], scale),
                stops: stops_of(gradient.stops()),
            }
        }
        slint::Brush::ConicGradient(gradient) => {
            let (cx, cy) = gradient.center_or_default_scaled(size[0], size[1], scale);
            Brush::Conic {
                center: [cx, cy],
                stops: stops_of(gradient.stops()),
            }
        }
        _ => Brush::Solid(color_of(brush.color())),
    }
}

fn fit_of(fit: &FitResult, tiling: (ImageTiling, ImageTiling)) -> ImageFit {
    let clip = fit.clip_rect;
    ImageFit {
        offset: [fit.offset.x, fit.offset.y],
        size: [fit.size.width, fit.size.height],
        source_clip: [
            clip.origin.x as f32,
            clip.origin.y as f32,
            clip.size.width as f32,
            clip.size.height as f32,
        ],
        source_to_target: [fit.source_to_target_x, fit.source_to_target_y],
        tiling: fit.tiled.map(|tiled| Tiling {
            offset: [tiled.x as f32, tiled.y as f32],
            repeat_x: tiling.0 != ImageTiling::None,
            repeat_y: tiling.1 != ImageTiling::None,
        }),
    }
}

/// Slint's image identity as the canvas's `u64` key; a collision would need
/// two live images out of 2^64.
fn hash_of(key: &slint::ImageCacheKey) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

fn pixels_of(buffer: &SharedImageBuffer) -> Pixels {
    match buffer {
        SharedImageBuffer::RGB8(pixels) => Pixels {
            width: pixels.width(),
            height: pixels.height(),
            data: pixels
                .as_slice()
                .iter()
                .flat_map(|p| [p.r, p.g, p.b, 255])
                .collect(),
        },
        SharedImageBuffer::RGBA8(pixels) => {
            Pixels::from_straight_rgba(pixels.width(), pixels.height(), pixels.as_bytes())
        }
        SharedImageBuffer::RGBA8Premultiplied(pixels) => Pixels {
            width: pixels.width(),
            height: pixels.height(),
            data: pixels.as_bytes().to_vec(),
        },
    }
}

fn radii_of(radius: LogicalBorderRadius, scale: f32) -> [f32; 4] {
    [
        radius.top_left * scale,
        radius.top_right * scale,
        radius.bottom_right * scale,
        radius.bottom_left * scale,
    ]
}

impl ItemRenderer for Frame<'_, '_> {
    fn draw_rectangle(
        &mut self,
        rect: Pin<&dyn RenderRectangle>,
        _self_rc: &ItemRc,
        size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
        let [w, h] = self.logical_size(size);
        if let Some(paint) = self.paint(&rect.background(), [w, h]) {
            self.canvas.fill_rect([0.0, 0.0, w, h], [0.0; 4], paint);
        }
    }

    fn draw_border_rectangle(
        &mut self,
        rect: Pin<&dyn RenderBorderRectangle>,
        _self_rc: &ItemRc,
        size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
        let Some(layout) = BorderRectLayout::new(rect, size, ScaleFactor::new(self.scale)) else {
            return;
        };
        let brush_size = [layout.brush_size.width, layout.brush_size.height];
        if let Some(paint) = self.paint(&rect.background(), brush_size) {
            let r = layout.background_radius;
            self.canvas.fill_rect(
                rect_of(layout.background_rect),
                [r.top_left, r.top_right, r.bottom_right, r.bottom_left],
                paint,
            );
        }
        let width = layout.border_width.get();
        if width > 0.0
            && let Some(paint) = self.paint(&layout.border_color, brush_size)
        {
            let r = layout.border_radius;
            self.canvas.stroke_rect(
                rect_of(layout.border_rect),
                [r.top_left, r.top_right, r.bottom_right, r.bottom_left],
                width,
                paint,
            );
        }
    }

    fn draw_window_background(
        &mut self,
        rect: Pin<&dyn RenderRectangle>,
        self_rc: &ItemRc,
        size: LogicalSize,
        cache: &CachedRenderingData,
    ) {
        self.draw_rectangle(rect, self_rc, size, cache);
    }

    fn draw_image(
        &mut self,
        image: Pin<&dyn RenderImage>,
        _self_rc: &ItemRc,
        size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
        let source = image.source();
        let inner: &slint::ImageInner = (&source).into();
        let target = euclid::Size2D::new(size.width * self.scale, size.height * self.scale);
        let scale = ScaleFactor::new(self.scale);
        let alignment = image.alignment();
        let tiling = image.tiling();
        let rendering = image.rendering();
        let colorize = self.paint(&image.colorize(), [target.width, target.height]);
        if let slint::ImageInner::NineSlice(nine) = inner {
            let base = &nine.0;
            let source_size = base.size();
            for fit in slint::fit9slice(source_size, nine.1, target, scale, alignment, tiling) {
                self.draw_fit(base, &fit, source_size, rendering, colorize, tiling);
            }
            return;
        }
        let source_size = inner.size();
        let clip = image
            .source_clip()
            .unwrap_or_else(|| IntRect::from_size(source_size.cast()));
        let fit = slint::fit(image.image_fit(), target, clip, scale, alignment, tiling);
        self.draw_fit(inner, &fit, source_size, rendering, colorize, tiling);
    }

    fn draw_text(
        &mut self,
        text: Pin<&dyn RenderText>,
        self_rc: &ItemRc,
        size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
        let cache = self.text_cache;
        sharedparley::draw_text(self, text, Some(self_rc), size, Some(cache));
    }

    fn draw_text_input(
        &mut self,
        text_input: Pin<&TextInput>,
        self_rc: &ItemRc,
        size: LogicalSize,
    ) {
        let cache = self.text_cache;
        sharedparley::draw_text_input(self, text_input, self_rc, size, cache);
    }

    fn draw_path(&mut self, path: Pin<&Path>, self_rc: &ItemRc, size: LogicalSize) {
        let Some((offset, events)) = path.fitted_path_events(self_rc) else {
            return;
        };
        let events: Vec<_> = events.iter().collect();
        let brush_size = self.logical_size(size);
        let placement = PathPlacement {
            offset: [offset.x, offset.y],
            scale: self.scale,
        };
        if let Some(paint) = self.paint(&path.fill(), brush_size) {
            let rule = match path.fill_rule() {
                items::FillRule::Evenodd => FillRule::EvenOdd,
                _ => FillRule::NonZero,
            };
            self.canvas.fill_path(&events, placement, rule, paint);
        }
        let width = path.stroke_width().get();
        if width > 0.0
            && let Some(paint) = self.paint(&path.stroke(), brush_size)
        {
            let cap = match path.stroke_line_cap() {
                items::LineCap::Round => LineCap::Round,
                items::LineCap::Square => LineCap::Square,
                _ => LineCap::Butt,
            };
            let join = match path.stroke_line_join() {
                items::LineJoin::Round => LineJoin::Round,
                items::LineJoin::Bevel => LineJoin::Bevel,
                _ => LineJoin::Miter,
            };
            let stroke = Stroke { width, cap, join };
            self.canvas.stroke_path(&events, placement, stroke, paint);
        }
    }

    fn draw_box_shadow(
        &mut self,
        box_shadow: Pin<&BoxShadow>,
        _self_rc: &ItemRc,
        size: LogicalSize,
    ) {
        // Inset shadows are not drawn yet.
        if box_shadow.inset() {
            return;
        }
        let color = box_shadow.color();
        if color.alpha() == 0 {
            return;
        }
        let s = self.scale;
        let spread = box_shadow.spread().get() * s;
        let x = box_shadow.offset_x().get() * s - spread;
        let y = box_shadow.offset_y().get() * s - spread;
        let w = size.width * s + 2.0 * spread;
        let h = size.height * s + 2.0 * spread;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let radius = box_shadow.border_top_left_radius().get().max(0.0) * s + spread.max(0.0);
        let Some(paint) = self.canvas.paint(&Brush::Solid(color_of(color))) else {
            return;
        };
        let blur = box_shadow.blur().get() * s;
        self.canvas
            .box_shadow([x, y, x + w, y + h], radius, blur, paint);
    }

    fn combine_clip(&mut self, rect: LogicalRect, radius: LogicalBorderRadius) -> bool {
        let s = self.scale;
        let (x, y) = (rect.origin.x * s, rect.origin.y * s);
        let device = [x, y, x + rect.size.width * s, y + rect.size.height * s];
        let radii = if radius.is_zero() {
            [0.0; 4]
        } else {
            radii_of(radius, s)
        };
        self.canvas.clip(device, radii)
    }

    fn get_current_clip(&self) -> LogicalRect {
        let [x0, y0, x1, y1] = self.canvas.clip_bounds();
        let [ox, oy] = self.canvas.origin();
        let s = self.scale;
        LogicalRect::new(
            LogicalPoint::new((x0 - ox) / s, (y0 - oy) / s),
            LogicalSize::new(((x1 - x0) / s).max(0.0), ((y1 - y0) / s).max(0.0)),
        )
    }

    fn translate(&mut self, distance: LogicalVector) {
        self.canvas
            .translate(distance.x * self.scale, distance.y * self.scale);
    }

    // `supports_transformations` is false, so Slint never asks for these.
    fn rotate(&mut self, _angle_in_degrees: f32) {}

    fn scale(&mut self, _scale_x_factor: f32, _scale_y_factor: f32) {}

    fn apply_opacity(&mut self, opacity: f32) {
        self.canvas.apply_opacity(opacity);
    }

    fn global_alpha_transparent(&self) -> bool {
        self.canvas.opacity() <= 0.0
    }

    fn save_state(&mut self) {
        self.canvas.save();
    }

    fn restore_state(&mut self) {
        self.canvas.restore();
    }

    fn scale_factor(&self) -> ScaleFactor {
        ScaleFactor::new(self.scale)
    }

    fn draw_cached_pixmap(
        &mut self,
        _item_cache: &ItemRc,
        update_fn: &dyn Fn(&mut dyn FnMut(u32, u32, &[u8])),
    ) {
        let mut pixmap = None;
        update_fn(&mut |width, height, data| {
            pixmap = Some(Pixels {
                width,
                height,
                data: data.to_vec(),
            });
        });
        let Some(pixmap) = pixmap else {
            return;
        };
        let size = [pixmap.width as f32, pixmap.height as f32];
        let fit = ImageFit {
            size,
            source_clip: [0.0, 0.0, size[0], size[1]],
            source_to_target: [1.0, 1.0],
            ..ImageFit::default()
        };
        let image = Image {
            key: None,
            size: [pixmap.width, pixmap.height],
            texture_size: [pixmap.width, pixmap.height],
        };
        let style = ImageStyle {
            pixelated: true,
            colorize: None,
        };
        self.canvas
            .draw_image(&image, &[fit], style, || Some(pixmap));
    }

    /// Only Slint's performance overlay draws strings this way.
    fn draw_string(&mut self, _string: &str, _color: slint::Color) {}

    fn draw_image_direct(&mut self, image: slint::Image) {
        let inner: &slint::ImageInner = (&image).into();
        let size = inner.size();
        let fit = FitResult {
            clip_rect: IntRect::from_size(size.cast()),
            source_to_target_x: 1.0,
            source_to_target_y: 1.0,
            size: euclid::Size2D::new(size.width as f32, size.height as f32),
            offset: euclid::Point2D::default(),
            tiled: None,
        };
        self.draw_fit(
            inner,
            &fit,
            size,
            ImageRendering::Smooth,
            None,
            (ImageTiling::None, ImageTiling::None),
        );
    }

    fn window(&self) -> &WindowInner {
        self.window
    }

    fn as_any(&mut self) -> Option<&mut dyn core::any::Any> {
        None
    }
}

impl GlyphRenderer for Frame<'_, '_> {
    type PlatformBrush = TextBrush;

    fn platform_text_fill_brush(
        &mut self,
        brush: slint::Brush,
        size: LogicalSize,
    ) -> Option<TextBrush> {
        let size = self.logical_size(size);
        self.paint(&brush, size).map(|paint| TextBrush {
            paint,
            stroke: None,
        })
    }

    fn platform_brush_for_color(&mut self, color: &slint::Color) -> Option<TextBrush> {
        self.paint(&slint::Brush::SolidColor(*color), [0.0, 0.0])
            .map(|paint| TextBrush {
                paint,
                stroke: None,
            })
    }

    fn platform_text_stroke_brush(
        &mut self,
        brush: slint::Brush,
        physical_stroke_width: f32,
        size: LogicalSize,
    ) -> Option<TextBrush> {
        let size = self.logical_size(size);
        self.paint(&brush, size).map(|paint| TextBrush {
            paint,
            stroke: Some(physical_stroke_width),
        })
    }

    fn draw_glyph_run(
        &mut self,
        font: &parley::FontData,
        font_size: PhysicalLength,
        normalized_coords: &[i16],
        synthesis: &fontique::Synthesis,
        brush: TextBrush,
        y_offset: PhysicalLength,
        glyphs_it: &mut dyn Iterator<Item = parley::layout::Glyph>,
    ) {
        let run = GlyphRun {
            font,
            font_size: font_size.get(),
            normalized_coords,
            synthesis,
            stroke: brush.stroke,
        };
        self.canvas
            .draw_glyph_run(&run, brush.paint, [0.0, y_offset.get()], glyphs_it);
    }

    fn fill_rectangle(
        &mut self,
        physical_rect: PhysicalRect,
        brush: TextBrush,
        radius: PhysicalLength,
        border: Option<RectangleBorder<TextBrush>>,
    ) {
        let rect = rect_of(physical_rect);
        let r = radius.get();
        self.canvas.fill_rect(rect, [r; 4], brush.paint);
        if let Some(border) = border
            && border.width.get() > 0.0
        {
            self.canvas
                .stroke_rect(rect, [r; 4], border.width.get(), border.brush.paint);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use i_slint_core::graphics::{Rgba8Pixel, SharedPixelBuffer};

    #[test]
    fn straight_alpha_images_are_premultiplied() {
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(1, 1);
        buffer.make_mut_slice()[0] = Rgba8Pixel {
            r: 255,
            g: 128,
            b: 0,
            a: 128,
        };
        let pixels = pixels_of(&SharedImageBuffer::RGBA8(buffer));
        assert_eq!(pixels.data, [128, 64, 0, 128]);
    }
}
