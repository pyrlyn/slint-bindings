//! Several hosts on one thread, and popups drawn inside their host's buffer.
//! Hit targets are the centers listed at the top of `ui/popups.slint`.

use std::path::Path;

use i_slint_core::window::{PopupWindowLocation, WindowInner};
use slint_interpreter::Value;

use super::*;
use crate::runtime::{self, ComponentInstance};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 200;
const TOP_COMBO: (f32, f32) = (80.0, 32.0);
const BOTTOM_COMBO: (f32, f32) = (80.0, 184.0);
const MENU_AREA: (f32, f32) = (240.0, 100.0);

struct View {
    host: EmbeddedHost<ComponentInstance>,
    frame: Vec<u8>,
}

impl View {
    fn new() -> Result<Self, Error> {
        let source = include_str!("../../ui/popups.slint");
        let definition = runtime::compile(Some(source), Path::new("popups.slint"))?;
        let host = EmbeddedHost::new(|| definition.create(), WIDTH, HEIGHT, 1.0)?;
        let frame = vec![0; host.frame_len()];
        let mut view = Self { host, frame };
        view.render()?;
        Ok(view)
    }

    /// Runs pending timers and paints; returns whether the frame changed.
    fn render(&mut self) -> Result<bool, Error> {
        slint::platform::update_timers_and_animations();
        Ok(self.host.render(&mut self.frame)?.redrawn)
    }

    fn click(&mut self, (x, y): (f32, f32), button: PointerButton) -> Result<(), Error> {
        self.host.pointer_moved(x, y)?;
        self.host.pointer_pressed(x, y, button)?;
        self.host.pointer_released(x, y, button)?;
        self.render()?;
        Ok(())
    }

    fn key(&mut self, text: &str) -> Result<(), Error> {
        let _kept = self.host.key_pressed(text)?;
        self.host.key_released(text)?;
        self.render()?;
        Ok(())
    }

    fn inner(&self) -> &WindowInner {
        WindowInner::from_pub(self.host.window.window())
    }

    /// Each open popup's rectangle in view coordinates, bottom first.
    /// `None` for a popup in a native window of its own.
    fn popups(&self) -> Vec<Option<(f32, f32, f32, f32)>> {
        self.inner()
            .active_popups()
            .iter()
            .map(|popup| match popup.location {
                PopupWindowLocation::ChildWindow(origin) => {
                    let root = i_slint_core::item_tree::ItemRc::new(popup.component.clone(), 0);
                    let size = root.geometry().size;
                    Some((origin.x, origin.y, size.width, size.height))
                }
                PopupWindowLocation::TopLevel(_) => None,
            })
            .collect()
    }

    fn string(&self, name: &str) -> String {
        match self.host.component().get_property(name) {
            Ok(Value::String(text)) => text.into(),
            other => format!("<{name}: {other:?}>"),
        }
    }

    /// Pixels that differ from `other` inside the logical rectangle.
    fn changed_in(&self, other: &[u8], (x, y, w, h): (f32, f32, f32, f32)) -> usize {
        let (x0, y0) = (x.max(0.0) as usize, y.max(0.0) as usize);
        let x1 = ((x + w) as usize).min(WIDTH as usize);
        let y1 = ((y + h) as usize).min(HEIGHT as usize);
        let mut changed = 0;
        for row in y0..y1 {
            for col in x0..x1 {
                let at = (row * WIDTH as usize + col) * BYTES_PER_PIXEL;
                if self.frame[at..at + BYTES_PER_PIXEL] != other[at..at + BYTES_PER_PIXEL] {
                    changed += 1;
                }
            }
        }
        changed
    }
}

const DOWN: &str = "\u{F701}";
const RETURN: &str = "\n";
const ESCAPE: &str = "\u{1b}";

fn inside_the_frame((x, y, w, h): (f32, f32, f32, f32)) -> bool {
    x >= 0.0 && y >= 0.0 && x + w <= WIDTH as f32 && y + h <= HEIGHT as f32
}

fn whole_frame() -> (f32, f32, f32, f32) {
    (0.0, 0.0, WIDTH as f32, HEIGHT as f32)
}

/// The single open popup's rectangle; fails if it is not a child window.
fn only_popup(view: &View) -> Result<(f32, f32, f32, f32), Error> {
    let popups = view.popups();
    match popups[..] {
        [Some(rect)] => Ok(rect),
        _ => Err(Error::Component(format!(
            "expected one popup inside the view, got {popups:?}"
        ))),
    }
}

#[test]
fn combo_popup_draws_inside_the_view_without_a_window() -> Result<(), Error> {
    let mut view = View::new()?;
    let before = view.frame.clone();
    view.click(TOP_COMBO, PointerButton::Left)?;
    let rect = only_popup(&view)?;
    assert!(
        platform::take_created_window().is_none(),
        "the popup asked the platform for a window"
    );
    assert!(inside_the_frame(rect), "{rect:?}");
    let inside = view.changed_in(&before, rect);
    assert!(inside > 0, "the popup left no pixels in the frame");
    assert_eq!(
        view.changed_in(&before, whole_frame()),
        inside,
        "pixels outside the popup changed"
    );
    Ok(())
}

#[test]
fn bottom_popup_is_moved_up_into_the_view() -> Result<(), Error> {
    let mut view = View::new()?;
    view.click(BOTTOM_COMBO, PointerButton::Left)?;
    let rect = only_popup(&view)?;
    assert!(inside_the_frame(rect), "{rect:?}");
    assert!(
        rect.1 < 168.0,
        "the popup was not moved above the combo box"
    );
    view.key(DOWN)?;
    view.key(RETURN)?;
    assert_eq!(view.string("size"), "Medium");
    assert!(view.popups().is_empty());
    Ok(())
}

#[test]
fn context_menu_is_kept_inside_the_view() -> Result<(), Error> {
    let mut view = View::new()?;
    view.click(MENU_AREA, PointerButton::Right)?;
    let rect = only_popup(&view)?;
    assert!(inside_the_frame(rect), "{rect:?}");
    // Opened at the pointer, then shifted left: it is wider than the room on the right.
    assert!(rect.0 < MENU_AREA.0, "{rect:?}");
    view.key(DOWN)?;
    view.key(RETURN)?;
    assert_eq!(view.string("action"), "copy");
    assert!(view.popups().is_empty());
    Ok(())
}

#[test]
fn closing_a_popup_repaints_what_it_covered() -> Result<(), Error> {
    let mut view = View::new()?;
    // The first round leaves the combo box focused; compare from there.
    view.click(TOP_COMBO, PointerButton::Left)?;
    view.key(ESCAPE)?;
    let closed = view.frame.clone();
    view.click(TOP_COMBO, PointerButton::Left)?;
    assert_ne!(view.frame, closed);
    view.key(ESCAPE)?;
    assert!(view.popups().is_empty());
    assert_eq!(view.changed_in(&closed, whole_frame()), 0);
    // A full repaint of the same scene matches the partial one.
    view.host.resize(WIDTH + 1, HEIGHT, 1.0)?;
    view.host.resize(WIDTH, HEIGHT, 1.0)?;
    view.frame = vec![0; view.host.frame_len()];
    assert!(view.render()?);
    assert_eq!(view.changed_in(&closed, whole_frame()), 0);
    Ok(())
}

#[test]
fn focus_loss_closes_popups_that_close_from_outside() -> Result<(), Error> {
    let source = r#"
export component Pinned inherits Window {
    callback open-pinned();
    callback open-loose();
    width: 100px;
    height: 100px;
    // Separate parents: showing a popup closes its siblings under the same item.
    Rectangle {
        pinned := PopupWindow {
            close-policy: no-auto-close;
            width: 40px;
            height: 40px;
            Rectangle { background: red; }
        }
    }
    Rectangle {
        loose := PopupWindow {
            x: 50px;
            width: 40px;
            height: 40px;
            Rectangle { background: blue; }
        }
    }
    open-pinned => { pinned.show(); }
    open-loose => { loose.show(); }
}
"#;
    let definition = runtime::compile(Some(source), Path::new("pinned.slint"))?;
    let host = EmbeddedHost::new(|| definition.create(), 100, 100, 1.0)?;
    let open = |name: &str| host.component().invoke(name, &[]).map(drop);
    let count = || {
        WindowInner::from_pub(host.window.window())
            .active_popups()
            .len()
    };
    open("open-pinned").map_err(|err| Error::Component(err.to_string()))?;
    open("open-loose").map_err(|err| Error::Component(err.to_string()))?;
    assert_eq!(count(), 2);
    host.focus_changed(true)?;
    assert_eq!(count(), 2);
    host.focus_changed(false)?;
    assert_eq!(count(), 1, "only the no-auto-close popup stays");
    Ok(())
}

#[test]
fn two_views_on_one_thread_keep_their_own_popups_and_state() -> Result<(), Error> {
    let mut left = View::new()?;
    let mut right = View::new()?;
    left.host.focus_changed(true)?;
    left.click(TOP_COMBO, PointerButton::Left)?;
    assert_eq!(left.popups().len(), 1);
    assert!(right.popups().is_empty());
    assert!(!right.render()?, "a popup in one view repainted the other");

    right.host.focus_changed(true)?;
    right.click(BOTTOM_COMBO, PointerButton::Left)?;
    // The host moves focus between views; only the view that lost it closes.
    left.host.focus_changed(false)?;
    assert!(left.popups().is_empty());
    assert_eq!(right.popups().len(), 1);

    right.key(DOWN)?;
    right.key(RETURN)?;
    assert_eq!(right.string("size"), "Medium");
    assert_eq!(left.string("size"), "Small");
    assert_eq!(left.string("fruit"), "Apple");
    Ok(())
}
