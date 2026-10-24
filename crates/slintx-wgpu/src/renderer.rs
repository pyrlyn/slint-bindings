//! The GPU and the caches that live across frames.

use std::cell::RefCell;

use crate::canvas::Canvas;
use crate::glyphs::GlyphCache;
use crate::gpu::Gpu;
use crate::images::ImageCache;
use crate::scene::Scene;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no GPU adapter: {0}")]
    NoAdapter(String),
    #[error("no GPU device: {0}")]
    NoDevice(String),
    #[error("reading the frame back failed: {0}")]
    ReadBack(String),
}

/// Draws frames with wgpu. Glyph and image textures are cached across frames.
pub struct Renderer {
    gpu: Gpu,
    glyphs: RefCell<GlyphCache>,
    images: RefCell<ImageCache>,
}

impl Renderer {
    /// The device and queue may be shared with the host and other renderers.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            gpu: Gpu::new(device, queue),
            glyphs: RefCell::new(GlyphCache::new()),
            images: RefCell::new(ImageCache::default()),
        }
    }

    /// Opens a device on the default adapter, for hosts without their own
    /// (tests, offscreen rendering).
    pub fn headless() -> Result<Self, Error> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .map_err(|err| Error::NoAdapter(err.to_string()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("slintx-wgpu"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|err| Error::NoDevice(err.to_string()))?;
        Ok(Self::new(device, queue))
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.gpu.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.gpu.queue
    }

    /// Records a frame with `draw`, then draws it into `target`, which must
    /// allow `RENDER_ATTACHMENT` and use a non-sRGB 8-bit format
    /// (`Rgba8Unorm` or `Bgra8Unorm`): colours blend in sRGB-encoded space,
    /// as in Slint's software renderer.
    pub fn render(&self, target: &wgpu::Texture, draw: impl FnOnce(&mut Canvas<'_>)) {
        let size = [target.width() as f32, target.height() as f32];
        let mut scene = Scene::new();
        let mut glyphs = self.glyphs.borrow_mut();
        let mut images = self.images.borrow_mut();
        glyphs.begin_frame();
        images.begin_frame();
        draw(&mut Canvas::new(
            &mut scene,
            &mut glyphs,
            &mut images,
            self.gpu.uploader(),
            size,
        ));
        self.gpu.draw(&scene, &mut glyphs, target);
    }

    /// Draws into a new `Rgba8Unorm` texture of `size` (at least 1×1) and
    /// reads it back as straight-alpha RGBA8 rows.
    pub fn render_to_pixels(
        &self,
        size: [u32; 2],
        draw: impl FnOnce(&mut Canvas<'_>),
    ) -> Result<Vec<u8>, Error> {
        let texture = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("slintx-snapshot"),
            size: wgpu::Extent3d {
                width: size[0].max(1),
                height: size[1].max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.render(&texture, draw);
        let mut bytes = self.gpu.read_back(&texture).map_err(Error::ReadBack)?;
        let (pixels, _) = bytes.as_chunks_mut::<4>();
        for px in pixels {
            *px = unpremultiply(*px);
        }
        Ok(bytes)
    }
}

fn unpremultiply([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    if a == 0 {
        return [0; 4];
    }
    let un = |c: u8| ((u16::from(c) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8;
    [un(r), un(g), un(b), a]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpremultiply_restores_straight_alpha() {
        assert_eq!(unpremultiply([64, 32, 0, 128]), [128, 64, 0, 128]);
        assert_eq!(unpremultiply([0, 0, 0, 0]), [0; 4]);
    }
}
