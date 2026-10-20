//! Turns one frame of Slint items into the scene's display list.

use std::pin::Pin;
use std::rc::Rc;

use i_slint_core::graphics::{
    Brush, Color, FitResult, Image, ImageInner, IntRect, IntSize, euclid,
};
use i_slint_core::item_rendering::{
    BorderRectLayout, CachedRenderingData, ItemRenderer, RenderBorderRectangle, RenderImage,
    RenderRectangle, RenderText,
};
use i_slint_core::item_tree::ItemRc;
use i_slint_core::items::{
    BoxShadow, FillRule, ImageRendering, ImageTiling, LineCap, LineJoin, Path, TextInput,
};
use i_slint_core::lengths::{
    LogicalBorderRadius, LogicalPoint, LogicalRect, LogicalSize, LogicalVector, PhysicalPx,
    ScaleFactor,
};
use i_slint_core::textlayout::sharedparley::{
    self, GlyphRenderer, PhysicalLength, PhysicalRect, RectangleBorder, TextLayoutCache, fontique,
    parley,
};
use i_slint_core::window::WindowInner;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers,
};

use crate::glyphs::{GlyphCache, RunStyle};
use crate::images::{GpuImage, ImageCache, Uploader};
use crate::scene::{
    self, FLAG_COLORIZE, FLAG_TILE_X, FLAG_TILE_Y, GpuClip, KIND_FILL, KIND_GLYPH, KIND_IMAGE,
    KIND_RING, KIND_SHADOW, KIND_SOLID, NO_CLIP, Prim, Rect, Scene,
};

/// Device-pixel slack around analytic shapes, so their antialiased edge is
/// inside the quad.
const AA: f32 = 1.0;

#[derive(Clone, Copy)]
struct State {
    /// Item origin in device pixels.
    origin: [f32; 2],
    /// Hard clip, device pixels.
    clip: Rect,
    /// The latest rounded clip; children of nested rounded clips get the
    /// innermost one only.
    rounded: Rect,
    radii: [f32; 4],
    clip_index: Option<u32>,
    opacity: f32,
}

/// The brush a glyph run is painted with; strokes rasterize outlines.
#[derive(Clone, Copy)]
pub(crate) struct TextBrush {
    brush: u32,
    stroke: Option<f32>,
}

pub(crate) struct Frame<'a> {
    pub scene: &'a mut Scene,
    pub window: &'a WindowInner,
    pub glyphs: &'a mut GlyphCache,
    pub images: &'a mut ImageCache,
    pub uploader: Uploader<'a>,
    pub text_cache: &'a TextLayoutCache,
    pub scale: f32,
    state: State,
    stack: Vec<State>,
}

impl<'a> Frame<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        scene: &'a mut Scene,
        window: &'a WindowInner,
        glyphs: &'a mut GlyphCache,
        images: &'a mut ImageCache,
        uploader: Uploader<'a>,
        text_cache: &'a TextLayoutCache,
        scale: f32,
        size: [f32; 2],
    ) -> Self {
        Self {
            scene,
            window,
            glyphs,
            images,
            uploader,
            text_cache,
            scale,
            state: State {
                origin: [0.0, 0.0],
                clip: [0.0, 0.0, size[0], size[1]],
                rounded: NO_CLIP,
                radii: [0.0; 4],
                clip_index: None,
                opacity: 1.0,
            },
            stack: Vec::new(),
        }
    }

    fn clip_index(&mut self) -> u32 {
        if let Some(index) = self.state.clip_index {
            return index;
        }
        let index = self.scene.push_clip(GpuClip {
            rect: self.state.clip,
            rrect: self.state.rounded,
            radii: self.state.radii,
        });
        self.state.clip_index = Some(index);
        index
    }

    /// A rectangle relative to the item origin, in device pixels.
    fn device(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        let [ox, oy] = self.state.origin;
        [ox + x, oy + y, ox + x + w, oy + y + h]
    }

    fn logical_size(&self, size: LogicalSize) -> [f32; 2] {
        [size.width * self.scale, size.height * self.scale]
    }

    fn brush(&mut self, brush: &Brush, size: [f32; 2]) -> Option<u32> {
        let origin = self.state.origin;
        self.scene.push_brush(brush, origin, size, self.scale)
    }

    /// Records a primitive and its quad, cut to the clip; skips it when none
    /// of it is visible.
    fn shape(&mut self, bounds: Rect, mut prim: Prim, image: Option<(Rc<GpuImage>, bool)>) {
        let visible = scene::intersect(bounds, scene::inflate(self.state.clip, AA));
        if scene::is_empty(visible) || self.state.opacity <= 0.0 {
            return;
        }
        prim.clip = self.clip_index();
        prim.opacity = self.state.opacity;
        let index = self.scene.push_prim(prim);
        self.scene.push_quad(visible, [0.0; 4], index, image);
    }

    fn fill_rounded(&mut self, rect: Rect, radii: [f32; 4], brush: u32) {
        self.shape(
            scene::inflate(rect, AA),
            Prim {
                rect,
                radii,
                kind: KIND_FILL,
                brush,
                ..Prim::default()
            },
            None,
        );
    }

    /// A stroke of `width` centred on the outline of `rect`.
    fn ring(&mut self, rect: Rect, radii: [f32; 4], width: f32, brush: u32) {
        let half = width / 2.0;
        let outer = scene::inflate(rect, half);
        let inner = scene::inflate(rect, -half);
        let grow = |r: f32, by: f32| if r > 0.0 { (r + by).max(0.0) } else { 0.0 };
        self.shape(
            scene::inflate(outer, AA),
            Prim {
                rect: outer,
                radii: radii.map(|r| grow(r, half)),
                a: inner,
                b: radii.map(|r| grow(r, -half)),
                kind: KIND_RING,
                brush,
                ..Prim::default()
            },
            None,
        );
    }

    fn draw_fit(
        &mut self,
        image: &ImageInner,
        fit: &FitResult,
        source_size: IntSize,
        rendering: ImageRendering,
        colorize: Option<u32>,
        tiling: (ImageTiling, ImageTiling),
    ) {
        if fit.clip_rect.is_empty() || source_size.is_empty() {
            return;
        }
        // Vector images render at the size they are shown, then are sampled
        // as if they had their intrinsic size.
        let render_size = if image.is_svg() {
            let w = (source_size.width as f32 * fit.source_to_target_x).ceil() as u32;
            let h = (source_size.height as f32 * fit.source_to_target_y).ceil() as u32;
            [w.max(1), h.max(1)]
        } else {
            [source_size.width, source_size.height]
        };
        let key = i_slint_core::graphics::ImageCacheKey::new(image)
            .unwrap_or(i_slint_core::graphics::ImageCacheKey::Invalid);
        let size = euclid::Size2D::new(render_size[0], render_size[1]);
        let Some(texture) = self.images.get(
            &self.uploader,
            key,
            || image.render_to_buffer(Some(size)),
            render_size,
        ) else {
            return;
        };
        let origin = [
            self.state.origin[0] + fit.offset.x,
            self.state.origin[1] + fit.offset.y,
        ];
        let target = [
            origin[0],
            origin[1],
            origin[0] + fit.size.width,
            origin[1] + fit.size.height,
        ];
        let tiled = fit.tiled.unwrap_or_default();
        let mut flags = 0;
        if fit.tiled.is_some() {
            if tiling.0 != ImageTiling::None {
                flags |= FLAG_TILE_X;
            }
            if tiling.1 != ImageTiling::None {
                flags |= FLAG_TILE_Y;
            }
        }
        let brush = match colorize {
            Some(brush) => {
                flags |= FLAG_COLORIZE;
                brush
            }
            None => 0,
        };
        let clip = fit.clip_rect;
        self.shape(
            target,
            Prim {
                rect: target,
                a: [
                    origin[0],
                    origin[1],
                    1.0 / fit.source_to_target_x,
                    1.0 / fit.source_to_target_y,
                ],
                b: [
                    clip.origin.x as f32,
                    clip.origin.y as f32,
                    clip.size.width as f32,
                    clip.size.height as f32,
                ],
                c: [
                    tiled.x as f32,
                    tiled.y as f32,
                    source_size.width as f32,
                    source_size.height as f32,
                ],
                kind: KIND_IMAGE,
                brush,
                flags,
                ..Prim::default()
            },
            Some((texture, rendering == ImageRendering::Pixelated)),
        );
    }

    fn tessellate(&mut self, path: Pin<&Path>, self_rc: &ItemRc, size: LogicalSize) {
        let Some((offset, events)) = path.fitted_path_events(self_rc) else {
            return;
        };
        let events: Vec<_> = events.iter().collect();
        let brush_size = self.logical_size(size);
        let scale = self.scale;
        let [ox, oy] = self.state.origin;
        let to_device = move |p: lyon_tessellation::math::Point| {
            [ox + (p.x + offset.x) * scale, oy + (p.y + offset.y) * scale]
        };
        // Logical units; the tolerance is a tenth of a device pixel.
        let tolerance = 0.1 / scale.max(0.01);

        let fill = path.fill();
        if let Some(brush) = self.brush(&fill, brush_size) {
            let rule = match path.fill_rule() {
                FillRule::Evenodd => lyon_tessellation::FillRule::EvenOdd,
                _ => lyon_tessellation::FillRule::NonZero,
            };
            let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
            let options = FillOptions::tolerance(tolerance).with_fill_rule(rule);
            let built = FillTessellator::new().tessellate(
                events.iter().copied(),
                &options,
                &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| to_device(v.position())),
            );
            if built.is_ok() {
                self.triangles(&buffers, brush);
            }
        }

        let stroke = path.stroke();
        let width = path.stroke_width().get();
        if width > 0.0
            && let Some(brush) = self.brush(&stroke, brush_size)
        {
            let cap = match path.stroke_line_cap() {
                LineCap::Round => lyon_tessellation::LineCap::Round,
                LineCap::Square => lyon_tessellation::LineCap::Square,
                _ => lyon_tessellation::LineCap::Butt,
            };
            let join = match path.stroke_line_join() {
                LineJoin::Round => lyon_tessellation::LineJoin::Round,
                LineJoin::Bevel => lyon_tessellation::LineJoin::Bevel,
                _ => lyon_tessellation::LineJoin::Miter,
            };
            let options = StrokeOptions::tolerance(tolerance)
                .with_line_width(width)
                .with_line_cap(cap)
                .with_line_join(join);
            let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
            let built = StrokeTessellator::new().tessellate(
                events.iter().copied(),
                &options,
                &mut BuffersBuilder::new(&mut buffers, |v: StrokeVertex| to_device(v.position())),
            );
            if built.is_ok() {
                self.triangles(&buffers, brush);
            }
        }
    }

    fn triangles(&mut self, buffers: &VertexBuffers<[f32; 2], u32>, brush: u32) {
        if buffers.indices.is_empty() || self.state.opacity <= 0.0 {
            return;
        }
        let prim = Prim {
            kind: KIND_SOLID,
            brush,
            clip: self.clip_index(),
            opacity: self.state.opacity,
            ..Prim::default()
        };
        let index = self.scene.push_prim(prim);
        let vertices = &buffers.vertices;
        self.scene.push_triangles(
            buffers
                .indices
                .iter()
                .filter_map(|&i| vertices.get(i as usize).copied()),
            index,
        );
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

fn rect_of(rect: euclid::Rect<f32, PhysicalPx>, origin: [f32; 2]) -> Rect {
    [
        origin[0] + rect.min_x(),
        origin[1] + rect.min_y(),
        origin[0] + rect.max_x(),
        origin[1] + rect.max_y(),
    ]
}

impl ItemRenderer for Frame<'_> {
    fn draw_rectangle(
        &mut self,
        rect: Pin<&dyn RenderRectangle>,
        _self_rc: &ItemRc,
        size: LogicalSize,
        _cache: &CachedRenderingData,
    ) {
        let [w, h] = self.logical_size(size);
        if let Some(brush) = self.brush(&rect.background(), [w, h]) {
            let device = self.device(0.0, 0.0, w, h);
            self.fill_rounded(device, [0.0; 4], brush);
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
        let origin = self.state.origin;
        if let Some(brush) = self.brush(&rect.background(), brush_size) {
            let r = layout.background_radius;
            self.fill_rounded(
                rect_of(layout.background_rect, origin),
                [r.top_left, r.top_right, r.bottom_right, r.bottom_left],
                brush,
            );
        }
        let width = layout.border_width.get();
        if width > 0.0
            && let Some(brush) = self.brush(&layout.border_color, brush_size)
        {
            let r = layout.border_radius;
            self.ring(
                rect_of(layout.border_rect, origin),
                [r.top_left, r.top_right, r.bottom_right, r.bottom_left],
                width,
                brush,
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
        let inner: &ImageInner = (&source).into();
        let target = euclid::Size2D::new(size.width * self.scale, size.height * self.scale);
        let scale = ScaleFactor::new(self.scale);
        let alignment = image.alignment();
        let tiling = image.tiling();
        let rendering = image.rendering();
        let colorize = image.colorize();
        let colorize = self.brush(&colorize, [target.width, target.height]);
        if let ImageInner::NineSlice(nine) = inner {
            let base = &nine.0;
            let source_size = base.size();
            let fits = i_slint_core::graphics::fit9slice(
                source_size,
                nine.1,
                target,
                scale,
                alignment,
                tiling,
            );
            for fit in fits {
                self.draw_fit(base, &fit, source_size, rendering, colorize, tiling);
            }
            return;
        }
        let source_size = inner.size();
        let clip = image
            .source_clip()
            .unwrap_or_else(|| IntRect::from_size(source_size.cast()));
        let fit =
            i_slint_core::graphics::fit(image.image_fit(), target, clip, scale, alignment, tiling);
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
        self.tessellate(path, self_rc, size);
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
        let rect = self.device(x, y, w, h);
        // The blur is a radius, as in CSS, so the Gaussian's sigma is half of it.
        let sigma = box_shadow.blur().get() * s / 2.0;
        let radius = box_shadow.border_top_left_radius().get().max(0.0) * s + spread.max(0.0);
        let Some(brush) = self.brush(&Brush::SolidColor(color), [w, h]) else {
            return;
        };
        self.shape(
            scene::inflate(rect, sigma * 3.0 + AA),
            Prim {
                rect,
                radii: [radius; 4],
                a: [sigma, 0.0, 0.0, 0.0],
                kind: KIND_SHADOW,
                brush,
                ..Prim::default()
            },
            None,
        );
    }

    fn combine_clip(&mut self, rect: LogicalRect, radius: LogicalBorderRadius) -> bool {
        let s = self.scale;
        let device = self.device(
            rect.origin.x * s,
            rect.origin.y * s,
            rect.size.width * s,
            rect.size.height * s,
        );
        self.state.clip = scene::intersect(self.state.clip, device);
        if !radius.is_zero() {
            self.state.rounded = device;
            self.state.radii = radii_of(radius, s);
        }
        self.state.clip_index = None;
        !scene::is_empty(self.state.clip)
    }

    fn get_current_clip(&self) -> LogicalRect {
        let [x0, y0, x1, y1] = self.state.clip;
        let [ox, oy] = self.state.origin;
        let s = self.scale;
        LogicalRect::new(
            LogicalPoint::new((x0 - ox) / s, (y0 - oy) / s),
            LogicalSize::new(((x1 - x0) / s).max(0.0), ((y1 - y0) / s).max(0.0)),
        )
    }

    fn translate(&mut self, distance: LogicalVector) {
        self.state.origin[0] += distance.x * self.scale;
        self.state.origin[1] += distance.y * self.scale;
    }

    // `supports_transformations` is false, so Slint never asks for these.
    fn rotate(&mut self, _angle_in_degrees: f32) {}

    fn scale(&mut self, _scale_x_factor: f32, _scale_y_factor: f32) {}

    fn apply_opacity(&mut self, opacity: f32) {
        self.state.opacity *= opacity.clamp(0.0, 1.0);
    }

    fn global_alpha_transparent(&self) -> bool {
        self.state.opacity <= 0.0
    }

    fn save_state(&mut self) {
        self.stack.push(self.state);
    }

    fn restore_state(&mut self) {
        if let Some(state) = self.stack.pop() {
            self.state = state;
        }
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
            pixmap = Some((width, height, data.to_vec()));
        });
        let Some((width, height, data)) = pixmap else {
            return;
        };
        if width == 0 || height == 0 {
            return;
        }
        let texture = self.uploader.upload(width, height, &data);
        let rect = self.device(0.0, 0.0, width as f32, height as f32);
        self.shape(
            rect,
            Prim {
                rect,
                a: [rect[0], rect[1], 1.0, 1.0],
                b: [0.0, 0.0, width as f32, height as f32],
                c: [0.0, 0.0, width as f32, height as f32],
                kind: KIND_IMAGE,
                ..Prim::default()
            },
            Some((texture, true)),
        );
    }

    /// Only Slint's performance overlay draws strings this way.
    fn draw_string(&mut self, _string: &str, _color: Color) {}

    fn draw_image_direct(&mut self, image: Image) {
        let inner: &ImageInner = (&image).into();
        let size = inner.size();
        if size.is_empty() {
            return;
        }
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

impl GlyphRenderer for Frame<'_> {
    type PlatformBrush = TextBrush;

    fn platform_text_fill_brush(&mut self, brush: Brush, size: LogicalSize) -> Option<TextBrush> {
        let size = self.logical_size(size);
        self.brush(&brush, size).map(|brush| TextBrush {
            brush,
            stroke: None,
        })
    }

    fn platform_brush_for_color(&mut self, color: &Color) -> Option<TextBrush> {
        self.brush(&Brush::SolidColor(*color), [0.0, 0.0])
            .map(|brush| TextBrush {
                brush,
                stroke: None,
            })
    }

    fn platform_text_stroke_brush(
        &mut self,
        brush: Brush,
        physical_stroke_width: f32,
        size: LogicalSize,
    ) -> Option<TextBrush> {
        let size = self.logical_size(size);
        self.brush(&brush, size).map(|brush| TextBrush {
            brush,
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
        if self.state.opacity <= 0.0 {
            return;
        }
        let prim = Prim {
            kind: KIND_GLYPH,
            brush: brush.brush,
            clip: self.clip_index(),
            opacity: self.state.opacity,
            ..Prim::default()
        };
        let index = self.scene.push_prim(prim);
        let style = RunStyle {
            font,
            size: font_size.get(),
            coords: normalized_coords,
            synthesis,
            stroke: brush.stroke,
        };
        let clip = scene::inflate(self.state.clip, AA);
        let [ox, oy] = self.state.origin;
        for glyph in glyphs_it {
            let (x, subpixel) = GlyphCache::snap(ox + glyph.x);
            let y = (oy + glyph.y + y_offset.get()).round();
            let Some(entry) = self.glyphs.get(&style, glyph.id, subpixel) else {
                continue;
            };
            let left = x + entry.left as f32;
            let top = y - entry.top as f32;
            let bounds = [
                left,
                top,
                left + entry.width as f32,
                top + entry.height as f32,
            ];
            if scene::is_empty(scene::intersect(bounds, clip)) {
                continue;
            }
            let uv = [
                entry.x as f32,
                entry.y as f32,
                (entry.x + entry.width) as f32,
                (entry.y + entry.height) as f32,
            ];
            self.scene.push_quad(bounds, uv, index, None);
        }
    }

    fn fill_rectangle(
        &mut self,
        physical_rect: PhysicalRect,
        brush: TextBrush,
        radius: PhysicalLength,
        border: Option<RectangleBorder<TextBrush>>,
    ) {
        let rect = rect_of(physical_rect, self.state.origin);
        let r = radius.get();
        self.fill_rounded(rect, [r; 4], brush.brush);
        if let Some(border) = border
            && border.width.get() > 0.0
        {
            self.ring(rect, [r; 4], border.width.get(), border.brush.brush);
        }
    }
}
