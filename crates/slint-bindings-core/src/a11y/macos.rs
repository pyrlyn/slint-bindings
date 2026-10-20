//! The AccessKit tree inside a host `NSView`.
//!
//! `accesskit_macos::Adapter` rather than its `SubclassingAdapter`: the host
//! view is a Swift class it owns, so it overrides the three `NSAccessibility`
//! entry points and forwards them here. Dynamic subclassing would swap the
//! class of a Swift object under it.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::time::Duration;

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use accesskit_macos::{Adapter, NSPoint, QueuedEvents};
use slint::ComponentHandle;

use super::{AccessTree, perform};

/// One host view's accessibility adapter.
pub struct MacAccessibility {
    adapter: Adapter,
    tree: Rc<RefCell<AccessTree>>,
}

impl MacAccessibility {
    /// Attaches to `view`. Must run on the main thread; AccessKit checks it.
    ///
    /// # Safety
    /// `view` is a live `NSView`. The adapter keeps only a weak reference.
    pub unsafe fn new(view: *mut c_void, view_focused: bool) -> Self {
        let tree = Rc::new(RefCell::new(AccessTree::default()));
        let actions = Actions {
            tree: Rc::clone(&tree),
        };
        // SAFETY: forwarded caller contract.
        let adapter = unsafe { Adapter::new(view, view_focused, actions) };
        Self { adapter, tree }
    }

    /// `accessibilityChildren` of the view: an autoreleased `NSArray`.
    pub fn children(&mut self, component: &impl ComponentHandle) -> *mut c_void {
        let mut activation = Activation {
            tree: &self.tree,
            window: component.window(),
        };
        self.adapter.view_children(&mut activation).cast()
    }

    /// `accessibilityFocusedUIElement` of the view: autoreleased, or null.
    pub fn focus(&mut self, component: &impl ComponentHandle) -> *mut c_void {
        let mut activation = Activation {
            tree: &self.tree,
            window: component.window(),
        };
        self.adapter.focus(&mut activation).cast()
    }

    /// `accessibilityHitTest:` of the view; `x`, `y` are screen coordinates.
    pub fn hit_test(&mut self, component: &impl ComponentHandle, x: f64, y: f64) -> *mut c_void {
        let mut activation = Activation {
            tree: &self.tree,
            window: component.window(),
        };
        self.adapter
            .hit_test(NSPoint::new(x, y), &mut activation)
            .cast()
    }

    /// A new tree after a frame, if a screen reader is listening and anything
    /// it could see changed.
    pub fn frame_done(&mut self, component: &impl ComponentHandle, redrawn: bool) -> Notifications {
        let window = component.window();
        let mut tree = self.tree.borrow_mut();
        if !tree.is_stale(window, redrawn) {
            return Notifications(None);
        }
        // Built only while a screen reader is listening.
        Notifications(self.adapter.update_if_active(|| tree.build(window)))
    }

    /// The host view gained or lost first responder.
    pub fn view_focus_changed(&mut self, focused: bool) -> Notifications {
        Notifications(self.adapter.update_view_focus_state(focused))
    }
}

/// `NSAccessibility` notifications for a change. Raise them once the adapter
/// is no longer borrowed: VoiceOver may call straight back into the view.
#[must_use = "the change reaches VoiceOver only when raised"]
pub struct Notifications(Option<QueuedEvents>);

impl Notifications {
    /// Posts the notifications.
    pub fn raise(self) {
        if let Some(events) = self.0 {
            events.raise();
        }
    }
}

struct Activation<'a> {
    tree: &'a RefCell<AccessTree>,
    window: &'a slint::Window,
}

impl ActivationHandler for Activation<'_> {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        Some(self.tree.borrow_mut().build(self.window))
    }
}

struct Actions {
    tree: Rc<RefCell<AccessTree>>,
}

impl ActionHandler for Actions {
    fn do_action(&mut self, request: ActionRequest) {
        let Some(item) = self.tree.borrow().item(request.target_node) else {
            return;
        };
        // AppKit asks from inside its own accessibility call; the action runs
        // on the host's next `sb_tick`, outside it.
        slint::Timer::single_shot(Duration::ZERO, move || perform(&item, &request));
    }
}
