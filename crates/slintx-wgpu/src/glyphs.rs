//! Glyph masks rasterized by swash and packed into one R8 atlas.
//!
//! The atlas grows instead of evicting during a frame, because quads recorded
//! earlier in the frame point at fixed atlas pixels. A full atlas at the size
//! limit is cleared at the start of the next frame.

use std::collections::HashMap;

use parley::fontique;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Angle, Format, Stroke, Transform, Vector};

const START_SIZE: u32 = 1024;
const MAX_SIZE: u32 = 4096;
/// Horizontal glyph positions snap to quarter pixels.
const SUBPIXEL_STEPS: f32 = 4.0;
/// One pixel of empty border, so linear sampling never meets a neighbour.
const PADDING: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct GlyphKey {
    font: u64,
    index: u32,
    glyph: u32,
    size: u32,
    subpixel: u8,
    coords: u64,
    embolden: bool,
    skew: u32,
    stroke: u32,
}

/// Where a glyph sits in the atlas and how it is placed against the pen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GlyphEntry {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub left: i32,
    pub top: i32,
}

/// What one glyph run asks for besides the glyph id.
pub(crate) struct RunStyle<'a> {
    pub font: &'a parley::FontData,
    pub size: f32,
    pub coords: &'a [i16],
    pub synthesis: &'a fontique::Synthesis,
    pub stroke: Option<f32>,
}

struct Shelf {
    y: u32,
    height: u32,
    cursor: u32,
}

pub(crate) struct GlyphCache {
    context: ScaleContext,
    entries: HashMap<GlyphKey, Option<GlyphEntry>>,
    pub(crate) size: u32,
    pub(crate) pixels: Vec<u8>,
    shelves: Vec<Shelf>,
    next_y: u32,
    full: bool,
    /// Set when the CPU copy changed and the GPU texture needs it.
    pub(crate) dirty: bool,
}

impl GlyphCache {
    pub fn new() -> Self {
        Self {
            context: ScaleContext::new(),
            entries: HashMap::new(),
            size: START_SIZE,
            pixels: vec![0; (START_SIZE * START_SIZE) as usize],
            shelves: Vec::new(),
            next_y: 0,
            full: false,
            dirty: true,
        }
    }

    pub fn begin_frame(&mut self) {
        if self.full {
            self.entries.clear();
            self.shelves.clear();
            self.next_y = 0;
            self.pixels.fill(0);
            self.full = false;
            self.dirty = true;
        }
    }

    /// The pen x position split into a whole pixel and the snapped fraction.
    pub fn snap(x: f32) -> (f32, u8) {
        let whole = x.floor();
        let steps = ((x - whole) * SUBPIXEL_STEPS).round();
        if steps >= SUBPIXEL_STEPS {
            (whole + 1.0, 0)
        } else {
            (whole, steps as u8)
        }
    }

    /// `None` for glyphs with no pixels (spaces) or that do not fit.
    pub fn get(&mut self, style: &RunStyle<'_>, glyph: u32, subpixel: u8) -> Option<GlyphEntry> {
        let key = GlyphKey {
            font: style.font.data.id(),
            index: style.font.index,
            glyph,
            size: style.size.to_bits(),
            subpixel,
            coords: hash_coords(style.coords),
            embolden: style.synthesis.embolden(),
            skew: style.synthesis.skew().unwrap_or(0.0).to_bits(),
            stroke: style.stroke.unwrap_or(0.0).to_bits(),
        };
        if let Some(entry) = self.entries.get(&key) {
            return *entry;
        }
        let entry = self.rasterize(style, glyph, subpixel);
        // A glyph that missed only because the atlas is full is retried next frame.
        if entry.is_some() || !self.full {
            self.entries.insert(key, entry);
        }
        entry
    }

    fn rasterize(&mut self, style: &RunStyle<'_>, glyph: u32, subpixel: u8) -> Option<GlyphEntry> {
        let font = swash::FontRef::from_index(style.font.data.as_ref(), style.font.index as usize)?;
        let mut scaler = self
            .context
            .builder(font)
            .size(style.size)
            .hint(false)
            .normalized_coords(style.coords.iter().copied())
            .build();
        let mut render = Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ]);
        render
            .format(Format::Alpha)
            .offset(Vector::new(f32::from(subpixel) / SUBPIXEL_STEPS, 0.0));
        if style.synthesis.embolden() {
            render.embolden(style.size / 48.0);
        }
        if let Some(angle) = style.synthesis.skew() {
            render.transform(Some(Transform::skew(
                Angle::from_degrees(angle),
                Angle::ZERO,
            )));
        }
        if let Some(width) = style.stroke {
            render.style(Stroke::new(width));
        }
        let image = render.render(&mut scaler, u16::try_from(glyph).ok()?)?;
        let placement = image.placement;
        if placement.width == 0 || placement.height == 0 {
            return None;
        }
        let (x, y) = self.allocate(placement.width, placement.height)?;
        let alpha = alpha_of(&image);
        for row in 0..placement.height {
            let src = (row * placement.width) as usize;
            let dst = ((y + row) * self.size + x) as usize;
            let width = placement.width as usize;
            self.pixels[dst..dst + width].copy_from_slice(&alpha[src..src + width]);
        }
        self.dirty = true;
        Some(GlyphEntry {
            x,
            y,
            width: placement.width,
            height: placement.height,
            left: placement.left,
            top: placement.top,
        })
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        // An unrenderable glyph must not flush useful entries every frame.
        if width > MAX_SIZE - PADDING || height > MAX_SIZE - PADDING {
            return None;
        }
        let (w, h) = (width + PADDING, height + PADDING);
        loop {
            if let Some(found) = self.place(w, h) {
                return Some(found);
            }
            if self.size >= MAX_SIZE {
                self.full = true;
                return None;
            }
            self.grow();
        }
    }

    fn place(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let size = self.size;
        if let Some(shelf) = self
            .shelves
            .iter_mut()
            .find(|s| s.height >= h && s.height <= h + h / 4 + 2 && s.cursor + w <= size)
        {
            let at = (shelf.cursor, shelf.y);
            shelf.cursor += w;
            return Some(at);
        }
        if self.next_y + h > size || w > size {
            return None;
        }
        let y = self.next_y;
        self.next_y += h;
        self.shelves.push(Shelf {
            y,
            height: h,
            cursor: w,
        });
        Some((0, y))
    }

    /// Doubles the atlas and keeps every glyph at its pixel position.
    fn grow(&mut self) {
        let old = self.size;
        let new = old * 2;
        let mut pixels = vec![0; (new * new) as usize];
        for row in 0..old {
            let src = (row * old) as usize;
            let dst = (row * new) as usize;
            pixels[dst..dst + old as usize].copy_from_slice(&self.pixels[src..src + old as usize]);
        }
        self.pixels = pixels;
        self.size = new;
        self.dirty = true;
    }
}

/// Colour glyphs (emoji) come back as RGBA; the mask keeps their alpha.
fn alpha_of(image: &swash::scale::image::Image) -> std::borrow::Cow<'_, [u8]> {
    match image.content {
        swash::scale::image::Content::Mask => std::borrow::Cow::Borrowed(&image.data),
        _ => std::borrow::Cow::Owned(image.data.chunks(4).map(|px| px[3]).collect()),
    }
}

fn hash_coords(coords: &[i16]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    coords.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_rounds_to_quarter_pixels() {
        assert_eq!(GlyphCache::snap(3.0), (3.0, 0));
        assert_eq!(GlyphCache::snap(3.26), (3.0, 1));
        assert_eq!(GlyphCache::snap(3.9), (4.0, 0));
    }

    #[test]
    fn growing_keeps_placed_pixels() {
        let mut cache = GlyphCache::new();
        let first = cache.allocate(10, 10);
        assert_eq!(first, Some((0, 0)));
        cache.pixels[0] = 7;
        cache.grow();
        assert_eq!(cache.size, START_SIZE * 2);
        assert_eq!(cache.pixels[0], 7);
    }

    #[test]
    fn full_atlas_clears_on_the_next_frame() {
        let mut cache = GlyphCache::new();
        assert_eq!(
            cache.allocate(MAX_SIZE - PADDING, MAX_SIZE - PADDING),
            Some((0, 0))
        );
        assert_eq!(cache.allocate(1, 1), None);
        assert!(cache.full);
        cache.begin_frame();
        assert!(!cache.full);
        assert_eq!(cache.allocate(4, 4), Some((0, 0)));
    }

    #[test]
    fn oversized_glyphs_do_not_grow_or_flush_the_atlas() {
        let mut cache = GlyphCache::new();
        assert_eq!(cache.allocate(4, 4), Some((0, 0)));
        cache.pixels[0] = 7;
        for (width, height) in [(MAX_SIZE, 1), (1, MAX_SIZE), (u32::MAX, u32::MAX)] {
            assert_eq!(cache.allocate(width, height), None);
            assert_eq!(cache.size, START_SIZE);
            assert!(!cache.full);
        }
        cache.begin_frame();
        assert_eq!(cache.pixels[0], 7);
        assert_eq!(cache.allocate(4, 4), Some((4 + PADDING, 0)));
    }
}
