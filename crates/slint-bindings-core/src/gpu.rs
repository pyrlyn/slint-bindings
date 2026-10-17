//! FemtoVG on wgpu, presented into a host surface.
//!
//! macOS passes a `CAMetalLayer`. Windows passes an `ISwapChainPanel`. The
//! software platform stays installed; [`slint_embed::platform::set_next_window`]
//! swaps in this adapter for the one component being created, so a CPU host
//! in the same process still gets a [`MinimalSoftwareWindow`](slint::platform::software_renderer::MinimalSoftwareWindow).
//!
//! Skia is not used. `i-slint-renderer-skia` compiles all of Skia, and Slint
//! does not re-export a public Skia renderer. FemtoVG's wgpu renderer is the
//! public one (`renderer-femtovg-wgpu` + `unstable-wgpu-30`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use slint::platform::femtovg_renderer::FemtoVGWGPURenderer;
use slint::platform::{WindowAdapter, WindowEvent};
use slint::wgpu_30::wgpu;
use slint::{ComponentHandle, LogicalPosition, PhysicalSize, SharedString};

use crate::host::{
    check_scale, dispatch_composition_window, dispatch_window, move_focus_inside, send_window,
};
use crate::ime::{ImeSession, ImeUpdate};
use crate::keys::utf16_selection_to_utf8;
use crate::{Error, Frame, platform};

struct GpuSession {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

thread_local! {
    static SESSION: RefCell<Option<Rc<GpuSession>>> = const { RefCell::new(None) };
}

impl GpuSession {
    fn shared() -> Result<Rc<Self>, Error> {
        if let Some(session) = SESSION.with_borrow(Clone::clone) {
            return Ok(session);
        }
        let backends = if cfg!(target_os = "macos") {
            wgpu::Backends::METAL
        } else {
            wgpu::Backends::DX12
        };
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = backends;
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .map_err(|err| Error::Gpu(err.to_string()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("slint-bindings"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|err| Error::Gpu(err.to_string()))?;
        let session = Rc::new(Self {
            instance,
            adapter,
            device,
            queue,
        });
        SESSION.with_borrow_mut(|slot| *slot = Some(session.clone()));
        Ok(session)
    }
}

struct Attached {
    /// Host pointer the surface was created from, so a lost swapchain can be rebuilt.
    /// The host keeps the object alive until this window is dropped.
    ptr: *mut std::ffi::c_void,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

/// One GPU window. The host presents by calling [`GpuWindow::render`].
struct GpuWindow {
    window: slint::Window,
    renderer: FemtoVGWGPURenderer,
    session: Rc<GpuSession>,
    size: Cell<PhysicalSize>,
    attached: RefCell<Option<Attached>>,
    needs_redraw: Cell<bool>,
}

impl GpuWindow {
    fn new(session: Rc<GpuSession>) -> Result<Rc<Self>, Error> {
        let renderer = FemtoVGWGPURenderer::new(
            session.instance.clone(),
            session.device.clone(),
            session.queue.clone(),
        )?;
        Ok(Rc::new_cyclic(|weak| Self {
            window: slint::Window::new(weak.clone()),
            renderer,
            session,
            size: Cell::new(PhysicalSize::new(1, 1)),
            attached: RefCell::new(None),
            needs_redraw: Cell::new(true),
        }))
    }

    fn slint_window(&self) -> &slint::Window {
        &self.window
    }

    /// # Safety
    ///
    /// `ptr` is a live `CAMetalLayer` on macOS or `ISwapChainPanel` on Windows,
    /// and it stays live until this window is dropped. wgpu retains the layer
    /// and increments the panel's reference count for the life of the surface.
    unsafe fn attach(&self, ptr: *mut std::ffi::c_void) -> Result<(), Error> {
        if ptr.is_null() {
            return Err(Error::InvalidArgument("the GPU surface pointer is null"));
        }
        // SAFETY: the caller keeps `ptr` alive for the window, which is the
        // contract of `SurfaceTargetUnsafe` for these two variants.
        let surface = unsafe { self.open_surface(ptr) }?;
        if !self.session.adapter.is_surface_supported(&surface) {
            return Err(Error::Gpu(
                "the GPU adapter cannot present to this surface".into(),
            ));
        }
        let size = self.size.get();
        let config = self.configure_of(&surface, size.width.max(1), size.height.max(1))?;
        *self.attached.borrow_mut() = Some(Attached {
            ptr,
            surface,
            config,
        });
        self.needs_redraw.set(true);
        Ok(())
    }

    /// # Safety
    /// `ptr` meets the same contract as [`Self::attach`].
    unsafe fn open_surface(
        &self,
        ptr: *mut std::ffi::c_void,
    ) -> Result<wgpu::Surface<'static>, Error> {
        #[cfg(target_os = "macos")]
        let target = wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(ptr);
        #[cfg(target_os = "windows")]
        let target = wgpu::SurfaceTargetUnsafe::SwapChainPanel(ptr);
        // SAFETY: forwarded to the caller. The surface is `'static` because the
        // host, not a Rust borrow, owns the layer or panel.
        unsafe { self.session.instance.create_surface_unsafe(target) }
            .map_err(|err| Error::Gpu(err.to_string()))
    }

    fn configure_of(
        &self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
    ) -> Result<wgpu::SurfaceConfiguration, Error> {
        let caps = surface.get_capabilities(&self.session.adapter);
        let mut config = surface
            .get_default_config(&self.session.adapter, width, height)
            .ok_or_else(|| Error::Gpu("the surface has no presentable format".into()))?;
        if let Some(format) = caps.formats.iter().copied().find(|format| {
            matches!(
                format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
            )
        }) {
            config.format = format;
        }
        if caps.present_modes.contains(&wgpu::PresentMode::Fifo) {
            config.present_mode = wgpu::PresentMode::Fifo;
        }
        if caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            config.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
        }
        surface.configure(&self.session.device, &config);
        Ok(config)
    }

    fn reconfigure(&self, width: u32, height: u32) -> Result<(), Error> {
        let mut slot = self.attached.borrow_mut();
        let Some(attached) = slot.as_mut() else {
            return Ok(());
        };
        attached.config.width = width.max(1);
        attached.config.height = height.max(1);
        attached
            .surface
            .configure(&self.session.device, &attached.config);
        self.needs_redraw.set(true);
        Ok(())
    }

    fn render(&self) -> Result<Frame, Error> {
        let animating = self.window.has_active_animations();
        if !self.needs_redraw.replace(false) && !animating {
            return Ok(Frame {
                redrawn: false,
                animating,
            });
        }
        self.present_frame(false)
    }

    fn present_frame(&self, retried: bool) -> Result<Frame, Error> {
        // Take the texture, then drop the surface borrow before FemtoVG runs.
        // Rendering queries the window adapter; holding the surface RefCell
        // across that call would panic if the adapter touched it.
        let taken = {
            let slot = self.attached.borrow();
            let Some(attached) = slot.as_ref() else {
                return Err(Error::InvalidArgument(
                    "this GPU host has no surface; the CPU renderer is the fallback",
                ));
            };
            match attached.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => Ok(Some(texture)),
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    Ok(None)
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost
                    if !retried =>
                {
                    Err((attached.ptr, attached.config.width, attached.config.height))
                }
                wgpu::CurrentSurfaceTexture::Outdated
                | wgpu::CurrentSurfaceTexture::Lost
                | wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(Error::Gpu(
                        "the swapchain was lost and could not be recreated".into(),
                    ));
                }
            }
        };
        let texture = match taken {
            Ok(None) => {
                self.needs_redraw.set(true);
                return Ok(Frame {
                    redrawn: false,
                    animating: true,
                });
            }
            Ok(Some(texture)) => texture,
            Err((ptr, width, height)) => {
                // SAFETY: `ptr` is the same host pointer `attach` checked.
                unsafe { self.attach(ptr) }?;
                self.reconfigure(width, height)?;
                return self.present_frame(true);
            }
        };
        self.renderer.render_to_texture(&texture.texture)?;
        self.session.queue.present(texture);
        Ok(Frame {
            redrawn: true,
            animating: self.window.has_active_animations(),
        })
    }

    fn render_offscreen(&self, width: u32, height: u32) -> Result<(), Error> {
        let texture = self
            .session
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("slint-offscreen"),
                size: wgpu::Extent3d {
                    width: width.max(1),
                    height: height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
        self.renderer.render_to_texture(&texture)?;
        Ok(())
    }
}

impl WindowAdapter for GpuWindow {
    fn window(&self) -> &slint::Window {
        &self.window
    }

    fn size(&self) -> PhysicalSize {
        self.size.get()
    }

    fn set_size(&self, size: slint::WindowSize) {
        let scale = self.window.scale_factor();
        let physical = size.to_physical(scale);
        self.size.set(physical);
        self.needs_redraw.set(true);
        self.window.dispatch_event(WindowEvent::Resized {
            size: size.to_logical(scale),
        });
    }

    fn request_redraw(&self) {
        self.needs_redraw.set(true);
    }

    fn renderer(&self) -> &dyn slint::platform::Renderer {
        &self.renderer
    }
}

/// A Slint component rendered on the GPU into the host's surface.
pub struct GpuHost<C: ComponentHandle> {
    component: C,
    window: Rc<GpuWindow>,
    width: u32,
    height: u32,
    scale: f32,
    ime: RefCell<ImeSession>,
}

impl<C: ComponentHandle> GpuHost<C> {
    /// Creates the component and binds `surface` as its presentation target.
    ///
    /// # Safety
    ///
    /// `surface` is a live `CAMetalLayer` on macOS or `ISwapChainPanel` on
    /// Windows and outlives this host.
    pub unsafe fn new(
        make: impl FnOnce() -> Result<C, slint::PlatformError>,
        surface: *mut std::ffi::c_void,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<Self, Error> {
        // SAFETY: forwarded to `attach`, which documents the pointer.
        unsafe { Self::open(make, Some(surface), width, height, scale) }
    }

    /// Same component, drawn into an offscreen texture. Used where no window
    /// surface exists (the GPU smoke test). Fails with [`Error::Gpu`] when
    /// the machine has no adapter.
    pub fn new_offscreen(
        make: impl FnOnce() -> Result<C, slint::PlatformError>,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<Self, Error> {
        // SAFETY: there is no surface pointer on this path.
        unsafe { Self::open(make, None, width, height, scale) }
    }

    /// # Safety
    /// When `surface` is `Some`, the pointer meets [`Self::new`]'s contract.
    unsafe fn open(
        make: impl FnOnce() -> Result<C, slint::PlatformError>,
        surface: Option<*mut std::ffi::c_void>,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<Self, Error> {
        check_scale(scale)?;
        platform::ensure_installed()?;
        let session = GpuSession::shared()?;
        let window = GpuWindow::new(session)?;
        let staged = window.clone();
        slint_embed::platform::set_next_window(Box::new(move || Ok(staged)));
        let component = match make() {
            Ok(component) => component,
            Err(err) => {
                slint_embed::platform::clear_next_window();
                return Err(err.into());
            }
        };
        if slint_embed::platform::clear_next_window() {
            return Err(Error::NoWindow);
        }
        let _ = component.window();
        if let Some(surface) = surface {
            // SAFETY: `new` requires the pointer to outlive the host.
            unsafe { window.attach(surface) }?;
        }
        component.show()?;
        let mut host = Self {
            component,
            window,
            width: 0,
            height: 0,
            scale,
            ime: RefCell::new(ImeSession::default()),
        };
        host.resize(width, height, scale)?;
        Ok(host)
    }

    pub fn component(&self) -> &C {
        &self.component
    }

    /// Current surface size in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn frame_len(&self) -> usize {
        0
    }

    pub fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<(), Error> {
        check_scale(scale)?;
        let (width, height) = (width.max(1), height.max(1));
        if width == self.width && height == self.height && scale == self.scale {
            return Ok(());
        }
        if scale != self.scale || self.width == 0 {
            send_window(
                self.window.slint_window(),
                WindowEvent::ScaleFactorChanged {
                    scale_factor: scale,
                },
            )?;
        }
        self.scale = scale;
        self.width = width;
        self.height = height;
        self.window
            .slint_window()
            .set_size(PhysicalSize::new(width, height));
        self.window.reconfigure(width, height)?;
        Ok(())
    }

    pub fn render(&mut self, _out: &mut [u8]) -> Result<Frame, Error> {
        Err(Error::InvalidArgument(
            "this host draws on the GPU; call sb_host_gpu_render",
        ))
    }

    pub fn render_bgra(&mut self, out: &mut [u8]) -> Result<Frame, Error> {
        self.render(out)
    }

    pub fn render_gpu(&self) -> Result<Frame, Error> {
        self.window.render()
    }

    pub fn render_offscreen(&self) -> Result<Frame, Error> {
        self.window.render_offscreen(self.width, self.height)?;
        Ok(Frame {
            redrawn: true,
            animating: self.window.slint_window().has_active_animations(),
        })
    }

    pub fn pointer_moved(&self, x: f32, y: f32) -> Result<(), Error> {
        send_window(
            self.window.slint_window(),
            WindowEvent::PointerMoved {
                position: LogicalPosition::new(x, y),
            },
        )
    }

    pub fn pointer_pressed(
        &self,
        x: f32,
        y: f32,
        button: crate::PointerButton,
    ) -> Result<(), Error> {
        send_window(
            self.window.slint_window(),
            WindowEvent::PointerPressed {
                position: LogicalPosition::new(x, y),
                button: button.into(),
            },
        )
    }

    pub fn pointer_released(
        &self,
        x: f32,
        y: f32,
        button: crate::PointerButton,
    ) -> Result<(), Error> {
        send_window(
            self.window.slint_window(),
            WindowEvent::PointerReleased {
                position: LogicalPosition::new(x, y),
                button: button.into(),
            },
        )
    }

    pub fn pointer_exited(&self) -> Result<(), Error> {
        send_window(self.window.slint_window(), WindowEvent::PointerExited)
    }

    pub fn pointer_scrolled(&self, x: f32, y: f32, dx: f32, dy: f32) -> Result<(), Error> {
        send_window(
            self.window.slint_window(),
            WindowEvent::PointerScrolled {
                position: LogicalPosition::new(x, y),
                delta_x: dx,
                delta_y: dy,
            },
        )
    }

    pub fn key_pressed(&self, text: &str) -> Result<bool, Error> {
        if text == "\t" || text == "\u{19}" {
            return move_focus_inside(self.window.slint_window(), text == "\t");
        }
        dispatch_window(
            self.window.slint_window(),
            WindowEvent::KeyPressed {
                text: SharedString::from(text),
            },
        )
    }

    pub fn key_released(&self, text: &str) -> Result<(), Error> {
        send_window(
            self.window.slint_window(),
            WindowEvent::KeyReleased {
                text: SharedString::from(text),
            },
        )
    }

    pub fn key_repeated(&self, text: &str) -> Result<(), Error> {
        send_window(
            self.window.slint_window(),
            WindowEvent::KeyPressRepeated {
                text: SharedString::from(text),
            },
        )
    }

    pub fn update_composition(
        &self,
        preedit: &str,
        utf16_start: i32,
        utf16_end: i32,
    ) -> Result<(), Error> {
        dispatch_composition_window(
            self.window.slint_window(),
            i_slint_core::input::KeyEventType::UpdateComposition,
            "",
            preedit,
            utf16_selection_to_utf8(preedit, utf16_start, utf16_end),
        )
    }

    pub fn commit_composition(&self, text: &str) -> Result<(), Error> {
        dispatch_composition_window(
            self.window.slint_window(),
            i_slint_core::input::KeyEventType::CommitComposition,
            text,
            "",
            None,
        )
    }

    pub fn ime_started(&self) -> Result<(), Error> {
        self.ime.borrow_mut().started();
        Ok(())
    }

    pub fn ime_composing(&self) -> bool {
        self.ime.borrow().composing()
    }

    pub fn ime_document(&self) -> String {
        self.ime.borrow().document().to_owned()
    }

    pub fn ime_selection(&self) -> (i32, i32) {
        self.ime.borrow().selection()
    }

    pub fn ime_replace(
        &self,
        range_start: i32,
        range_end: i32,
        text: &str,
        sel_start: i32,
        sel_end: i32,
    ) -> Result<(), Error> {
        let update =
            self.ime
                .borrow_mut()
                .replace(range_start, range_end, text, sel_start, sel_end)?;
        self.apply_ime(update)
    }

    pub fn ime_select(&self, start: i32, end: i32) -> Result<(), Error> {
        let update = self.ime.borrow_mut().set_selection(start, end);
        self.apply_ime(update)
    }

    pub fn ime_completed(&self, canceled: bool) -> Result<(), Error> {
        let update = self.ime.borrow_mut().completed(canceled);
        self.apply_ime(update)
    }

    fn apply_ime(&self, update: ImeUpdate) -> Result<(), Error> {
        match update {
            ImeUpdate::Ignored => Ok(()),
            ImeUpdate::Preedit {
                text,
                utf16_start,
                utf16_end,
            } => self.update_composition(&text, utf16_start, utf16_end),
            ImeUpdate::Commit { text } => self.commit_composition(&text),
            ImeUpdate::Clear => self.update_composition("", -1, -1),
            ImeUpdate::Character { text } => {
                let _accepted = self.key_pressed(&text)?;
                self.key_released(&text)
            }
        }
    }

    pub fn focus_changed(&self, focused: bool) -> Result<(), Error> {
        send_window(
            self.window.slint_window(),
            WindowEvent::WindowActiveChanged(focused),
        )
    }
}
