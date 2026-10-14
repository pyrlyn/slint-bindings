//! One embedded component and the pixel buffer it renders into.
//!
//! The host drives everything: size, scale, input, timers and frames. Nothing
//! here blocks or spins a loop, so it fits any native run loop (AppKit's main
//! thread, the WinUI dispatcher) as long as every call comes from that thread.

use std::cell::{Cell, RefCell};
use std::mem::{align_of, offset_of, size_of};
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{PointerEventButton, WindowAdapter, WindowEvent, WindowEventDispatchResult};
use slint::{ComponentHandle, LogicalPosition, LogicalSize, PhysicalSize, SharedString};

use crate::ime::{ImeSession, ImeUpdate};
use crate::keys::utf16_selection_to_utf8;

use crate::{Error, platform};

/// Bytes per pixel in the RGBA8 frames handed to the host.
pub const BYTES_PER_PIXEL: usize = 4;

/// Two vsync callbacks on the same thread closer than this share one timer tick.
const TICK_COALESCE: Duration = Duration::from_millis(1);

thread_local! {
    static LAST_TICK: Cell<Option<Instant>> = const { Cell::new(None) };
}

const TRANSPARENT: PremultipliedRgbaColor = PremultipliedRgbaColor {
    red: 0,
    green: 0,
    blue: 0,
    alpha: 0,
};

// `PremultipliedRgbaColor` is `#[repr(C)]` RGBA8. These fail the build if a
// future Slint release reorders the fields, which would make the frame copy
// below write the wrong channels.
const _: () = {
    assert!(size_of::<PremultipliedRgbaColor>() == BYTES_PER_PIXEL);
    assert!(align_of::<PremultipliedRgbaColor>() == 1);
    assert!(offset_of!(PremultipliedRgbaColor, red) == 0);
    assert!(offset_of!(PremultipliedRgbaColor, green) == 1);
    assert!(offset_of!(PremultipliedRgbaColor, blue) == 2);
    assert!(offset_of!(PremultipliedRgbaColor, alpha) == 3);
};

pub(crate) fn check_scale(scale: f32) -> Result<(), Error> {
    if scale.is_finite() && scale > 0.0 {
        Ok(())
    } else {
        Err(Error::BadScale { scale })
    }
}

/// Mouse buttons the host can forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PointerButton {
    /// Primary button.
    Left = 0,
    /// Secondary button.
    Right = 1,
    /// Wheel button.
    Middle = 2,
}

impl From<PointerButton> for PointerEventButton {
    fn from(button: PointerButton) -> Self {
        match button {
            PointerButton::Left => Self::Left,
            PointerButton::Right => Self::Right,
            PointerButton::Middle => Self::Middle,
        }
    }
}

impl TryFrom<u8> for PointerButton {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Left),
            1 => Ok(Self::Right),
            2 => Ok(Self::Middle),
            _ => Err(Error::InvalidArgument(
                "pointer button must be 0 (left), 1 (right) or 2 (middle)",
            )),
        }
    }
}

/// What a call to [`EmbeddedHost::render`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    /// The buffer was written; the host must present it.
    pub redrawn: bool,
    /// Animations are running; the host should ask for another frame on its next vsync.
    pub animating: bool,
}

/// A Slint component rendered into a host-owned RGBA8 buffer.
pub struct EmbeddedHost<C: ComponentHandle> {
    component: C,
    window: Rc<MinimalSoftwareWindow>,
    pixels: Vec<PremultipliedRgbaColor>,
    width: u32,
    height: u32,
    scale: f32,
    /// The pixel storage was replaced. The reused-buffer renderer would otherwise
    /// repaint only dirty items and leave the rest of the new buffer transparent.
    full_repaint: bool,
    /// WinUI IME preedit. Input methods take `&self` because the window
    /// adapter already mutates through interior cells; the preedit does too.
    ime: RefCell<ImeSession>,
}

impl<C: ComponentHandle> EmbeddedHost<C> {
    /// Creates the component through `make` and sizes it to `width`×`height`
    /// physical pixels at `scale` physical pixels per logical point.
    pub fn new(
        make: impl FnOnce() -> Result<C, slint::PlatformError>,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<Self, Error> {
        check_scale(scale)?;
        platform::ensure_installed()?;
        // A window left over from a component that failed half-way is not ours.
        drop(platform::take_created_window());
        let component = make()?;
        // Window adapters are created lazily; asking for the window forces it.
        let _ = component.window();
        let window = platform::take_created_window().ok_or(Error::NoWindow)?;
        component.show()?;
        let mut host = Self {
            component,
            window,
            pixels: Vec::new(),
            width: 0,
            height: 0,
            scale,
            full_repaint: true,
            ime: RefCell::new(ImeSession::default()),
        };
        host.resize(width, height, scale)?;
        Ok(host)
    }

    /// The component, for its generated property and callback accessors.
    pub fn component(&self) -> &C {
        &self.component
    }

    /// Current frame size in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Bytes a buffer passed to [`Self::render`] must hold.
    pub fn frame_len(&self) -> usize {
        self.width as usize * self.height as usize * BYTES_PER_PIXEL
    }

    /// Resizes the frame. A zero dimension is clamped to one pixel, because
    /// hosts report zero-sized views during layout and Slint cannot render them.
    pub fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<(), Error> {
        check_scale(scale)?;
        // Hosts report 0×0 while a view is being laid out. Slint cannot render that.
        let (width, height) = (width.max(1), height.max(1));
        // SwiftUI calls this on every layout pass. Reallocating the reused buffer
        // when nothing changed throws away the previous frame and forces a full paint.
        if width == self.width && height == self.height && scale == self.scale {
            return Ok(());
        }
        if scale != self.scale || self.width == 0 {
            self.send(WindowEvent::ScaleFactorChanged {
                scale_factor: scale,
            })?;
        }
        self.scale = scale;
        self.width = width;
        self.height = height;
        self.window.set_size(PhysicalSize::new(width, height));
        self.send(WindowEvent::Resized {
            size: LogicalSize::new(width as f32 / scale, height as f32 / scale),
        })?;
        let len = width as usize * height as usize;
        if self.pixels.len() == len {
            // Same pixel count (scale-only change): drop stale samples, keep the allocation.
            self.pixels.fill(TRANSPARENT);
        } else {
            self.pixels = vec![TRANSPARENT; len];
        }
        self.full_repaint = true;
        self.window.request_redraw();
        Ok(())
    }

    /// Renders into `out` (premultiplied RGBA8, rows top to bottom, no padding)
    /// if anything changed. When nothing changed `out` is left untouched, so the
    /// host keeps presenting the previous frame.
    pub fn render(&mut self, out: &mut [u8]) -> Result<Frame, Error> {
        self.render_pixels(out, false)
    }

    /// Like [`Self::render`], but the bytes are premultiplied BGRA8.
    ///
    /// WinUI `WriteableBitmap` and DXGI swap chains store that order. Swapping
    /// here keeps the C# host from walking the frame a second time.
    pub fn render_bgra(&mut self, out: &mut [u8]) -> Result<Frame, Error> {
        self.render_pixels(out, true)
    }

    fn render_pixels(&mut self, out: &mut [u8], bgra: bool) -> Result<Frame, Error> {
        let needed = self.frame_len();
        if out.len() < needed {
            return Err(Error::BufferTooSmall {
                got: out.len(),
                needed,
                width: self.width,
                height: self.height,
            });
        }
        let stride = self.width as usize;
        let full_repaint = self.full_repaint;
        let pixels = &mut self.pixels;
        let redrawn = self.window.draw_if_needed(|renderer| {
            // NewBuffer paints every pixel. Switching back afterwards keeps later
            // frames partial, which is what the reused buffer is for.
            if full_repaint {
                renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
            }
            renderer.render(pixels, stride);
            if full_repaint {
                renderer.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
            }
        });
        if redrawn {
            self.full_repaint = false;
        }
        if redrawn {
            let len = self.pixels.len() * BYTES_PER_PIXEL;
            if len != needed {
                return Err(Error::InvalidArgument(
                    "internal frame size does not match the pixel buffer",
                ));
            }
            if bgra {
                write_frame_bgra(out, &self.pixels);
            } else {
                write_frame(out, &self.pixels);
            }
        }
        Ok(Frame {
            redrawn,
            animating: self.window.has_active_animations(),
        })
    }

    /// Advances timers and animations. Call once per display refresh.
    ///
    /// Timers are thread-global. Two host views often fire their vsync
    /// callbacks back-to-back; a second call within [`TICK_COALESCE`] is ignored
    /// so animations do not run twice as fast.
    pub fn tick() {
        LAST_TICK.with(|slot| {
            let now = Instant::now();
            if let Some(prev) = slot.get()
                && now.saturating_duration_since(prev) < TICK_COALESCE
            {
                return;
            }
            slot.set(Some(now));
            slint::platform::update_timers_and_animations();
        });
    }

    /// Pointer moved to `x`,`y` logical points from the top-left corner.
    pub fn pointer_moved(&self, x: f32, y: f32) -> Result<(), Error> {
        self.send(WindowEvent::PointerMoved {
            position: LogicalPosition::new(x, y),
        })
    }

    /// Pointer button went down.
    pub fn pointer_pressed(&self, x: f32, y: f32, button: PointerButton) -> Result<(), Error> {
        let position = LogicalPosition::new(x, y);
        self.send(WindowEvent::PointerPressed {
            position,
            button: button.into(),
        })
    }

    /// Pointer button went up.
    pub fn pointer_released(&self, x: f32, y: f32, button: PointerButton) -> Result<(), Error> {
        let position = LogicalPosition::new(x, y);
        self.send(WindowEvent::PointerReleased {
            position,
            button: button.into(),
        })
    }

    /// Pointer left the view.
    pub fn pointer_exited(&self) -> Result<(), Error> {
        self.send(WindowEvent::PointerExited)
    }

    /// Scroll wheel or trackpad delta, in logical points.
    pub fn pointer_scrolled(&self, x: f32, y: f32, dx: f32, dy: f32) -> Result<(), Error> {
        let position = LogicalPosition::new(x, y);
        self.send(WindowEvent::PointerScrolled {
            position,
            delta_x: dx,
            delta_y: dy,
        })
    }

    /// A key went down. Returns whether the view kept the key.
    ///
    /// `false` with no error means the host should keep handling it. Tab and
    /// Backtab return `false` when the focus chain has no further item in that
    /// direction (including the step that would wrap around), so the host can
    /// move focus out of the view.
    pub fn key_pressed(&self, text: &str) -> Result<bool, Error> {
        if text == "\t" || text == "\u{19}" {
            return move_focus_inside(&self.window, text == "\t");
        }
        dispatch_window(
            &self.window,
            WindowEvent::KeyPressed {
                text: SharedString::from(text),
            },
        )
    }

    /// A key went up. A release the scene ignores is still a successful call.
    pub fn key_released(&self, text: &str) -> Result<(), Error> {
        self.send(WindowEvent::KeyReleased {
            text: SharedString::from(text),
        })
    }

    /// Replaces the input method's preedit.
    ///
    /// `utf16_start` and `utf16_end` select inside `preedit` in UTF-16 code
    /// units, which is what AppKit's `NSRange` counts. A negative start means
    /// the input method gave no selection (`NSNotFound`). An empty `preedit`
    /// clears the composition without inserting text.
    pub fn update_composition(
        &self,
        preedit: &str,
        utf16_start: i32,
        utf16_end: i32,
    ) -> Result<(), Error> {
        self.dispatch_composition(
            i_slint_core::input::KeyEventType::UpdateComposition,
            "",
            preedit,
            utf16_selection_to_utf8(preedit, utf16_start, utf16_end),
        )
    }

    /// A held key auto-repeated. Distinct from [`Self::key_pressed`].
    pub fn key_repeated(&self, text: &str) -> Result<(), Error> {
        self.send(WindowEvent::KeyPressRepeated {
            text: SharedString::from(text),
        })
    }

    /// Inserts `text` and clears the preedit. This is the commit half of an
    /// input method: the marked text becomes the final string (a kana, a
    /// kanji, an emoji, a dead-key combination).
    pub fn commit_composition(&self, text: &str) -> Result<(), Error> {
        self.dispatch_composition(
            i_slint_core::input::KeyEventType::CommitComposition,
            text,
            "",
            None,
        )
    }

    /// The WinUI edit context opened a composition.
    pub fn ime_started(&self) -> Result<(), Error> {
        self.ime.borrow_mut().started();
        Ok(())
    }

    /// True while a WinUI composition is open.
    pub fn ime_composing(&self) -> bool {
        self.ime.borrow().composing()
    }

    /// The preedit the edit context should read back. Empty when nothing is composing.
    pub fn ime_document(&self) -> String {
        self.ime.borrow().document().to_owned()
    }

    /// Caret inside the preedit, in UTF-16 code units. `(0, 0)` when nothing is composing.
    pub fn ime_selection(&self) -> (i32, i32) {
        self.ime.borrow().selection()
    }

    /// Apply one `TextUpdating` from `CoreTextEditContext`.
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

    /// Apply one `SelectionUpdating` from `CoreTextEditContext`.
    pub fn ime_select(&self, start: i32, end: i32) -> Result<(), Error> {
        let update = self.ime.borrow_mut().set_selection(start, end);
        self.apply_ime(update)
    }

    /// Apply `CompositionCompleted`. `canceled` drops the preedit.
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

    /// The host view gained or lost keyboard focus.
    pub fn focus_changed(&self, focused: bool) -> Result<(), Error> {
        self.send(WindowEvent::WindowActiveChanged(focused))
    }

    fn send(&self, event: WindowEvent) -> Result<(), Error> {
        send_window(&self.window, event)
    }

    fn dispatch_composition(
        &self,
        event_type: i_slint_core::input::KeyEventType,
        text: &str,
        preedit: &str,
        selection: Option<std::ops::Range<i32>>,
    ) -> Result<(), Error> {
        dispatch_composition_window(&self.window, event_type, text, preedit, selection)
    }
}

/// Delivers `event`. `true` when the scene accepted it.
pub(crate) fn dispatch_window(window: &slint::Window, event: WindowEvent) -> Result<bool, Error> {
    Ok(window.dispatch_event_with_result(event)? == WindowEventDispatchResult::Accepted)
}

/// Delivers `event` and ignores whether the scene accepted it.
pub(crate) fn send_window(window: &slint::Window, event: WindowEvent) -> Result<(), Error> {
    let _accepted = dispatch_window(window, event)?;
    Ok(())
}

/// Moves focus to the next or previous item inside `window`.
///
/// Slint's own Tab handling always reports the key as accepted, and it
/// wraps from the last item back to the first. A host that owns the rest
/// of the window needs the wrap (and a chain with nowhere to go) reported
/// as "not kept", with focus left on the item that had it.
pub(crate) fn move_focus_inside(window: &slint::Window, forward: bool) -> Result<bool, Error> {
    let inner = i_slint_core::window::WindowInner::from_pub(window);
    let before = inner.focus_item.borrow().upgrade();
    let text = if forward { "\t" } else { "\u{19}" };
    let _accepted = dispatch_window(
        window,
        WindowEvent::KeyPressed {
            text: SharedString::from(text),
        },
    )?;
    let after = inner.focus_item.borrow().upgrade();
    let kept = match (&before, &after) {
        (None, Some(_)) => true,
        (Some(before), Some(after)) if before != after => {
            let wrapped = if forward {
                focus_ordinal(after) <= focus_ordinal(before)
            } else {
                focus_ordinal(after) >= focus_ordinal(before)
            };
            if wrapped {
                if forward {
                    inner.focus_previous_item();
                } else {
                    inner.focus_next_item();
                }
            }
            !wrapped
        }
        _ => false,
    };
    Ok(kept)
}

pub(crate) fn dispatch_composition_window(
    window: &slint::Window,
    event_type: i_slint_core::input::KeyEventType,
    text: &str,
    preedit: &str,
    selection: Option<std::ops::Range<i32>>,
) -> Result<(), Error> {
    // Composition is not a public `WindowEvent`. Slint's own backends
    // deliver it through the same hidden internal event, which is what
    // makes a preedit show up in a `TextInput` without being committed.
    let mut key_event = i_slint_core::input::KeyEvent::default();
    key_event.text = SharedString::from(text);
    let event = i_slint_core::input::InternalKeyEvent {
        key_event,
        event_type,
        preedit_text: SharedString::from(preedit),
        preedit_selection: selection,
        ..Default::default()
    };
    let _accepted = dispatch_window(window, WindowEvent::internal(event))?;
    Ok(())
}

/// Steps of `next_focus_item` from the component root until `item`.
/// Later controls have a higher ordinal, so a forward Tab that lands on a
/// lower ordinal wrapped around the chain.
fn focus_ordinal(item: &i_slint_core::item_tree::ItemRc) -> usize {
    let mut cursor = i_slint_core::item_tree::ItemRc::new(item.item_tree().clone(), 0);
    let start = cursor.clone();
    for step in 0..4096 {
        if cursor == *item {
            return step;
        }
        cursor = cursor.next_focus_item();
        if cursor == start {
            break;
        }
    }
    usize::MAX
}

/// Copies packed RGBA8 into `out`. `out` may be longer than one frame; only the frame is written.
fn write_frame(out: &mut [u8], pixels: &[PremultipliedRgbaColor]) {
    let len = pixels.len() * BYTES_PER_PIXEL;
    let Some(dst) = out.get_mut(..len) else {
        return;
    };
    // SAFETY: the const asserts above pin `PremultipliedRgbaColor` as packed RGBA8
    // (`repr(C)`, 4 bytes, align 1), so this slice is the pixel bytes and nothing past them.
    let src = unsafe { std::slice::from_raw_parts(pixels.as_ptr().cast::<u8>(), len) };
    dst.copy_from_slice(src);
}

/// Copies packed BGRA8 into `out`. Same layout rules as [`write_frame`].
fn write_frame_bgra(out: &mut [u8], pixels: &[PremultipliedRgbaColor]) {
    let len = pixels.len() * BYTES_PER_PIXEL;
    let Some(dst) = out.get_mut(..len) else {
        return;
    };
    let (slots, _) = dst.as_chunks_mut::<BYTES_PER_PIXEL>();
    for (px, slot) in pixels.iter().zip(slots) {
        *slot = [px.blue, px.green, px.red, px.alpha];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{self, ComponentInstance};
    use slint_interpreter::Value;

    const WIDTH: u32 = 320;
    const HEIGHT: u32 = 200;

    type DemoForm = ComponentInstance;

    /// The accessors the compiled demo used to generate, over the interpreter.
    trait DemoName {
        fn get_name(&self) -> SharedString;
        fn set_name(&self, name: SharedString);
    }

    impl DemoName for ComponentInstance {
        fn get_name(&self) -> SharedString {
            match self.get_property("name") {
                Ok(Value::String(name)) => name,
                // Never equal to a name a test expects.
                _ => "<no name property>".into(),
            }
        }
        fn set_name(&self, name: SharedString) {
            let _ = self.set_property("name", Value::String(name));
        }
    }

    fn demo() -> Result<EmbeddedHost<DemoForm>, Error> {
        let source = include_str!("../ui/demo.slint");
        let definition = runtime::compile(Some(source), std::path::Path::new("demo.slint"))?;
        EmbeddedHost::new(|| definition.create(), WIDTH, HEIGHT, 1.0)
    }

    #[test]
    fn first_frame_paints_visible_pixels() -> Result<(), Error> {
        let mut host = demo()?;
        let mut buf = vec![0u8; host.frame_len()];
        let frame = host.render(&mut buf)?;
        assert!(frame.redrawn);
        assert!(
            buf.as_chunks::<BYTES_PER_PIXEL>()
                .0
                .iter()
                .any(|px| px[3] > 0)
        );
        Ok(())
    }

    #[test]
    fn unchanged_scene_is_not_redrawn() -> Result<(), Error> {
        let mut host = demo()?;
        let mut buf = vec![0u8; host.frame_len()];
        host.render(&mut buf)?;
        assert!(!host.render(&mut buf)?.redrawn);
        Ok(())
    }

    #[test]
    fn short_buffer_is_rejected_not_overrun() -> Result<(), Error> {
        let mut host = demo()?;
        let mut buf = vec![0u8; host.frame_len() - 1];
        assert!(matches!(
            host.render(&mut buf),
            Err(Error::BufferTooSmall { .. })
        ));
        Ok(())
    }

    #[test]
    fn resize_changes_frame_len_and_forces_redraw() -> Result<(), Error> {
        let mut host = demo()?;
        let mut buf = vec![0u8; host.frame_len()];
        host.render(&mut buf)?;
        host.resize(WIDTH * 2, HEIGHT * 2, 2.0)?;
        assert_eq!(
            host.frame_len(),
            (WIDTH * 2 * HEIGHT * 2) as usize * BYTES_PER_PIXEL
        );
        let mut buf = vec![0u8; host.frame_len()];
        assert!(host.render(&mut buf)?.redrawn);
        assert!(
            buf.as_chunks::<BYTES_PER_PIXEL>()
                .0
                .iter()
                .any(|px| px[3] > 0)
        );
        Ok(())
    }

    #[test]
    fn zero_size_from_layout_is_clamped() -> Result<(), Error> {
        let mut host = demo()?;
        host.resize(0, 0, 1.0)?;
        assert_eq!(host.size(), (1, 1));
        Ok(())
    }

    #[test]
    fn property_set_from_host_reaches_the_component() -> Result<(), Error> {
        let host = demo()?;
        host.component().set_name("Ivan".into());
        assert_eq!(host.component().get_name(), "Ivan");
        Ok(())
    }

    #[test]
    fn same_size_resize_keeps_the_painted_frame() -> Result<(), Error> {
        let mut host = demo()?;
        let mut buf = vec![0u8; host.frame_len()];
        host.render(&mut buf)?;
        host.resize(WIDTH, HEIGHT, 1.0)?;
        assert!(!host.render(&mut buf)?.redrawn);
        Ok(())
    }

    fn press(host: &EmbeddedHost<DemoForm>, x: f32, y: f32) -> Result<(), Error> {
        host.pointer_moved(x, y)?;
        host.pointer_pressed(x, y, PointerButton::Left)?;
        host.pointer_released(x, y, PointerButton::Left)?;
        Ok(())
    }

    #[test]
    fn preedit_stays_uncommitted_until_the_input_method_commits() -> Result<(), Error> {
        let host = demo()?;
        host.focus_changed(true)?;
        press(&host, 160.0, 64.0)?;
        // One hiragana: the reading, not the committed text.
        host.update_composition("あ", 0, 1)?;
        assert_eq!(host.component().get_name().as_str(), "");
        host.commit_composition("日本語")?;
        assert_eq!(host.component().get_name().as_str(), "日本語");
        Ok(())
    }

    #[test]
    fn tab_leaves_the_view_at_either_end_of_the_focus_chain() -> Result<(), Error> {
        let host = demo()?;
        host.focus_changed(true)?;
        press(&host, 160.0, 64.0)?;
        // Forward from the field reaches the button, so the view keeps Tab
        // and a following letter does not change the name.
        assert!(host.key_pressed("\t")?);
        host.key_released("\t")?;
        let _ = host.key_pressed("Z")?;
        host.key_released("Z")?;
        assert_eq!(host.component().get_name().as_str(), "");

        // A second Tab would wrap to the field. The view does not keep it,
        // and the letter stays out of the name.
        let inner = i_slint_core::window::WindowInner::from_pub(host.window.window());
        let before = inner
            .focus_item
            .borrow()
            .upgrade()
            .as_ref()
            .map(focus_ordinal);
        let kept = host.key_pressed("\t")?;
        let after = inner
            .focus_item
            .borrow()
            .upgrade()
            .as_ref()
            .map(focus_ordinal);
        assert!(
            !kept,
            "kept the wrapping tab; ordinals {before:?} -> {after:?}"
        );
        assert_eq!(before, after, "undo left focus on a different item");
        host.key_released("\t")?;
        let _ = host.key_pressed("Q")?;
        host.key_released("Q")?;
        assert_eq!(host.component().get_name().as_str(), "");

        let host = demo()?;
        host.focus_changed(true)?;
        press(&host, 160.0, 64.0)?;
        // Backtab from the first control would wrap to the button, so the
        // view does not keep it and the letter still reaches the field.
        let inner = i_slint_core::window::WindowInner::from_pub(host.window.window());
        let before = inner
            .focus_item
            .borrow()
            .upgrade()
            .as_ref()
            .map(focus_ordinal);
        assert!(!host.key_pressed("\u{19}")?);
        let after = inner
            .focus_item
            .borrow()
            .upgrade()
            .as_ref()
            .map(focus_ordinal);
        assert_eq!(before, after, "undo left focus on a different item");
        let _ = host.key_pressed("Q")?;
        host.key_released("Q")?;
        assert_eq!(host.component().get_name().as_str(), "Q");
        Ok(())
    }

    #[test]
    fn dead_key_commit_inserts_the_composed_character() -> Result<(), Error> {
        let host = demo()?;
        host.focus_changed(true)?;
        press(&host, 160.0, 64.0)?;
        host.update_composition("´", -1, -1)?;
        assert_eq!(host.component().get_name().as_str(), "");
        host.commit_composition("é")?;
        assert_eq!(host.component().get_name().as_str(), "é");
        Ok(())
    }

    #[test]
    fn pointer_button_rejects_out_of_range() {
        assert!(PointerButton::try_from(0).is_ok());
        assert!(PointerButton::try_from(2).is_ok());
        assert!(matches!(
            PointerButton::try_from(3),
            Err(Error::InvalidArgument(_))
        ));
    }

    #[test]
    fn tick_twice_in_a_row_does_not_panic() -> Result<(), Error> {
        let _host = demo()?;
        EmbeddedHost::<DemoForm>::tick();
        EmbeddedHost::<DemoForm>::tick();
        Ok(())
    }

    #[test]
    fn bgra_frame_swaps_red_and_blue() -> Result<(), Error> {
        let mut rgba_host = demo()?;
        let mut bgra_host = demo()?;
        let mut rgba = vec![0u8; rgba_host.frame_len()];
        let mut bgra = vec![0u8; bgra_host.frame_len()];
        assert!(rgba_host.render(&mut rgba)?.redrawn);
        assert!(bgra_host.render_bgra(&mut bgra)?.redrawn);
        let mut differs = false;
        let (rgba_px, _) = rgba.as_chunks::<4>();
        let (bgra_px, _) = bgra.as_chunks::<4>();
        for (px, qx) in rgba_px.iter().zip(bgra_px) {
            assert_eq!(px[0], qx[2]);
            assert_eq!(px[1], qx[1]);
            assert_eq!(px[2], qx[0]);
            assert_eq!(px[3], qx[3]);
            if px[0] != px[2] {
                differs = true;
            }
        }
        assert!(
            differs,
            "the frame never put red and blue on different values"
        );
        Ok(())
    }

    #[test]
    fn windows_ime_commits_a_preedit_and_inserts_a_direct_character() -> Result<(), Error> {
        let host = demo()?;
        host.focus_changed(true)?;
        press(&host, 160.0, 64.0)?;
        host.ime_started()?;
        host.ime_replace(0, 0, "あ", 0, 1)?;
        assert_eq!(host.ime_document(), "あ");
        assert!(host.ime_composing());
        assert_eq!(host.component().get_name().as_str(), "");
        host.ime_replace(0, 1, "日", 0, 1)?;
        host.ime_completed(false)?;
        assert_eq!(host.component().get_name().as_str(), "日");
        assert!(!host.ime_composing());
        assert_eq!(host.ime_document(), "");

        host.ime_replace(0, 0, "A", 1, 1)?;
        assert_eq!(host.component().get_name().as_str(), "日A");

        host.ime_started()?;
        host.ime_replace(0, 0, "か", 0, 1)?;
        host.ime_completed(true)?;
        assert_eq!(host.component().get_name().as_str(), "日A");
        Ok(())
    }

    #[test]
    fn non_positive_scale_is_rejected_and_the_frame_stays() -> Result<(), Error> {
        let mut host = demo()?;
        assert!(matches!(
            host.resize(WIDTH, HEIGHT, 0.0),
            Err(Error::BadScale { .. })
        ));
        assert!(matches!(
            host.resize(WIDTH, HEIGHT, f32::NAN),
            Err(Error::BadScale { .. })
        ));
        assert_eq!(host.size(), (WIDTH, HEIGHT));
        Ok(())
    }
}
