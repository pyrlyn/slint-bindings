//! What callers hand a [`Canvas`](crate::Canvas): colours, brushes, images and
//! path styles. Lengths are device pixels unless a type says otherwise.

/// A device-pixel rectangle as `[x0, y0, x1, y1]`.
pub type Rect = [f32; 4];

/// Corner radii in device pixels: top-left, top-right, bottom-right,
/// bottom-left.
pub type Radii = [f32; 4];

/// An sRGB colour with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub(crate) fn premultiplied(self) -> [f32; 4] {
        let a = f32::from(self.a) / 255.0;
        [
            f32::from(self.r) / 255.0 * a,
            f32::from(self.g) / 255.0 * a,
            f32::from(self.b) / 255.0 * a,
            a,
        ]
    }
}

/// A gradient colour at `position`, 0 at the gradient's start and 1 at its
/// end. Stops are expected in ascending order.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GradientStop {
    pub color: Color,
    pub position: f32,
}

/// How a shape is painted. Gradient geometry is in device pixels, relative to
/// the canvas origin at the time the brush becomes a [`Paint`](crate::Paint).
#[derive(Clone, Debug, PartialEq)]
pub enum Brush {
    Solid(Color),
    Linear {
        start: [f32; 2],
        end: [f32; 2],
        stops: Vec<GradientStop>,
    },
    Radial {
        center: [f32; 2],
        radius: f32,
        stops: Vec<GradientStop>,
    },
    /// Sweeps clockwise from 12 o'clock around `center`.
    Conic {
        center: [f32; 2],
        stops: Vec<GradientStop>,
    },
}

impl Default for Brush {
    fn default() -> Self {
        Self::Solid(Color::TRANSPARENT)
    }
}

impl From<Color> for Brush {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

impl Brush {
    /// A transparent solid colour paints nothing. Gradients always count as
    /// painting, even when every stop is transparent.
    pub fn is_transparent(&self) -> bool {
        matches!(self, Self::Solid(color) if color.a == 0)
    }
}

/// Premultiplied RGBA8 pixels in tightly packed rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Pixels {
    /// Premultiplies straight-alpha RGBA8 pixels.
    pub fn from_straight_rgba(width: u32, height: u32, rgba: &[u8]) -> Self {
        let (pixels, _) = rgba.as_chunks::<4>();
        let data = pixels
            .iter()
            .flat_map(|&[r, g, b, a]| {
                let alpha = u16::from(a);
                let m = |c: u8| ((u16::from(c) * alpha + 127) / 255) as u8;
                [m(r), m(g), m(b), a]
            })
            .collect();
        Self {
            width,
            height,
            data,
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        u64::from(self.width) * u64::from(self.height) * 4 == self.data.len() as u64
    }
}

/// An image a canvas draws. Its pixels are asked for only when no texture for
/// `key` and `texture_size` is cached yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Image {
    /// The caller's identity for the image across frames; `None` uploads it
    /// on every draw.
    pub key: Option<u64>,
    /// The image's own size in source pixels; [`ImageFit`] is in these units.
    pub size: [u32; 2],
    /// The size of the pixels that get uploaded. A vector image renders at the
    /// size it is shown and is still sampled as if it had `size`.
    pub texture_size: [u32; 2],
}

/// Where one piece of an image lands. A nine-slice image is nine of these.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImageFit {
    /// Top-left of the drawn image, relative to the canvas origin.
    pub offset: [f32; 2],
    /// Size of the drawn image.
    pub size: [f32; 2],
    /// The source pixels shown, as `[x, y, width, height]`.
    pub source_clip: [f32; 4],
    /// Device pixels per source pixel, per axis.
    pub source_to_target: [f32; 2],
    /// Repeats the clipped source instead of stretching it.
    pub tiling: Option<Tiling>,
}

/// How an image repeats inside its fit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tiling {
    /// Where the first tile starts, in source pixels.
    pub offset: [f32; 2],
    pub repeat_x: bool,
    pub repeat_y: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// A path in its own units. A point `p` lands at
/// `origin + (p + offset) * scale` in device pixels.
#[derive(Clone, Copy, Debug)]
pub struct PathPlacement {
    /// Added to every point, in path units.
    pub offset: [f32; 2],
    /// Device pixels per path unit.
    pub scale: f32,
}

/// The outline of a stroked path. `width` is in path units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_alpha_is_premultiplied() {
        let pixels = Pixels::from_straight_rgba(1, 1, &[255, 128, 0, 128]);
        assert_eq!(pixels.data, [128, 64, 0, 128]);
        assert!(pixels.is_valid());
    }

    #[test]
    fn pixels_of_the_wrong_length_are_invalid() {
        let pixels = Pixels {
            width: 2,
            height: 2,
            data: vec![0; 12],
        };
        assert!(!pixels.is_valid());
    }
}
