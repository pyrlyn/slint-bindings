//! `sb_*` C ABI over `slint-bindings-core`.
//!
//! Every entry point only forwards: it checks pointers, catches panics so they
//! never unwind into Swift or C#, and stores the failure text for
//! [`sb_last_error`]. All calls must come from the thread that created the
//! host (the host app's UI thread), because Slint objects are not `Send`.
//!
//! A downstream Rust library may link this crate as an rlib and hand out
//! hosts it built itself ([`SbHost::new`], [`SbHost::into_raw`]); the `sb_*`
//! symbols are then exported from that library, as long as it references
//! this crate (any call into it does).

#![allow(non_camel_case_types)]

use std::cell::RefCell;
use std::ffi::{CStr, CString, c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::ptr;

#[cfg(any(target_os = "macos", target_os = "windows"))]
use slint_bindings_core::GpuHost;
use slint_bindings_core::runtime::{self, ComponentDefinition, ComponentInstance, Instance};
use slint_bindings_core::{
    AppKitKey, EmbeddedHost, Error, PointerButton, VirtualKeyEvent, appkit_key_text,
    virtual_key_command, virtual_key_text,
};

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// C callback for [`sb_on_callback`]: `user_data`, then the arguments as a
/// UTF-8 JSON array, valid only during the call.
pub type SbCallbackFn = Option<extern "C" fn(user_data: *mut c_void, args_json: *const c_char)>;

/// Opaque handle to a compiled `.slint` component definition ([`sb_compile`]).
pub struct SbDefinition(ComponentDefinition);

/// Opaque handle to one interpreted component and its pixel buffer or GPU surface.
pub struct SbHost {
    // Declared first so the bridge's handle drops before the host's window.
    component: Instance,
    host: Backend,
}

enum Backend {
    Cpu(EmbeddedHost<ComponentInstance>),
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    Gpu(GpuHost<ComponentInstance>),
}

impl SbHost {
    /// Creates a component of `definition` drawn on the CPU at
    /// `width`×`height` physical pixels.
    pub fn new(
        definition: &ComponentDefinition,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<Self, Error> {
        let (host, component) = runtime::instantiate(definition, |make| {
            EmbeddedHost::new(make, width, height, scale)
        })?;
        Ok(Self {
            component,
            host: Backend::Cpu(host),
        })
    }

    /// Creates a component of `definition` presenting into `surface`: a
    /// `CAMetalLayer` on macOS, an `ISwapChainPanel` on Windows.
    ///
    /// # Safety
    /// `surface` is live and outlives the host.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub unsafe fn new_gpu(
        definition: &ComponentDefinition,
        surface: *mut c_void,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<Self, Error> {
        if surface.is_null() {
            return Err(Error::InvalidArgument("the GPU surface is null"));
        }
        let (host, component) = runtime::instantiate(definition, |make| {
            // SAFETY: forwarded caller contract.
            unsafe { GpuHost::new(make, surface, width, height, scale) }
        })?;
        Ok(Self {
            component,
            host: Backend::Gpu(host),
        })
    }

    /// The interpreted component, for Rust callers that set properties or
    /// callbacks directly.
    pub fn component(&self) -> &ComponentInstance {
        &self.component.inner
    }

    /// The component's JSON bridge (what `sb_*_json` and `sb_on_callback` use).
    pub fn bridge(&self) -> &Instance {
        &self.component
    }

    /// Hands the host to C. Free it with [`sb_host_free`].
    pub fn into_raw(self) -> *mut SbHost {
        Box::into_raw(Box::new(self))
    }
}

macro_rules! forward {
    ($self:ident.$method:ident($($arg:expr),*)) => {
        match $self {
            Backend::Cpu(host) => host.$method($($arg),*),
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Backend::Gpu(host) => host.$method($($arg),*),
        }
    };
}

impl Backend {
    fn frame_len(&self) -> usize {
        forward!(self.frame_len())
    }
    fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<(), Error> {
        forward!(self.resize(width, height, scale))
    }
    fn render(&mut self, out: &mut [u8]) -> Result<slint_bindings_core::Frame, Error> {
        forward!(self.render(out))
    }
    fn render_bgra(&mut self, out: &mut [u8]) -> Result<slint_bindings_core::Frame, Error> {
        forward!(self.render_bgra(out))
    }
    fn render_gpu(&self) -> Result<slint_bindings_core::Frame, Error> {
        match self {
            Backend::Cpu(_) => Err(Error::InvalidArgument(
                "this host draws on the CPU; call sb_host_render",
            )),
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Backend::Gpu(host) => host.render_gpu(),
        }
    }
    fn is_gpu(&self) -> bool {
        match self {
            Backend::Cpu(_) => false,
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Backend::Gpu(_) => true,
        }
    }
    fn pointer_moved(&self, x: f32, y: f32) -> Result<(), Error> {
        forward!(self.pointer_moved(x, y))
    }
    fn pointer_pressed(&self, x: f32, y: f32, button: PointerButton) -> Result<(), Error> {
        forward!(self.pointer_pressed(x, y, button))
    }
    fn pointer_released(&self, x: f32, y: f32, button: PointerButton) -> Result<(), Error> {
        forward!(self.pointer_released(x, y, button))
    }
    fn pointer_exited(&self) -> Result<(), Error> {
        forward!(self.pointer_exited())
    }
    fn pointer_scrolled(&self, x: f32, y: f32, dx: f32, dy: f32) -> Result<(), Error> {
        forward!(self.pointer_scrolled(x, y, dx, dy))
    }
    fn focus_changed(&self, focused: bool) -> Result<(), Error> {
        forward!(self.focus_changed(focused))
    }
    fn key_pressed(&self, text: &str) -> Result<bool, Error> {
        forward!(self.key_pressed(text))
    }
    fn key_released(&self, text: &str) -> Result<(), Error> {
        forward!(self.key_released(text))
    }
    fn key_repeated(&self, text: &str) -> Result<(), Error> {
        forward!(self.key_repeated(text))
    }
    fn update_composition(
        &self,
        preedit: &str,
        utf16_start: i32,
        utf16_end: i32,
    ) -> Result<(), Error> {
        forward!(self.update_composition(preedit, utf16_start, utf16_end))
    }
    fn commit_composition(&self, text: &str) -> Result<(), Error> {
        forward!(self.commit_composition(text))
    }
    fn ime_started(&self) -> Result<(), Error> {
        forward!(self.ime_started())
    }
    fn ime_composing(&self) -> bool {
        forward!(self.ime_composing())
    }
    fn ime_document(&self) -> String {
        forward!(self.ime_document())
    }
    fn ime_selection(&self) -> (i32, i32) {
        forward!(self.ime_selection())
    }
    fn ime_replace(
        &self,
        range_start: i32,
        range_end: i32,
        text: &str,
        sel_start: i32,
        sel_end: i32,
    ) -> Result<(), Error> {
        forward!(self.ime_replace(range_start, range_end, text, sel_start, sel_end))
    }
    fn ime_select(&self, start: i32, end: i32) -> Result<(), Error> {
        forward!(self.ime_select(start, end))
    }
    fn ime_completed(&self, canceled: bool) -> Result<(), Error> {
        forward!(self.ime_completed(canceled))
    }
}

/// Result of [`sb_host_render`].
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SbFrame {
    /// 1 when the buffer was written, 0 when the previous frame is still valid.
    pub redrawn: u8,
    /// 1 while animations run: render again on the next vsync.
    pub animating: u8,
    /// 1 when the call failed; see `sb_last_error`.
    pub failed: u8,
}

/// Mouse buttons, mirrored by the Swift and C# wrappers.
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum SbPointerButton {
    /// Primary button.
    Left = 0,
    /// Secondary button.
    Right = 1,
    /// Wheel button.
    Middle = 2,
}

fn pointer_button(button: u8) -> Result<PointerButton, Error> {
    PointerButton::try_from(button)
}

fn set_error(msg: String) {
    // Interior NULs would truncate the message on the C side; make them visible instead.
    let msg = msg.replace('\0', "\\0");
    LAST_ERROR.with_borrow_mut(|slot| *slot = CString::new(msg).ok());
}

/// Clears the error slot, runs `body`, and turns an error or a panic into
/// `fail` plus a message for `sb_last_error`.
fn guard<T>(fail: T, body: impl FnOnce() -> Result<T, Error>) -> T {
    LAST_ERROR.with_borrow_mut(|slot| *slot = None);
    if let Err(err) = slint_bindings_core::check_ui_thread() {
        set_error(err.to_string());
        return fail;
    }
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(value)) => value,
        Ok(Err(err)) => {
            set_error(err.to_string());
            fail
        }
        Err(_) => {
            set_error("panic inside slint-bindings".to_owned());
            fail
        }
    }
}

/// # Safety
/// `host` must be null or a live pointer returned by `sb_component_new*`.
unsafe fn host_ref<'a>(host: *const SbHost) -> Option<&'a SbHost> {
    // SAFETY: the caller guarantees `host` is null or live and unaliased by `sb_host_free`.
    unsafe { host.as_ref() }
}

/// Writes `text` and a trailing NUL into `out`.
///
/// # Safety
/// `out` must be null or valid for `out_len` writable bytes.
unsafe fn write_utf8(out: *mut c_char, out_len: usize, text: &str) -> Result<(), Error> {
    if out.is_null() || out_len == 0 {
        return Err(Error::InvalidArgument("the text buffer is missing"));
    }
    let bytes = text.as_bytes();
    if bytes.len() >= out_len {
        return Err(Error::InvalidArgument("the text buffer is too small"));
    }
    // SAFETY: `out` is valid for `out_len` bytes and `bytes.len() < out_len`,
    // so the copy and the trailing NUL stay inside the buffer.
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), out.cast(), bytes.len());
        *out.add(bytes.len()) = 0;
    }
    Ok(())
}

/// # Safety
/// `text` must be null or a NUL-terminated string valid for the call.
unsafe fn str_arg<'a>(text: *const c_char) -> Result<&'a str, Error> {
    if text.is_null() {
        return Ok("");
    }
    // SAFETY: non-null and NUL-terminated per the caller's contract.
    unsafe { CStr::from_ptr(text) }
        .to_str()
        .map_err(|_| Error::InvalidArgument("text is not valid UTF-8"))
}

/// Message of the last failed call on this thread, or null. Valid until the next `sb_*` call.
#[unsafe(no_mangle)]
pub extern "C" fn sb_last_error() -> *const c_char {
    LAST_ERROR.with_borrow(|slot| slot.as_ref().map_or(ptr::null(), |s| s.as_ptr()))
}

/// Compiles `.slint` source and returns its main component (the last exported
/// one). `path` names the file for diagnostics and relative imports; when
/// `source` is null, the file at `path` is read. Null on failure, with the
/// compiler's diagnostics in [`sb_last_error`]. Free with [`sb_definition_free`].
///
/// # Safety
/// `source` is null or NUL-terminated; `path` is NUL-terminated. Both are
/// valid for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_compile(
    source: *const c_char,
    path: *const c_char,
) -> *mut SbDefinition {
    guard(ptr::null_mut(), || {
        // SAFETY: forwarded caller contract.
        let path = unsafe { required_str(path, "the .slint path is null") }?;
        let source = if source.is_null() {
            None
        } else {
            // SAFETY: forwarded caller contract.
            Some(unsafe { str_arg(source) }?)
        };
        let definition = runtime::compile(source, Path::new(path))?;
        Ok(Box::into_raw(Box::new(SbDefinition(definition))))
    })
}

/// Destroys a definition. Hosts created from it stay valid. Null is ignored.
///
/// # Safety
/// `definition` is null or a pointer from [`sb_compile`] not freed before.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_definition_free(definition: *mut SbDefinition) {
    if !definition.is_null() {
        // SAFETY: the pointer came from `Box::into_raw` in `sb_compile` and is freed once.
        drop(unsafe { Box::from_raw(definition) });
    }
}

/// # Safety
/// `definition` must be null or live.
unsafe fn definition_ref<'a>(definition: *const SbDefinition) -> Result<&'a SbDefinition, Error> {
    // SAFETY: the caller guarantees `definition` is null or live.
    unsafe { definition.as_ref() }.ok_or(Error::InvalidArgument("a null definition was passed"))
}

/// Creates a component of `definition` drawn on the CPU at `width`×`height`
/// physical pixels. Null on failure. Free with [`sb_host_free`].
///
/// # Safety
/// `definition` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_component_new(
    definition: *const SbDefinition,
    width: u32,
    height: u32,
    scale: f32,
) -> *mut SbHost {
    guard(ptr::null_mut(), || {
        // SAFETY: forwarded caller contract.
        let definition = unsafe { definition_ref(definition) }?;
        Ok(SbHost::new(&definition.0, width, height, scale)?.into_raw())
    })
}

/// Creates a component of `definition` on a `CAMetalLayer` (`layer`). Null on
/// failure, including on every OS other than macOS and when the GPU adapter
/// cannot be opened; the caller then uses [`sb_component_new`].
///
/// # Safety
/// `definition` must be null or live; `layer` is null or a live
/// `CAMetalLayer` that outlives the returned host.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_component_new_metal(
    definition: *const SbDefinition,
    layer: *mut c_void,
    width: u32,
    height: u32,
    scale: f32,
) -> *mut SbHost {
    guard(ptr::null_mut(), || {
        // SAFETY: forwarded caller contract.
        let definition = unsafe { definition_ref(definition) }?;
        #[cfg(target_os = "macos")]
        {
            // SAFETY: the caller keeps the layer alive until `sb_host_free`.
            Ok(unsafe { SbHost::new_gpu(&definition.0, layer, width, height, scale) }?.into_raw())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (definition, layer, width, height, scale);
            Err(Error::Gpu(
                "Metal rendering is only available on macOS".into(),
            ))
        }
    })
}

/// Creates a component of `definition` on an `ISwapChainPanel` (`panel`).
/// Null on failure, including on every OS other than Windows; the caller then
/// uses [`sb_component_new`].
///
/// # Safety
/// `definition` must be null or live; `panel` is null or a live
/// `ISwapChainPanel` that outlives the returned host.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_component_new_swapchain(
    definition: *const SbDefinition,
    panel: *mut c_void,
    width: u32,
    height: u32,
    scale: f32,
) -> *mut SbHost {
    guard(ptr::null_mut(), || {
        // SAFETY: forwarded caller contract.
        let definition = unsafe { definition_ref(definition) }?;
        #[cfg(target_os = "windows")]
        {
            // SAFETY: the caller keeps the panel alive until `sb_host_free`.
            Ok(unsafe { SbHost::new_gpu(&definition.0, panel, width, height, scale) }?.into_raw())
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (definition, panel, width, height, scale);
            Err(Error::Gpu(
                "SwapChainPanel rendering is only available on Windows".into(),
            ))
        }
    })
}

/// 1 when `host` presents on the GPU.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_is_gpu(host: *const SbHost) -> bool {
    // SAFETY: forwarded caller contract.
    unsafe { host_ref(host) }.is_some_and(|h| h.host.is_gpu())
}

/// Presents one frame into the surface passed to `sb_component_new_metal` or
/// `sb_component_new_swapchain`. `failed` is 1 on a CPU host or a lost swapchain.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_gpu_render(host: *mut SbHost) -> SbFrame {
    guard(
        SbFrame {
            failed: 1,
            ..SbFrame::default()
        },
        || {
            // SAFETY: forwarded caller contract; exclusive because the caller holds the only handle.
            let Some(h) = (unsafe { host.as_mut() }) else {
                return Err(Error::InvalidArgument("a null host was passed"));
            };
            let frame = h.host.render_gpu()?;
            Ok(SbFrame {
                redrawn: u8::from(frame.redrawn),
                animating: u8::from(frame.animating),
                failed: 0,
            })
        },
    )
}

/// Destroys a host. Null is ignored.
///
/// # Safety
/// `host` must be null or a pointer from `sb_component_new*` (or
/// [`SbHost::into_raw`]) not freed before.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_free(host: *mut SbHost) {
    if !host.is_null() {
        // SAFETY: the pointer came from `Box::into_raw` in `SbHost::into_raw` and is freed once.
        drop(unsafe { Box::from_raw(host) });
    }
}

/// Bytes the buffer passed to [`sb_host_render`] must hold; 0 for a null host.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_frame_len(host: *const SbHost) -> usize {
    // SAFETY: forwarded caller contract.
    unsafe { host_ref(host) }.map_or(0, |h| h.host.frame_len())
}

/// Resizes the frame; returns false on failure.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_resize(
    host: *mut SbHost,
    width: u32,
    height: u32,
    scale: f32,
) -> bool {
    guard(false, || {
        // SAFETY: forwarded caller contract; exclusive because the caller holds the only handle.
        let Some(h) = (unsafe { host.as_mut() }) else {
            return Err(Error::InvalidArgument("a null host was passed"));
        };
        h.host.resize(width, height, scale)?;
        Ok(true)
    })
}

/// Renders into `buf` (premultiplied RGBA8, `len` bytes) if the scene changed.
///
/// # Safety
/// `host` must be null or live; `buf` must be valid for `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_render(host: *mut SbHost, buf: *mut u8, len: usize) -> SbFrame {
    let failed = SbFrame {
        failed: 1,
        ..SbFrame::default()
    };
    guard(failed, || {
        // SAFETY: forwarded caller contract.
        let Some(h) = (unsafe { host.as_mut() }) else {
            return Err(Error::InvalidArgument("a null host was passed"));
        };
        if buf.is_null() {
            return Err(Error::InvalidArgument("a null pixel buffer was passed"));
        }
        // SAFETY: the caller guarantees `buf` is valid for `len` writable bytes.
        let out = unsafe { std::slice::from_raw_parts_mut(buf, len) };
        let frame = h.host.render(out)?;
        Ok(SbFrame {
            redrawn: frame.redrawn.into(),
            animating: frame.animating.into(),
            failed: 0,
        })
    })
}

/// Renders into `buf` (premultiplied BGRA8, `len` bytes) if the scene changed.
///
/// WinUI wants this order. macOS keeps using [`sb_host_render`].
///
/// # Safety
/// `host` must be null or live; `buf` must be valid for `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_render_bgra(
    host: *mut SbHost,
    buf: *mut u8,
    len: usize,
) -> SbFrame {
    let failed = SbFrame {
        failed: 1,
        ..SbFrame::default()
    };
    guard(failed, || {
        // SAFETY: forwarded caller contract.
        let Some(h) = (unsafe { host.as_mut() }) else {
            return Err(Error::InvalidArgument("a null host was passed"));
        };
        if buf.is_null() {
            return Err(Error::InvalidArgument("a null pixel buffer was passed"));
        }
        // SAFETY: the caller guarantees `buf` is valid for `len` writable bytes.
        let out = unsafe { std::slice::from_raw_parts_mut(buf, len) };
        let frame = h.host.render_bgra(out)?;
        Ok(SbFrame {
            redrawn: frame.redrawn.into(),
            animating: frame.animating.into(),
            failed: 0,
        })
    })
}

/// Advances Slint timers and animations; call once per host frame tick.
#[unsafe(no_mangle)]
pub extern "C" fn sb_tick() {
    guard((), || {
        EmbeddedHost::<ComponentInstance>::tick();
        Ok(())
    });
}

/// Runs one event forwarder: false for a null host or a failed dispatch.
///
/// # Safety
/// `host` must be null or live.
unsafe fn with_host(host: *const SbHost, f: impl FnOnce(&SbHost) -> Result<(), Error>) -> bool {
    guard(false, || {
        // SAFETY: forwarded caller contract.
        let Some(h) = (unsafe { host_ref(host) }) else {
            return Err(Error::InvalidArgument("a null host was passed"));
        };
        f(h)?;
        Ok(true)
    })
}

/// Pointer moved, in logical points from the top-left corner.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_pointer_moved(host: *const SbHost, x: f32, y: f32) -> bool {
    // SAFETY: forwarded caller contract.
    unsafe { with_host(host, |h| h.host.pointer_moved(x, y)) }
}

/// Pointer button pressed, in logical points. `button` is 0 left, 1 right, 2 middle.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_pointer_pressed(
    host: *const SbHost,
    x: f32,
    y: f32,
    button: u8,
) -> bool {
    // SAFETY: forwarded caller contract.
    unsafe {
        with_host(host, |h| {
            h.host.pointer_pressed(x, y, pointer_button(button)?)
        })
    }
}

/// Pointer button released, in logical points. `button` is 0 left, 1 right, 2 middle.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_pointer_released(
    host: *const SbHost,
    x: f32,
    y: f32,
    button: u8,
) -> bool {
    // SAFETY: forwarded caller contract.
    unsafe {
        with_host(host, |h| {
            h.host.pointer_released(x, y, pointer_button(button)?)
        })
    }
}

/// Pointer left the view.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_pointer_exited(host: *const SbHost) -> bool {
    // SAFETY: forwarded caller contract.
    unsafe { with_host(host, |h| h.host.pointer_exited()) }
}

/// Scroll delta at `x`,`y`, all in logical points.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_pointer_scrolled(
    host: *const SbHost,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
) -> bool {
    // SAFETY: forwarded caller contract.
    unsafe { with_host(host, |h| h.host.pointer_scrolled(x, y, dx, dy)) }
}

/// The host view gained (true) or lost (false) keyboard focus.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_focus_changed(host: *const SbHost, focused: bool) -> bool {
    // SAFETY: forwarded caller contract.
    unsafe { with_host(host, |h| h.host.focus_changed(focused)) }
}

/// A key went down; `text` is UTF-8 (the typed character or a Slint key code).
///
/// Returns true when Slint accepted the key. Returns false when the scene
/// rejected it or the call failed. A rejection leaves `sb_last_error` null, so
/// the host can move focus (Tab with no next item). A failure sets it.
///
/// # Safety
/// `host` must be null or live; `text` null or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_key_pressed(host: *const SbHost, text: *const c_char) -> bool {
    guard(false, || {
        // SAFETY: forwarded caller contract.
        let Some(h) = (unsafe { host_ref(host) }) else {
            return Err(Error::InvalidArgument("a null host was passed"));
        };
        // SAFETY: forwarded caller contract.
        h.host.key_pressed(unsafe { str_arg(text) }?)
    })
}

/// A key went up. Returns false only when the call failed; see `sb_last_error`.
/// A release the scene ignores is still a success.
///
/// # Safety
/// `host` must be null or live; `text` null or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_key_released(host: *const SbHost, text: *const c_char) -> bool {
    guard(false, || {
        // SAFETY: forwarded caller contract.
        let Some(h) = (unsafe { host_ref(host) }) else {
            return Err(Error::InvalidArgument("a null host was passed"));
        };
        // SAFETY: forwarded caller contract.
        h.host.key_released(unsafe { str_arg(text) }?)?;
        Ok(true)
    })
}

/// A held key auto-repeated; `text` is UTF-8 (the typed character or a Slint key code).
///
/// # Safety
/// `host` must be null or live; `text` null or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_key_repeated(host: *const SbHost, text: *const c_char) -> bool {
    guard(false, || {
        // SAFETY: forwarded caller contract.
        let Some(h) = (unsafe { host_ref(host) }) else {
            return Err(Error::InvalidArgument("a null host was passed"));
        };
        // SAFETY: forwarded caller contract.
        h.host.key_repeated(unsafe { str_arg(text) }?)?;
        Ok(true)
    })
}

/// Replaces the input-method preedit. `utf16_start`/`utf16_end` select inside
/// `preedit` in UTF-16 code units (an AppKit `NSRange`). A negative start means
/// no selection. An empty `preedit` clears the composition.
///
/// Returns false on failure; see `sb_last_error`.
///
/// # Safety
/// `host` must be null or live; `preedit` null or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_composition_update(
    host: *const SbHost,
    preedit: *const c_char,
    utf16_start: i32,
    utf16_end: i32,
) -> bool {
    guard(false, || {
        // SAFETY: forwarded caller contract.
        let Some(h) = (unsafe { host_ref(host) }) else {
            return Err(Error::InvalidArgument("a null host was passed"));
        };
        // SAFETY: forwarded caller contract.
        h.host
            .update_composition(unsafe { str_arg(preedit) }?, utf16_start, utf16_end)?;
        Ok(true)
    })
}

/// Inserts `text` and clears the preedit. Empty `text` only clears it.
///
/// Returns false on failure; see `sb_last_error`.
///
/// # Safety
/// `host` must be null or live; `text` null or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_composition_commit(
    host: *const SbHost,
    text: *const c_char,
) -> bool {
    guard(false, || {
        // SAFETY: forwarded caller contract.
        let Some(h) = (unsafe { host_ref(host) }) else {
            return Err(Error::InvalidArgument("a null host was passed"));
        };
        // SAFETY: forwarded caller contract.
        h.host.commit_composition(unsafe { str_arg(text) }?)?;
        Ok(true)
    })
}

/// The WinUI edit context opened a composition.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_ime_started(host: *const SbHost) -> bool {
    unsafe { with_host(host, |h| h.host.ime_started()) }
}

/// 1 while a WinUI composition is open.
///
/// Returns false when nothing is composing or the call failed. A failure sets
/// `sb_last_error`; a closed composition leaves it null.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_ime_composing(host: *const SbHost) -> bool {
    guard(false, || {
        // SAFETY: forwarded caller contract.
        let Some(h) = (unsafe { host_ref(host) }) else {
            return Err(Error::InvalidArgument("a null host was passed"));
        };
        Ok(h.host.ime_composing())
    })
}

/// Writes the preedit the edit context should read back. Empty when nothing is composing.
///
/// # Safety
/// `host` must be null or live. `out` must be null or valid for `out_len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_ime_text(
    host: *const SbHost,
    out: *mut c_char,
    out_len: usize,
) -> bool {
    guard(false, || {
        // SAFETY: forwarded caller contract.
        let Some(h) = (unsafe { host_ref(host) }) else {
            return Err(Error::InvalidArgument("a null host was passed"));
        };
        let text = h.host.ime_document();
        // SAFETY: forwarded caller contract.
        unsafe { write_utf8(out, out_len, &text) }?;
        Ok(true)
    })
}

/// Writes the preedit caret, in UTF-16 code units.
///
/// # Safety
/// `host` must be null or live. `start` and `end` must be null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_ime_selection(
    host: *const SbHost,
    start: *mut i32,
    end: *mut i32,
) -> bool {
    guard(false, || {
        // SAFETY: forwarded caller contract.
        let Some(h) = (unsafe { host_ref(host) }) else {
            return Err(Error::InvalidArgument("a null host was passed"));
        };
        if start.is_null() || end.is_null() {
            return Err(Error::InvalidArgument(
                "the IME selection pointers are missing",
            ));
        }
        let (sel_start, sel_end) = h.host.ime_selection();
        // SAFETY: both pointers are non-null and writable for one `i32`.
        unsafe {
            *start = sel_start;
            *end = sel_end;
        }
        Ok(true)
    })
}

/// Applies one `TextUpdating` from the WinUI edit context.
///
/// `range_start`/`range_end` and `sel_start`/`sel_end` are UTF-16 carets inside
/// the preedit. Returns false on failure; see `sb_last_error`.
///
/// # Safety
/// `host` must be null or live; `text` null or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_ime_replace(
    host: *const SbHost,
    range_start: i32,
    range_end: i32,
    text: *const c_char,
    sel_start: i32,
    sel_end: i32,
) -> bool {
    guard(false, || {
        // SAFETY: forwarded caller contract.
        let Some(h) = (unsafe { host_ref(host) }) else {
            return Err(Error::InvalidArgument("a null host was passed"));
        };
        // SAFETY: forwarded caller contract.
        h.host.ime_replace(
            range_start,
            range_end,
            unsafe { str_arg(text) }?,
            sel_start,
            sel_end,
        )?;
        Ok(true)
    })
}

/// Applies one `SelectionUpdating` from the WinUI edit context.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_ime_select(host: *const SbHost, start: i32, end: i32) -> bool {
    unsafe { with_host(host, |h| h.host.ime_select(start, end)) }
}

/// Applies `CompositionCompleted`. `canceled` drops the preedit.
///
/// # Safety
/// `host` must be null or live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_host_ime_completed(host: *const SbHost, canceled: bool) -> bool {
    unsafe { with_host(host, |h| h.host.ime_completed(canceled)) }
}

/// Writes the Slint key text for one AppKit key event into `out`.
///
/// Returns true when a key was written. Returns false when the event is not a
/// Slint key (the buffer is then an empty string and `sb_last_error` is null)
/// or the call failed (`sb_last_error` is set).
///
/// # Safety
/// `characters` and `ignoring` must be null or NUL-terminated. `out` must be
/// null or valid for `out_len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_appkit_key_text(
    key_code: u16,
    characters: *const c_char,
    ignoring: *const c_char,
    modifiers: u32,
    out: *mut c_char,
    out_len: usize,
) -> bool {
    guard(false, || {
        // SAFETY: forwarded caller contract.
        let characters = unsafe { str_arg(characters) }?;
        // SAFETY: forwarded caller contract.
        let ignoring = unsafe { str_arg(ignoring) }?;
        let text = appkit_key_text(AppKitKey {
            key_code,
            characters,
            characters_ignoring_modifiers: ignoring,
            modifiers,
        });
        // SAFETY: forwarded caller contract.
        unsafe { write_utf8(out, out_len, text.as_deref().unwrap_or("")) }?;
        Ok(text.is_some())
    })
}

/// Writes the Slint key text for a WinUI virtual key that has no character.
///
/// Returns true when a key was written. Returns false when the host should
/// wait for a character (`sb_last_error` is then null) or the call failed.
///
/// # Safety
/// `out` must be null or valid for `out_len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_virtual_key_command(
    virtual_key: u16,
    shift: bool,
    out: *mut c_char,
    out_len: usize,
) -> bool {
    guard(false, || {
        let text = virtual_key_command(virtual_key, shift);
        let written = text.map(|ch| ch.to_string());
        // SAFETY: forwarded caller contract.
        unsafe { write_utf8(out, out_len, written.as_deref().unwrap_or("")) }?;
        Ok(text.is_some())
    })
}

/// Writes the Slint key text for one WinUI key event into `out`.
///
/// `character` is the layout-produced text, or empty. `shift` and `control`
/// are the modifier state. Returns true when a key was written. Returns false
/// when the event is not a Slint key (the buffer is then empty and
/// `sb_last_error` is null) or the call failed.
///
/// # Safety
/// `character` must be null or NUL-terminated. `out` must be null or valid
/// for `out_len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_virtual_key_text(
    virtual_key: u16,
    character: *const c_char,
    shift: bool,
    control: bool,
    out: *mut c_char,
    out_len: usize,
) -> bool {
    guard(false, || {
        // SAFETY: forwarded caller contract.
        let character = unsafe { str_arg(character) }?;
        let text = virtual_key_text(VirtualKeyEvent {
            virtual_key,
            character,
            shift,
            control,
        });
        let written = text.map(|ch| ch.to_string());
        // SAFETY: forwarded caller contract.
        unsafe { write_utf8(out, out_len, written.as_deref().unwrap_or("")) }?;
        Ok(text.is_some())
    })
}

/// # Safety
/// `text` must be null or a NUL-terminated string valid for the call.
unsafe fn required_str<'a>(text: *const c_char, missing: &'static str) -> Result<&'a str, Error> {
    if text.is_null() {
        return Err(Error::InvalidArgument(missing));
    }
    // SAFETY: forwarded caller contract.
    unsafe { str_arg(text) }
}

/// Hands `text` to C as a string the caller frees with [`sb_string_free`].
fn owned_c_string(text: String) -> Result<*mut c_char, Error> {
    CString::new(text)
        .map(CString::into_raw)
        .map_err(|_| Error::InvalidArgument("the value contains an interior NUL"))
}

/// Sets the public property `name` from its JSON value (`"text"`, `42`,
/// `true`, `"#ff0000"`, `"Enum.value"`, arrays and objects for models and
/// structs). False on an unknown property, a wrong type or malformed JSON.
///
/// # Safety
/// `host` must be null or live; `name` and `json` are NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_set_property_json(
    host: *const SbHost,
    name: *const c_char,
    json: *const c_char,
) -> bool {
    // SAFETY: forwarded caller contract.
    unsafe {
        with_host(host, |h| {
            let name = required_str(name, "the property name is null")?;
            let json = required_str(json, "the property value is null")?;
            h.component
                .set_property_json(name, json)
                .map_err(Error::Component)
        })
    }
}

/// The public property `name` as JSON, or null on failure. Free with
/// [`sb_string_free`].
///
/// # Safety
/// `host` must be null or live; `name` is NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_get_property_json(
    host: *const SbHost,
    name: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        // SAFETY: forwarded caller contract.
        let h =
            unsafe { host_ref(host) }.ok_or(Error::InvalidArgument("a null host was passed"))?;
        // SAFETY: forwarded caller contract.
        let name = unsafe { required_str(name, "the property name is null") }?;
        owned_c_string(
            h.component
                .get_property_json(name)
                .map_err(Error::Component)?,
        )
    })
}

/// Invokes the public callback `name` with a JSON array of arguments and
/// returns its result as JSON (`null` for a callback without one), or null on
/// failure. Free the result with [`sb_string_free`].
///
/// # Safety
/// `host` must be null or live; `name` and `args_json` are NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_invoke_json(
    host: *const SbHost,
    name: *const c_char,
    args_json: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        // SAFETY: forwarded caller contract.
        let h =
            unsafe { host_ref(host) }.ok_or(Error::InvalidArgument("a null host was passed"))?;
        // SAFETY: forwarded caller contract.
        let name = unsafe { required_str(name, "the callback name is null") }?;
        // SAFETY: forwarded caller contract.
        let args = unsafe { required_str(args_json, "the arguments are null") }?;
        owned_c_string(
            h.component
                .invoke_json(name, args)
                .map_err(Error::Component)?,
        )
    })
}

/// Handles the public callback `name` in the host: `callback` receives
/// `user_data` and the arguments as a JSON array. A callback with a return
/// type returns its type's default to Slint. Null `callback` clears it.
/// `user_data` is passed back untouched and must stay valid until the
/// callback is replaced or the host is freed. False on an unknown callback.
///
/// # Safety
/// `host` must be null or live; `name` is NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_on_callback(
    host: *const SbHost,
    name: *const c_char,
    callback: SbCallbackFn,
    user_data: *mut c_void,
) -> bool {
    // SAFETY: forwarded caller contract.
    unsafe {
        with_host(host, |h| {
            let name = required_str(name, "the callback name is null")?;
            // Raw pointers are not `'static + Fn`-friendly; carry the address instead.
            let user_data = user_data as usize;
            let handler: runtime::JsonCallback = match callback {
                None => Box::new(|_| None),
                Some(callback) => Box::new(move |args: &str| {
                    // JSON escapes NUL, so this only fails on a bridge bug.
                    match CString::new(args) {
                        Ok(args) => callback(user_data as *mut c_void, args.as_ptr()),
                        Err(_) => set_error("callback arguments contain an interior NUL".into()),
                    }
                    None
                }),
            };
            h.component
                .set_callback_json(name, handler)
                .map_err(Error::Component)
        })
    }
}

/// Frees a string returned by [`sb_get_property_json`] or [`sb_invoke_json`]. Null is ignored.
///
/// # Safety
/// `text` is null or a string from those calls, not freed before.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sb_string_free(text: *mut c_char) {
    if !text.is_null() {
        // SAFETY: the pointer came from `CString::into_raw` and is freed once.
        drop(unsafe { CString::from_raw(text) });
    }
}

/// Runs `f` on a dedicated thread so parallel `cargo test` shares one Slint owner.
#[doc(hidden)]
pub fn run_on_test_ui_thread<R, F>(f: F) -> R
where
    R: Send + 'static,
    F: FnOnce() -> R + Send + 'static,
{
    use std::sync::mpsc::{self, Sender};
    use std::sync::{Mutex, OnceLock};

    struct Job {
        run: Box<dyn FnOnce() + Send>,
    }

    static TX: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        let builder = std::thread::Builder::new().name("sb-test-ui".into());
        match builder.spawn(move || {
            while let Ok(job) = rx.recv() {
                (job.run)();
            }
        }) {
            Ok(_) => Mutex::new(tx),
            Err(_) => std::process::abort(),
        }
    });
    let (done_tx, done_rx) = mpsc::channel();
    let guard = match tx.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if guard
        .send(Job {
            run: Box::new(move || {
                let _ = done_tx.send(f());
            }),
        })
        .is_err()
    {
        std::process::abort();
    }
    match done_rx.recv() {
        Ok(value) => value,
        Err(_) => std::process::abort(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDTH: u32 = 64;
    const HEIGHT: u32 = 48;

    /// A tiny component compiled in memory; the demo file is the e2e's job.
    fn new_host(width: u32, height: u32, scale: f32) -> *mut SbHost {
        let source = c"export component T inherits Window { background: #336699; TextInput {} }";
        // SAFETY: both strings are NUL-terminated literals.
        unsafe {
            let definition = sb_compile(source.as_ptr(), c"t.slint".as_ptr());
            assert!(!definition.is_null());
            let host = sb_component_new(definition, width, height, scale);
            sb_definition_free(definition);
            host
        }
    }

    #[test]
    fn render_through_the_c_abi_writes_a_frame() {
        run_on_test_ui_thread(|| {
            let host = new_host(WIDTH, HEIGHT, 1.0);
            assert!(!host.is_null());
            // SAFETY: `host` is live until the free below.
            unsafe {
                let mut buf = vec![0u8; sb_host_frame_len(host)];
                let frame = sb_host_render(host, buf.as_mut_ptr(), buf.len());
                assert_eq!((frame.failed, frame.redrawn), (0, 1));
                sb_host_free(host);
            }
        });
    }

    #[test]
    fn short_buffer_fails_with_a_message() {
        run_on_test_ui_thread(|| {
            let host = new_host(WIDTH, HEIGHT, 1.0);
            // SAFETY: `host` is live until the free below.
            unsafe {
                let mut buf = vec![0u8; 1];
                let frame = sb_host_render(host, buf.as_mut_ptr(), buf.len());
                assert_eq!(frame.failed, 1);
                assert!(!sb_last_error().is_null());
                sb_host_free(host);
            }
        });
    }

    #[test]
    fn null_host_is_refused_not_dereferenced() {
        run_on_test_ui_thread(|| {
            // SAFETY: null is part of every entry point's contract.
            unsafe {
                assert_eq!(sb_host_frame_len(ptr::null()), 0);
                assert!(!sb_host_pointer_moved(ptr::null(), 0.0, 0.0));
                assert!(!sb_last_error().is_null());
                assert!(new_host(WIDTH, HEIGHT, 0.0).is_null());
                assert!(!sb_last_error().is_null());
            }
        });
    }

    #[test]
    fn invalid_pointer_button_is_refused() {
        run_on_test_ui_thread(|| {
            let host = new_host(WIDTH, HEIGHT, 1.0);
            // SAFETY: `host` is live until the free below.
            unsafe {
                assert!(!sb_host_pointer_pressed(host, 0.0, 0.0, 9));
                assert!(!sb_last_error().is_null());
                sb_host_free(host);
            }
        });
    }

    #[test]
    fn invalid_utf8_key_text_is_refused() {
        run_on_test_ui_thread(|| {
            let host = new_host(WIDTH, HEIGHT, 1.0);
            let bytes = [0x80u8, 0];
            // SAFETY: `host` is live; `bytes` is a NUL-terminated invalid UTF-8 string.
            unsafe {
                assert!(!sb_host_key_pressed(host, bytes.as_ptr().cast::<c_char>()));
                assert!(!sb_last_error().is_null());
                sb_host_free(host);
            }
        });
    }

    #[test]
    fn another_thread_is_refused() {
        run_on_test_ui_thread(|| {
            let host = new_host(WIDTH, HEIGHT, 1.0);
            let addr = host as usize;
            let ok = std::thread::spawn(move || {
                // SAFETY: the pointer is not freed until this thread joins; we only
                // prove the other thread is refused, we do not touch the object.
                unsafe { sb_host_pointer_moved(addr as *const SbHost, 0.0, 0.0) }
            })
            .join()
            .unwrap_or(true);
            assert!(!ok);
            // SAFETY: `host` is live and this is the owning thread.
            unsafe { sb_host_free(host) };
        });
    }
}
