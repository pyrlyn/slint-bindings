//! Records one frame of shapes, images, paths and glyphs into the scene.

use std::rc::Rc;

use lyon_tessellation::path::PathEvent;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
    StrokeVertex, VertexBuffers,
};
use parley::fontique;

use crate::glyphs::{GlyphCache, RunStyle};
use crate::images::{GpuImage, ImageCache, Uploader};
use crate::scene::{
    self, FLAG_COLORIZE, FLAG_TILE_X, FLAG_TILE_Y, GpuClip, KIND_FILL, KIND_GLYPH, KIND_IMAGE,
    KIND_RING, KIND_SHADOW, KIND_SOLID, NO_CLIP, Prim, Scene,
};
use crate::types::{
    Brush, FillRule, Image, ImageFit, LineCap, LineJoin, PathPlacement, Pixels, Radii, Rect, Stroke,
};

/// Device-pixel slack around analytic shapes, so their antialiased edge is
/// inside the quad.
const AA: f32 = 1.0;

/// A brush recorded into the frame. Its gradient geometry was fixed against
/// the origin current when it was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paint(u32);

/// How an image is sampled and tinted.
#[derive(Clone, Copy, Debug, Default)]
pub struct ImageStyle {
    /// Nearest-neighbour sampling instead of linear.
    pub pixelated: bool,
    /// Keeps the image's alpha and paints it with this instead of its colours.
    pub colorize: Option<Paint>,
}

/// One run of glyphs in one font, as parley shapes it.
pub struct GlyphRun<'a> {
    pub font: &'a parley::FontData,
    /// Font size in device pixels.
    pub font_size: f32,
    pub normalized_coords: &'a [i16],
    pub synthesis: &'a fontique::Synthesis,
    /// Paints the glyph outlines this wide instead of filling them.
    pub stroke: Option<f32>,
}

#[derive(Clone, Copy)]
struct State {
    /// Device pixels.
    origin: [f32; 2],
    /// Hard clip, device pixels.
    clip: Rect,
    /// The latest rounded clip; children of nested rounded clips get the
    /// innermost one only.
    rounded: Rect,
    radii: Radii,
    clip_index: Option<u32>,
    opacity: f32,
}

/// Draws one frame. Coordinates are device pixels relative to the current
/// origin; [`save`](Self::save) and [`restore`](Self::restore) bracket
/// changes to the origin, clip and opacity.
pub struct Canvas<'a> {
    scene: &'a mut Scene,
    glyphs: &'a mut GlyphCache,
    images: &'a mut ImageCache,
    uploader: Uploader<'a>,
    state: State,
    stack: Vec<State>,
}

impl<'a> Canvas<'a> {
    pub(crate) fn new(
        scene: &'a mut Scene,
        glyphs: &'a mut GlyphCache,
        images: &'a mut ImageCache,
        uploader: Uploader<'a>,
        size: [f32; 2],
    ) -> Self {
        Self {
            scene,
            glyphs,
            images,
            uploader,
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

    pub fn save(&mut self) {
        self.stack.push(self.state);
    }

    /// Does nothing without a matching [`save`](Self::save).
    pub fn restore(&mut self) {
        if let Some(state) = self.stack.pop() {
            self.state = state;
        }
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.state.origin[0] += dx;
        self.state.origin[1] += dy;
    }

    /// The current origin in target pixels.
    pub fn origin(&self) -> [f32; 2] {
        self.state.origin
    }

    /// Narrows the clip to `rect`, rounded by `radii`. Returns whether any of
    /// the clip is left. A rounded clip replaces the previous rounded clip
    /// instead of intersecting it.
    pub fn clip(&mut self, rect: Rect, radii: Radii) -> bool {
        let device = self.device(rect);
        self.state.clip = scene::intersect(self.state.clip, device);
        if radii.iter().any(|&r| r != 0.0) {
            self.state.rounded = device;
            self.state.radii = radii;
        }
        self.state.clip_index = None;
        !scene::is_empty(self.state.clip)
    }

    /// The rectangular part of the clip, in target pixels.
    pub fn clip_bounds(&self) -> Rect {
        self.state.clip
    }

    /// Multiplies the opacity of everything drawn until the next
    /// [`restore`](Self::restore).
    pub fn apply_opacity(&mut self, opacity: f32) {
        self.state.opacity *= opacity.clamp(0.0, 1.0);
    }

    pub fn opacity(&self) -> f32 {
        self.state.opacity
    }

    /// Records `brush` against the current origin; `None` when it paints
    /// nothing.
    pub fn paint(&mut self, brush: &Brush) -> Option<Paint> {
        self.scene.push_brush(brush, self.state.origin).map(Paint)
    }

    pub fn fill_rect(&mut self, rect: Rect, radii: Radii, paint: Paint) {
        let rect = self.device(rect);
        self.shape(
            scene::inflate(rect, AA),
            Prim {
                rect,
                radii,
                kind: KIND_FILL,
                brush: paint.0,
                ..Prim::default()
            },
            None,
        );
    }

    /// A stroke `width` wide, centred on the outline of `rect`.
    pub fn stroke_rect(&mut self, rect: Rect, radii: Radii, width: f32, paint: Paint) {
        let rect = self.device(rect);
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
                brush: paint.0,
                ..Prim::default()
            },
            None,
        );
    }

    /// A Gaussian shadow of a rounded rectangle. `blur` is a radius, as in
    /// CSS, so the Gaussian's sigma is half of it.
    pub fn box_shadow(&mut self, rect: Rect, radius: f32, blur: f32, paint: Paint) {
        let rect = self.device(rect);
        let sigma = blur / 2.0;
        self.shape(
            scene::inflate(rect, sigma * 3.0 + AA),
            Prim {
                rect,
                radii: [radius; 4],
                a: [sigma, 0.0, 0.0, 0.0],
                kind: KIND_SHADOW,
                brush: paint.0,
                ..Prim::default()
            },
            None,
        );
    }

    /// Draws `image` once per fit. `pixels` is called at most once, when no
    /// texture for the image's key and texture size is cached.
    pub fn draw_image(
        &mut self,
        image: &Image,
        fits: &[ImageFit],
        style: ImageStyle,
        pixels: impl FnOnce() -> Option<Pixels>,
    ) {
        let [width, height] = image.size;
        if width == 0 || height == 0 {
            return;
        }
        let visible = |fit: &&ImageFit| fit.source_clip[2] > 0.0 && fit.source_clip[3] > 0.0;
        if !fits.iter().any(|fit| visible(&fit)) {
            return;
        }
        let Some(texture) = self
            .images
            .get(&self.uploader, image.key, pixels, image.texture_size)
        else {
            return;
        };
        for fit in fits.iter().filter(visible) {
            self.draw_fit(&texture, fit, image.size, style);
        }
    }

    fn draw_fit(
        &mut self,
        texture: &Rc<GpuImage>,
        fit: &ImageFit,
        size: [u32; 2],
        style: ImageStyle,
    ) {
        let origin = [
            self.state.origin[0] + fit.offset[0],
            self.state.origin[1] + fit.offset[1],
        ];
        let target = [
            origin[0],
            origin[1],
            origin[0] + fit.size[0],
            origin[1] + fit.size[1],
        ];
        let tiled = fit.tiling.unwrap_or_default();
        let mut flags = 0;
        if tiled.repeat_x {
            flags |= FLAG_TILE_X;
        }
        if tiled.repeat_y {
            flags |= FLAG_TILE_Y;
        }
        let brush = match style.colorize {
            Some(paint) => {
                flags |= FLAG_COLORIZE;
                paint.0
            }
            None => 0,
        };
        self.shape(
            target,
            Prim {
                rect: target,
                a: [
                    origin[0],
                    origin[1],
                    1.0 / fit.source_to_target[0],
                    1.0 / fit.source_to_target[1],
                ],
                b: fit.source_clip,
                c: [
                    tiled.offset[0],
                    tiled.offset[1],
                    size[0] as f32,
                    size[1] as f32,
                ],
                kind: KIND_IMAGE,
                brush,
                flags,
                ..Prim::default()
            },
            Some((texture.clone(), style.pixelated)),
        );
    }

    pub fn fill_path(
        &mut self,
        events: &[PathEvent],
        placement: PathPlacement,
        rule: FillRule,
        paint: Paint,
    ) {
        let rule = match rule {
            FillRule::EvenOdd => lyon_tessellation::FillRule::EvenOdd,
            FillRule::NonZero => lyon_tessellation::FillRule::NonZero,
        };
        let to_device = self.path_to_device(placement);
        let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
        let options = FillOptions::tolerance(path_tolerance(placement)).with_fill_rule(rule);
        let built = FillTessellator::new().tessellate(
            events.iter().copied(),
            &options,
            &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| to_device(v.position())),
        );
        if built.is_ok() {
            self.triangles(&buffers, paint.0);
        }
    }

    pub fn stroke_path(
        &mut self,
        events: &[PathEvent],
        placement: PathPlacement,
        stroke: Stroke,
        paint: Paint,
    ) {
        if stroke.width <= 0.0 {
            return;
        }
        let cap = match stroke.cap {
            LineCap::Round => lyon_tessellation::LineCap::Round,
            LineCap::Square => lyon_tessellation::LineCap::Square,
            LineCap::Butt => lyon_tessellation::LineCap::Butt,
        };
        let join = match stroke.join {
            LineJoin::Round => lyon_tessellation::LineJoin::Round,
            LineJoin::Bevel => lyon_tessellation::LineJoin::Bevel,
            LineJoin::Miter => lyon_tessellation::LineJoin::Miter,
        };
        let to_device = self.path_to_device(placement);
        let options = StrokeOptions::tolerance(path_tolerance(placement))
            .with_line_width(stroke.width)
            .with_line_cap(cap)
            .with_line_join(join);
        let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
        let built = StrokeTessellator::new().tessellate(
            events.iter().copied(),
            &options,
            &mut BuffersBuilder::new(&mut buffers, |v: StrokeVertex| to_device(v.position())),
        );
        if built.is_ok() {
            self.triangles(&buffers, paint.0);
        }
    }

    /// Draws glyphs positioned by parley; each lands at
    /// `origin + glyph position + offset`, its pen x snapped to a quarter
    /// pixel and its baseline to a whole one.
    pub fn draw_glyph_run(
        &mut self,
        run: &GlyphRun<'_>,
        paint: Paint,
        offset: [f32; 2],
        glyphs: impl IntoIterator<Item = parley::layout::Glyph>,
    ) {
        if self.state.opacity <= 0.0 {
            return;
        }
        let prim = Prim {
            kind: KIND_GLYPH,
            brush: paint.0,
            clip: self.clip_index(),
            opacity: self.state.opacity,
            ..Prim::default()
        };
        let index = self.scene.push_prim(prim);
        let style = RunStyle {
            font: run.font,
            size: run.font_size,
            coords: run.normalized_coords,
            synthesis: run.synthesis,
            stroke: run.stroke,
        };
        let clip = scene::inflate(self.state.clip, AA);
        let [ox, oy] = self.state.origin;
        for glyph in glyphs {
            let (x, subpixel) = GlyphCache::snap(ox + glyph.x + offset[0]);
            let y = (oy + glyph.y + offset[1]).round();
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

    /// Draws the glyphs of a parley layout `offset` from the origin, each run
    /// with its style's brush. Lay it out with the device scale as parley's
    /// display scale. Decorations, selection and inline boxes are the
    /// caller's to draw.
    pub fn draw_layout(&mut self, layout: &parley::Layout<Brush>, offset: [f32; 2]) {
        for line in layout.lines() {
            for item in line.items() {
                let parley::PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let Some(paint) = self.paint(&glyph_run.style().brush) else {
                    continue;
                };
                let run = glyph_run.run();
                let synthesis = run.synthesis();
                let desc = GlyphRun {
                    font: run.font(),
                    font_size: run.font_size(),
                    normalized_coords: run.normalized_coords(),
                    synthesis: &synthesis,
                    stroke: None,
                };
                self.draw_glyph_run(&desc, paint, offset, glyph_run.positioned_glyphs());
            }
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

    fn device(&self, [x0, y0, x1, y1]: Rect) -> Rect {
        let [ox, oy] = self.state.origin;
        [ox + x0, oy + y0, ox + x1, oy + y1]
    }

    fn path_to_device(
        &self,
        placement: PathPlacement,
    ) -> impl Fn(lyon_tessellation::math::Point) -> [f32; 2] + use<> {
        let [ox, oy] = self.state.origin;
        let PathPlacement {
            offset: [dx, dy],
            scale,
        } = placement;
        move |p| [ox + (p.x + dx) * scale, oy + (p.y + dy) * scale]
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

/// A tenth of a device pixel, in path units.
fn path_tolerance(placement: PathPlacement) -> f32 {
    0.1 / placement.scale.max(0.01)
}
