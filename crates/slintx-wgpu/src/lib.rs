//! A Slint renderer written on wgpu.
//!
//! It replaces Slint's GPU renderers (FemtoVG, Skia): none of their code is in
//! this crate or its dependency graph. Slint lays text out itself
//! (`sharedparley`); this crate rasterizes glyphs with swash, tessellates paths
//! with lyon and draws everything else as signed distance fields in one WGSL
//! pipeline.
//!
//! Hand a [`WgpuRenderer`] to Slint from a `WindowAdapter::renderer`, then call
//! [`WgpuRenderer::render_to_texture`] with the frame's target.
//!
//! Not supported yet: item rotation and scaling (like Slint's software
//! renderer, [`supports_transformations`](i_slint_core::renderer::RendererSealed::supports_transformations)
//! is false), layers (children of an `Opacity` fade one by one), inset box
//! shadows, and more than one rounded clip at a time.

mod frame;
mod glyphs;
mod gpu;
mod images;
mod scene;

use std::cell::{OnceCell, RefCell};
use std::pin::Pin;
use std::rc::{Rc, Weak};

use i_slint_core::api::PhysicalSize;
use i_slint_core::graphics::{Rgba8Pixel, SharedPixelBuffer};
use i_slint_core::item_rendering::render_component_items;
use i_slint_core::item_tree::{ItemTreeRef, ItemTreeWeak};
use i_slint_core::items::ItemRef;
use i_slint_core::platform::PlatformError;
use i_slint_core::renderer::RendererSealed;
use i_slint_core::textlayout::sharedparley::TextLayoutCache;
use i_slint_core::window::{WindowAdapter, WindowInner};

pub use wgpu;

use crate::frame::Frame;
use crate::glyphs::GlyphCache;
use crate::gpu::Gpu;
use crate::images::ImageCache;
use crate::scene::Scene;

/// Draws one Slint window with wgpu. Create one per window adapter.
pub struct WgpuRenderer {
    gpu: Gpu,
    window_adapter: OnceCell<Weak<dyn WindowAdapter>>,
    text_layout_cache: TextLayoutCache,
    glyphs: RefCell<GlyphCache>,
    images: RefCell<ImageCache>,
}

impl WgpuRenderer {
    /// The device and queue may be shared with the host and other renderers.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            gpu: Gpu::new(device, queue),
            window_adapter: OnceCell::new(),
            text_layout_cache: TextLayoutCache::default(),
            glyphs: RefCell::new(GlyphCache::new()),
            images: RefCell::new(ImageCache::default()),
        }
    }

    /// Opens a device on the default adapter, for hosts without their own
    /// (tests, offscreen rendering).
    pub fn headless() -> Result<Self, PlatformError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .map_err(|err| PlatformError::Other(format!("no GPU adapter: {err}")))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("slintx-wgpu"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|err| PlatformError::Other(format!("no GPU device: {err}")))?;
        Ok(Self::new(device, queue))
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.gpu.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.gpu.queue
    }

    /// Draws the window's items into `target`, which must allow
    /// `RENDER_ATTACHMENT` and use a non-sRGB 8-bit format (`Rgba8Unorm` or
    /// `Bgra8Unorm`): colours blend in sRGB-encoded space, as in Slint's
    /// software renderer.
    pub fn render_to_texture(&self, target: &wgpu::Texture) -> Result<(), PlatformError> {
        let adapter = self
            .window_adapter()
            .ok_or_else(|| PlatformError::Other("the renderer has no window".into()))?;
        let window = WindowInner::from_pub(adapter.window());
        let scale = adapter.window().scale_factor();
        let size = [target.width() as f32, target.height() as f32];

        let mut scene = Scene::new();
        let mut glyphs = self.glyphs.borrow_mut();
        let mut images = self.images.borrow_mut();
        glyphs.begin_frame();
        images.begin_frame();
        window.draw_contents(|components, post_render| {
            let mut frame = Frame::new(
                &mut scene,
                window,
                &mut glyphs,
                &mut images,
                self.gpu.uploader(),
                &self.text_layout_cache,
                scale,
                size,
            );
            for (component, origin) in components {
                if let Some(component) = ItemTreeWeak::upgrade(component) {
                    render_component_items(&component, &mut frame, *origin, &adapter);
                }
            }
            post_render(&mut frame);
        });
        self.gpu.draw(&scene, &mut glyphs, target);
        Ok(())
    }

    /// Draws into a new `Rgba8Unorm` texture of the window's size and reads it
    /// back as straight-alpha RGBA8.
    pub fn render_to_pixels(&self) -> Result<SharedPixelBuffer<Rgba8Pixel>, PlatformError> {
        let adapter = self
            .window_adapter()
            .ok_or_else(|| PlatformError::Other("the renderer has no window".into()))?;
        let size = adapter.size();
        let (width, height) = (size.width.max(1), size.height.max(1));
        let texture = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("slintx-snapshot"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.render_to_texture(&texture)?;
        let bytes = self.gpu.read_back(&texture).map_err(PlatformError::Other)?;
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
        let (pixels, _) = bytes.as_chunks::<4>();
        for (out, px) in buffer.make_mut_slice().iter_mut().zip(pixels) {
            *out = unpremultiply(*px);
        }
        Ok(buffer)
    }
}

fn unpremultiply([r, g, b, a]: [u8; 4]) -> Rgba8Pixel {
    if a == 0 {
        return Rgba8Pixel {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
    }
    let un = |c: u8| ((u16::from(c) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8;
    Rgba8Pixel {
        r: un(r),
        g: un(g),
        b: un(b),
        a,
    }
}

impl RendererSealed for WgpuRenderer {
    fn text_layout_cache(&self) -> Option<&TextLayoutCache> {
        Some(&self.text_layout_cache)
    }

    fn free_graphics_resources(
        &self,
        component: ItemTreeRef,
        _items: &mut dyn Iterator<Item = Pin<ItemRef<'_>>>,
    ) -> Result<(), PlatformError> {
        self.text_layout_cache.component_destroyed(component);
        Ok(())
    }

    fn set_window_adapter(&self, window_adapter: &Rc<dyn WindowAdapter>) {
        let _ = self.window_adapter.set(Rc::downgrade(window_adapter));
    }

    fn window_adapter(&self) -> Option<Rc<dyn WindowAdapter>> {
        self.window_adapter.get().and_then(Weak::upgrade)
    }

    fn resize(&self, _size: PhysicalSize) -> Result<(), PlatformError> {
        Ok(())
    }

    fn take_snapshot(&self) -> Result<SharedPixelBuffer<Rgba8Pixel>, PlatformError> {
        self.render_to_pixels()
    }

    fn supports_transformations(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpremultiply_restores_straight_alpha() {
        let px = unpremultiply([64, 32, 0, 128]);
        assert_eq!((px.r, px.g, px.b, px.a), (128, 64, 0, 128));
        let clear = unpremultiply([0, 0, 0, 0]);
        assert_eq!(clear.a, 0);
    }
}
