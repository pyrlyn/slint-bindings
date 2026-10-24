//! The display list one frame records before anything touches the GPU.
//!
//! Layouts match `shader.wgsl` field for field.

use std::rc::Rc;

use bytemuck::{Pod, Zeroable};

use crate::images::GpuImage;
use crate::types::{Brush, Color, GradientStop, Rect};

pub(crate) const KIND_FILL: u32 = 0;
pub(crate) const KIND_RING: u32 = 1;
pub(crate) const KIND_SHADOW: u32 = 2;
pub(crate) const KIND_IMAGE: u32 = 3;
pub(crate) const KIND_GLYPH: u32 = 4;
pub(crate) const KIND_SOLID: u32 = 5;

pub(crate) const FLAG_TILE_X: u32 = 1;
pub(crate) const FLAG_TILE_Y: u32 = 2;
pub(crate) const FLAG_COLORIZE: u32 = 4;

const BRUSH_SOLID: u32 = 0;
const BRUSH_LINEAR: u32 = 1;
const BRUSH_RADIAL: u32 = 2;
const BRUSH_CONIC: u32 = 3;

/// Texels per gradient ramp row.
pub(crate) const RAMP_WIDTH: usize = 256;
/// Gradient rows one frame can hold; later gradients fall back to a solid color.
pub(crate) const RAMP_ROWS: usize = 256;

pub(crate) const NO_CLIP: Rect = [-1e6, -1e6, 1e6, 1e6];

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub prim: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct Prim {
    pub rect: [f32; 4],
    pub radii: [f32; 4],
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub c: [f32; 4],
    pub kind: u32,
    pub brush: u32,
    pub flags: u32,
    pub clip: u32,
    pub opacity: f32,
    pub _pad: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct GpuBrush {
    pub color: [f32; 4],
    pub geom: [f32; 4],
    pub kind: u32,
    pub ramp: u32,
    pub _pad: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub(crate) struct GpuClip {
    pub rect: [f32; 4],
    pub rrect: [f32; 4],
    pub radii: [f32; 4],
}

/// A run of vertices drawn with one image bind group (or none).
pub(crate) struct Batch {
    pub image: Option<(Rc<GpuImage>, bool)>,
    pub start: u32,
    pub end: u32,
}

#[derive(Default)]
pub(crate) struct Scene {
    pub vertices: Vec<Vertex>,
    pub prims: Vec<Prim>,
    pub brushes: Vec<GpuBrush>,
    pub clips: Vec<GpuClip>,
    /// Premultiplied RGBA8 rows, `RAMP_WIDTH` texels each.
    pub ramps: Vec<u8>,
    pub batches: Vec<Batch>,
}

impl Scene {
    pub fn new() -> Self {
        let mut scene = Self::default();
        scene.clips.push(GpuClip {
            rect: NO_CLIP,
            rrect: NO_CLIP,
            radii: [0.0; 4],
        });
        scene
    }

    pub fn push_clip(&mut self, clip: GpuClip) -> u32 {
        self.clips.push(clip);
        (self.clips.len() - 1) as u32
    }

    pub fn push_prim(&mut self, prim: Prim) -> u32 {
        self.prims.push(prim);
        (self.prims.len() - 1) as u32
    }

    /// Returns the brush index, or `None` for a brush that paints nothing.
    /// `origin` is the device-pixel point brush geometry is relative to.
    pub fn push_brush(&mut self, brush: &Brush, origin: [f32; 2]) -> Option<u32> {
        if brush.is_transparent() {
            return None;
        }
        let [ox, oy] = origin;
        let gpu = match brush {
            Brush::Solid(color) => solid(*color),
            Brush::Linear { start, end, stops } => self.gradient(
                BRUSH_LINEAR,
                [ox + start[0], oy + start[1], ox + end[0], oy + end[1]],
                stops,
            ),
            Brush::Radial {
                center,
                radius,
                stops,
            } => self.gradient(
                BRUSH_RADIAL,
                [ox + center[0], oy + center[1], *radius, 0.0],
                stops,
            ),
            Brush::Conic { center, stops } => self.gradient(
                BRUSH_CONIC,
                [ox + center[0], oy + center[1], 0.0, 0.0],
                stops,
            ),
        };
        self.brushes.push(gpu);
        Some((self.brushes.len() - 1) as u32)
    }

    fn gradient(&mut self, kind: u32, geom: [f32; 4], stops: &[GradientStop]) -> GpuBrush {
        let Some(first) = stops.first() else {
            return solid(Color::default());
        };
        let row = self.ramps.len() / (RAMP_WIDTH * 4);
        if row >= RAMP_ROWS {
            return solid(first.color);
        }
        self.ramps.extend(ramp_row(stops));
        GpuBrush {
            color: first.color.premultiplied(),
            geom,
            kind,
            ramp: row as u32,
            _pad: [0; 2],
        }
    }

    /// A quad covering `bounds`; the shader decides coverage inside it.
    pub fn push_quad(
        &mut self,
        bounds: Rect,
        uv: Rect,
        prim: u32,
        image: Option<(Rc<GpuImage>, bool)>,
    ) {
        let [x0, y0, x1, y1] = bounds;
        let [u0, v0, u1, v1] = uv;
        let corners = [
            ([x0, y0], [u0, v0]),
            ([x1, y0], [u1, v0]),
            ([x0, y1], [u0, v1]),
            ([x1, y0], [u1, v0]),
            ([x1, y1], [u1, v1]),
            ([x0, y1], [u0, v1]),
        ];
        self.begin_batch(image);
        self.vertices
            .extend(corners.iter().map(|&(pos, uv)| Vertex { pos, uv, prim }));
        self.end_batch();
    }

    pub fn push_triangles(&mut self, points: impl Iterator<Item = [f32; 2]>, prim: u32) {
        self.begin_batch(None);
        self.vertices.extend(points.map(|pos| Vertex {
            pos,
            uv: [0.0; 2],
            prim,
        }));
        self.end_batch();
    }

    fn begin_batch(&mut self, image: Option<(Rc<GpuImage>, bool)>) {
        let start = self.vertices.len() as u32;
        let same = match (self.batches.last(), &image) {
            (Some(last), None) => last.image.is_none(),
            (Some(last), Some((img, nearest))) => last
                .image
                .as_ref()
                .is_some_and(|(other, n)| Rc::ptr_eq(other, img) && n == nearest),
            (None, _) => false,
        };
        if !same {
            self.batches.push(Batch {
                image,
                start,
                end: start,
            });
        }
    }

    fn end_batch(&mut self) {
        let end = self.vertices.len() as u32;
        if let Some(last) = self.batches.last_mut() {
            last.end = end;
        }
    }
}

fn solid(color: Color) -> GpuBrush {
    GpuBrush {
        color: color.premultiplied(),
        kind: BRUSH_SOLID,
        ..GpuBrush::default()
    }
}

/// Interpolates in premultiplied space, so a fade to transparent does not
/// darken toward the transparent stop's (often black) color.
fn ramp_row(stops: &[GradientStop]) -> Vec<u8> {
    let colors: Vec<(f32, [f32; 4])> = stops
        .iter()
        .map(|stop| (stop.position, stop.color.premultiplied()))
        .collect();
    let mut row = Vec::with_capacity(RAMP_WIDTH * 4);
    for i in 0..RAMP_WIDTH {
        let t = i as f32 / (RAMP_WIDTH - 1) as f32;
        let color = sample_stops(&colors, t);
        row.extend(color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
    }
    row
}

fn sample_stops(stops: &[(f32, [f32; 4])], t: f32) -> [f32; 4] {
    let Some(&(first_pos, first)) = stops.first() else {
        return [0.0; 4];
    };
    if t <= first_pos {
        return first;
    }
    for pair in stops.windows(2) {
        let (p0, c0) = pair[0];
        let (p1, c1) = pair[1];
        if t <= p1 {
            let span = p1 - p0;
            let f = if span <= f32::EPSILON {
                1.0
            } else {
                (t - p0) / span
            };
            return std::array::from_fn(|i| c0[i] + (c1[i] - c0[i]) * f);
        }
    }
    stops.last().map_or(first, |&(_, c)| c)
}

pub(crate) fn intersect(a: Rect, b: Rect) -> Rect {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

pub(crate) fn is_empty(r: Rect) -> bool {
    r[2] <= r[0] || r[3] <= r[1]
}

pub(crate) fn inflate(r: Rect, by: f32) -> Rect {
    [r[0] - by, r[1] - by, r[2] + by, r[3] + by]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_interpolates_premultiplied() {
        let stops = [
            GradientStop {
                color: Color::rgba(255, 0, 0, 255),
                position: 0.0,
            },
            GradientStop {
                color: Color::TRANSPARENT,
                position: 1.0,
            },
        ];
        let row = ramp_row(&stops);
        assert_eq!(&row[..4], &[255, 0, 0, 255]);
        let mid = (RAMP_WIDTH / 2) * 4;
        // Half alpha, and red stays at full intensity once un-premultiplied.
        assert!((row[mid + 3] as i32 - 127).abs() <= 1);
        assert!((row[mid] as i32 - row[mid + 3] as i32).abs() <= 1);
        assert_eq!(&row[row.len() - 4..], &[0, 0, 0, 0]);
    }

    #[test]
    fn neighbouring_quads_share_a_batch() {
        let mut scene = Scene::new();
        scene.push_quad([0.0, 0.0, 1.0, 1.0], [0.0; 4], 0, None);
        scene.push_quad([1.0, 0.0, 2.0, 1.0], [0.0; 4], 0, None);
        assert_eq!(scene.batches.len(), 1);
        assert_eq!(scene.batches[0].end, 12);
    }
}
