//! Accessibility: a component's item tree as an AccessKit tree.
//!
//! Slint feeds AccessKit only from its winit backend, which a host-driven
//! platform does not use. The item tree carries everything a screen reader
//! needs (`ItemRc` roles, accessible properties and actions), so this module
//! builds the tree itself and leaves the OS side to a per-host adapter
//! (`accesskit_macos` on macOS). Bounds are physical pixels from the view's
//! top-left corner, the space AccessKit adapters expect.
//!
//! The tree is rebuilt whole. AccessKit diffs updates by node id, so stable ids
//! are what keeps screen readers from seeing a new tree every frame.

use std::collections::HashMap;
use std::pin::Pin;
use std::ptr::NonNull;

use accesskit::{
    Action, ActionData, ActionRequest, Node, NodeId, Orientation, Rect, Role, Toggled, TreeId,
    TreeInfo, TreeUpdate,
};
use i_slint_core::accessibility::{
    AccessibilityAction, AccessibleStringProperty as Prop, SupportedAccessibilityAction,
    accessible_descendents, find_text_input, nearest_accessible_item,
};
use i_slint_core::input::FocusReason;
use i_slint_core::item_tree::{ItemTreeRc, ItemTreeRef, ItemTreeWeak};
use i_slint_core::items::{AccessibleRole, InputType, ItemRc, WindowItem};
use i_slint_core::lengths::LogicalPoint;
use i_slint_core::properties::PropertyTracker;
use i_slint_core::window::{PopupWindowLocation, WindowInner};

/// Id of the root while the window has no component yet.
const EMPTY_ROOT: NodeId = NodeId(0);
/// Low bits of a [`NodeId`] hold the item index inside its item tree, the high
/// bits the item tree (component, repeater instance or popup).
const INDEX_BITS: u32 = 32;

/// Builds AccessKit trees for one window and maps node ids back to items.
pub struct AccessTree {
    /// Item trees by address. Ids are never reused, so a stale id from a
    /// screen reader cannot reach a newer component at the same address.
    trees: HashMap<NonNull<u8>, (u64, ItemTreeWeak)>,
    next_tree: u64,
    /// Tracks the properties the last build read. AccessKit gets a new tree
    /// when one of them changes even if nothing was redrawn.
    tracker: Pin<Box<PropertyTracker<true>>>,
    last_focus: Option<NodeId>,
}

impl Default for AccessTree {
    fn default() -> Self {
        Self {
            trees: HashMap::new(),
            next_tree: 1,
            tracker: Box::pin(PropertyTracker::default()),
            last_focus: None,
        }
    }
}

impl AccessTree {
    /// A full tree of `window`'s component.
    pub fn build(&mut self, window: &slint::Window) -> TreeUpdate {
        self.trees.retain(|_, (_, weak)| weak.upgrade().is_some());
        // Taken out for the build, which needs `self`; the box keeps it pinned.
        let tracker = std::mem::replace(&mut self.tracker, Box::pin(PropertyTracker::default()));
        let update = tracker
            .as_ref()
            .evaluate_as_dependency_root(|| self.build_untracked(window));
        self.tracker = tracker;
        self.last_focus = Some(update.focus);
        update
    }

    /// Whether the last tree is out of date: the scene was `redrawn`, a
    /// property it read changed, or focus moved.
    ///
    /// Redraws stand in for structure changes (repeaters, conditionals, popups):
    /// Slint reports those only through `WindowAdapterInternal`, which is not
    /// public API.
    pub fn is_stale(&mut self, window: &slint::Window, redrawn: bool) -> bool {
        let focus = self.focus_node(window);
        redrawn || self.last_focus != Some(focus) || self.tracker.is_dirty()
    }

    /// A full tree when [`Self::is_stale`]; `None` when the last tree still holds.
    pub fn update(&mut self, window: &slint::Window, redrawn: bool) -> Option<TreeUpdate> {
        self.is_stale(window, redrawn).then(|| self.build(window))
    }

    /// The item a screen reader names by `id`, if it still exists.
    pub fn item(&self, id: NodeId) -> Option<ItemRc> {
        let tree = id.0 >> INDEX_BITS;
        let index = u32::try_from(id.0 & u64::from(u32::MAX)).ok()?;
        let (_, weak) = self.trees.values().find(|(t, _)| *t == tree)?;
        Some(ItemRc::new(weak.upgrade()?, index))
    }

    fn build_untracked(&mut self, window: &slint::Window) -> TreeUpdate {
        let inner = WindowInner::from_pub(window);
        let Some(component) = inner.try_component() else {
            return TreeUpdate {
                nodes: vec![(EMPTY_ROOT, Node::new(Role::GenericContainer))],
                tree: Some(TreeInfo::new(EMPTY_ROOT)),
                tree_id: TreeId::ROOT,
                focus: EMPTY_ROOT,
            };
        };
        let root = ItemRc::new_root(component);
        // Popups the software window draws inline hang under the item that opened them.
        let popups: Vec<(NodeId, ItemTreeRc, LogicalPoint)> = inner
            .active_popups()
            .iter()
            .filter_map(|popup| {
                let PopupWindowLocation::ChildWindow(at) = popup.location else {
                    return None;
                };
                let opener = popup
                    .parent_item
                    .upgrade()
                    .map(nearest_accessible_item)
                    .filter(ItemRc::is_accessible)
                    .unwrap_or_else(|| root.clone());
                Some((self.node_id(&opener), popup.component.clone(), at))
            })
            .collect();
        let mut nodes = Vec::new();
        let scale = f64::from(window.scale_factor());
        let root_id = self.add(&root, LogicalPoint::default(), scale, &popups, &mut nodes);
        let mut info = TreeInfo::new(root_id);
        info.toolkit_name = Some("Slint".into());
        TreeUpdate {
            nodes,
            tree: Some(info),
            tree_id: TreeId::ROOT,
            focus: self.focus_node(window),
        }
    }

    /// Adds `item` and its accessible descendants; returns `item`'s id.
    fn add(
        &mut self,
        item: &ItemRc,
        origin: LogicalPoint,
        scale: f64,
        popups: &[(NodeId, ItemTreeRc, LogicalPoint)],
        nodes: &mut Vec<(NodeId, Node)>,
    ) -> NodeId {
        let id = self.node_id(item);
        let mut children: Vec<NodeId> = accessible_descendents(item)
            .map(|child| self.add(&child, origin, scale, popups, nodes))
            .collect();
        for (_, popup, at) in popups.iter().filter(|(opener, ..)| *opener == id) {
            children.push(self.add(&ItemRc::new_root(popup.clone()), *at, scale, popups, nodes));
        }
        let mut node = describe(item, origin, scale);
        node.set_children(children);
        nodes.push((id, node));
        id
    }

    fn node_id(&mut self, item: &ItemRc) -> NodeId {
        let tree = item.item_tree();
        let key = ItemTreeRef::as_ptr(ItemTreeRc::borrow(tree));
        let next = &mut self.next_tree;
        let (tree_id, _) = self.trees.entry(key).or_insert_with(|| {
            let id = *next;
            *next += 1;
            (id, ItemTreeRc::downgrade(tree))
        });
        NodeId((*tree_id << INDEX_BITS) | u64::from(item.index()))
    }

    /// The node standing for Slint's focus item, or the root when nothing has focus.
    fn focus_node(&mut self, window: &slint::Window) -> NodeId {
        let inner = WindowInner::from_pub(window);
        let focused = inner.focus_item.borrow().upgrade();
        match focused.map(nearest_accessible_item) {
            Some(item) => self.node_id(&delegated_focus(item)),
            None => match inner.try_component() {
                Some(component) => self.node_id(&ItemRc::new_root(component)),
                None => EMPTY_ROOT,
            },
        }
    }
}

/// Runs a screen reader's request on `item`. Call it outside the adapter's own
/// callbacks (from a zero-delay timer, as the winit backend does): Slint
/// callbacks may run, and they may call back into the host.
pub fn perform(item: &ItemRc, request: &ActionRequest) {
    let action = match (&request.action, &request.data) {
        (Action::Focus, _) => {
            if let Some(adapter) = item.window_adapter() {
                // A pointer reason, so text inputs do not select all their text.
                WindowInner::from_pub(adapter.window()).set_focus_item(
                    item,
                    true,
                    FocusReason::PointerClick,
                );
            }
            return;
        }
        (Action::Click, _) => AccessibilityAction::Default,
        (Action::Increment, _) => AccessibilityAction::Increment,
        (Action::Decrement, _) => AccessibilityAction::Decrement,
        (Action::Expand, _) => AccessibilityAction::Expand,
        (Action::SetValue, Some(ActionData::Value(v))) => {
            AccessibilityAction::SetValue(v.as_ref().into())
        }
        (Action::SetValue, Some(ActionData::NumericValue(v))) => {
            AccessibilityAction::SetValue(v.to_string().into())
        }
        (Action::ReplaceSelectedText, Some(ActionData::Value(v))) => {
            AccessibilityAction::ReplaceSelectedText(v.as_ref().into())
        }
        _ => return,
    };
    item.accessible_action(&action);
}

/// Items such as a list view name one of their children as the focused one.
fn delegated_focus(item: ItemRc) -> ItemRc {
    item.accessible_string_property(Prop::DelegateFocus)
        .and_then(|i| i.parse::<usize>().ok())
        .and_then(|i| accessible_descendents(&item).nth(i))
        .unwrap_or(item)
}

/// `item` as an AccessKit node, without children.
fn describe(item: &ItemRc, origin: LogicalPoint, scale: f64) -> Node {
    let prop = |p| item.accessible_string_property(p);
    let flag = |p| prop(p).is_some_and(|v| v == "true");
    let number = |p| prop(p).and_then(|v| v.parse::<f64>().ok());

    // The root `Window` is the view's content, not a window: the host view
    // and its window already carry the title.
    let is_window = item.downcast::<WindowItem>().is_some();
    let role = if is_window { Role::Group } else { role(item) };
    let mut node = Node::new(role);
    // Static text has no label: AppKit and UIA read a label's value.
    match prop(Prop::Label) {
        Some(label) if role == Role::Label => node.set_value(label.as_str()),
        Some(label) => node.set_label(label.as_str()),
        None => {}
    }
    if let Some(value) = prop(Prop::Value) {
        match value.parse::<f64>() {
            Ok(n) if !is_text_input(role) => node.set_numeric_value(n),
            _ => node.set_value(value.as_str()),
        }
    }
    if let Some(v) = prop(Prop::PlaceholderText) {
        node.set_placeholder(v.as_str());
    }
    if let Some(v) = prop(Prop::Description) {
        node.set_description(v.as_str());
    }
    if let Some(v) = prop(Prop::Id) {
        node.set_author_id(v.as_str());
    }
    if let Some(v) = number(Prop::ValueMinimum) {
        node.set_min_numeric_value(v);
    }
    if let Some(v) = number(Prop::ValueMaximum) {
        node.set_max_numeric_value(v);
    }
    if let Some(v) = number(Prop::ValueStep) {
        node.set_numeric_value_step(v);
    }
    if prop(Prop::Enabled).is_some_and(|v| v != "true") {
        node.set_disabled();
    }
    if flag(Prop::ReadOnly) {
        node.set_read_only();
    }
    if flag(Prop::Checkable) {
        node.set_toggled(if flag(Prop::Checked) {
            Toggled::True
        } else {
            Toggled::False
        });
    }
    if flag(Prop::Expandable) {
        node.set_expanded(flag(Prop::Expanded));
    }
    if flag(Prop::ItemSelectable) {
        node.set_selected(flag(Prop::ItemSelected));
    }
    if let Some(i) = number(Prop::ItemIndex) {
        node.set_position_in_set(i as usize);
    }
    if let Some(n) = number(Prop::ItemCount) {
        node.set_size_of_set(n as usize);
    }
    match prop(Prop::Orientation).as_deref() {
        Some("horizontal") => node.set_orientation(Orientation::Horizontal),
        Some("vertical") => node.set_orientation(Orientation::Vertical),
        _ => {}
    }
    if !item.is_visible() {
        node.set_hidden();
    }

    let geometry = item.geometry();
    let at = item.map_to_window(geometry.origin) + origin.to_vector();
    let (x, y) = (f64::from(at.x) * scale, f64::from(at.y) * scale);
    let (w, h) = (
        f64::from(geometry.width()) * scale,
        f64::from(geometry.height()) * scale,
    );
    node.set_bounds(Rect {
        x0: x,
        y0: y,
        x1: x + w,
        y1: y + h,
    });

    if takes_focus(role) {
        node.add_action(Action::Focus);
    }
    let supported = item.supported_accessibility_actions();
    for (slint, action) in [
        (SupportedAccessibilityAction::Default, Action::Click),
        (SupportedAccessibilityAction::Increment, Action::Increment),
        (SupportedAccessibilityAction::Decrement, Action::Decrement),
        (SupportedAccessibilityAction::Expand, Action::Expand),
        (SupportedAccessibilityAction::SetValue, Action::SetValue),
        (
            SupportedAccessibilityAction::ReplaceSelectedText,
            Action::ReplaceSelectedText,
        ),
    ] {
        if supported.contains(slint) {
            node.add_action(action);
        }
    }
    node
}

fn role(item: &ItemRc) -> Role {
    match item.accessible_role() {
        AccessibleRole::Button => Role::Button,
        AccessibleRole::Checkbox => Role::CheckBox,
        AccessibleRole::Combobox => Role::ComboBox,
        AccessibleRole::Groupbox => Role::Group,
        AccessibleRole::Image => Role::Image,
        AccessibleRole::List => Role::ListBox,
        AccessibleRole::ListItem => Role::ListBoxOption,
        AccessibleRole::ProgressIndicator => Role::ProgressIndicator,
        AccessibleRole::RadioButton => Role::RadioButton,
        AccessibleRole::RadioGroup => Role::RadioGroup,
        AccessibleRole::Slider => Role::Slider,
        AccessibleRole::Spinbox => Role::SpinButton,
        AccessibleRole::Switch => Role::Switch,
        AccessibleRole::Tab => Role::Tab,
        AccessibleRole::TabList => Role::TabList,
        AccessibleRole::TabPanel => Role::TabPanel,
        AccessibleRole::Table => Role::Table,
        AccessibleRole::Text => Role::Label,
        AccessibleRole::TextInput => text_input_role(item),
        AccessibleRole::Tree => Role::Tree,
        // Landmarks and newer roles stay plain containers for the spike.
        _ => Role::GenericContainer,
    }
}

fn text_input_role(item: &ItemRc) -> Role {
    let Some(input) = find_text_input(item) else {
        return Role::TextInput;
    };
    let input = input.as_pin_ref();
    if !input.single_line() {
        return Role::MultilineTextInput;
    }
    match input.input_type() {
        InputType::Password => Role::PasswordInput,
        InputType::Number | InputType::Decimal => Role::NumberInput,
        InputType::Search => Role::SearchInput,
        _ => Role::TextInput,
    }
}

fn is_text_input(role: Role) -> bool {
    matches!(
        role,
        Role::TextInput
            | Role::MultilineTextInput
            | Role::PasswordInput
            | Role::NumberInput
            | Role::SearchInput
    )
}

/// Roles a screen reader may move keyboard focus to.
fn takes_focus(role: Role) -> bool {
    is_text_input(role)
        || matches!(
            role,
            Role::Button
                | Role::CheckBox
                | Role::ComboBox
                | Role::ListBoxOption
                | Role::Slider
                | Role::SpinButton
                | Role::Switch
                | Role::Tab
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EmbeddedHost;
    use crate::runtime::{self, ComponentInstance, Value};
    use slint::ComponentHandle;

    fn demo() -> Result<EmbeddedHost<ComponentInstance>, crate::Error> {
        let source = include_str!("../ui/demo.slint");
        let definition = runtime::compile(Some(source), std::path::Path::new("demo.slint"))?;
        EmbeddedHost::new(|| definition.create(), 640, 400, 2.0)
    }

    fn find(update: &TreeUpdate, role: Role) -> Option<&(NodeId, Node)> {
        update.nodes.iter().find(|(_, n)| n.role() == role)
    }

    #[test]
    fn demo_form_exposes_field_button_and_heading() -> Result<(), crate::Error> {
        let host = demo()?;
        let update = AccessTree::default().build(host.component().window());

        let field = find(&update, Role::TextInput).map(|(_, n)| n);
        assert_eq!(field.and_then(Node::label), Some("Name"));
        assert!(field.is_some_and(|n| n.supports_action(Action::Focus)));

        let button = find(&update, Role::Button).map(|(_, n)| n);
        assert_eq!(button.and_then(Node::label), Some("Continue"));
        assert!(button.is_some_and(|n| n.supports_action(Action::Click)));

        let heading = update
            .nodes
            .iter()
            .any(|(_, n)| n.role() == Role::Label && n.value() == Some("Sign in"));
        assert!(heading, "the heading is read as static text");

        // Every child the root lists is in the update.
        let root = update.tree.as_ref().map(|t| t.root);
        let children = update.nodes.iter().find(|(id, _)| Some(*id) == root);
        let listed = children
            .map(|(_, n)| n.children().to_vec())
            .unwrap_or_default();
        assert!(listed.len() >= 3);
        assert!(
            listed
                .iter()
                .all(|c| update.nodes.iter().any(|(id, _)| id == c))
        );
        Ok(())
    }

    #[test]
    fn bounds_are_physical_pixels_from_the_top_left() -> Result<(), crate::Error> {
        let host = demo()?;
        let update = AccessTree::default().build(host.component().window());
        // demo.slint puts the button at (16, 96) logical, 128×32, at scale 2.
        let bounds = find(&update, Role::Button).and_then(|(_, n)| n.bounds());
        assert_eq!(
            bounds,
            Some(Rect {
                x0: 32.0,
                y0: 192.0,
                x1: 288.0,
                y1: 256.0
            })
        );
        Ok(())
    }

    #[test]
    fn ids_are_stable_and_map_back_to_items() -> Result<(), crate::Error> {
        let host = demo()?;
        let mut tree = AccessTree::default();
        let first = tree.build(host.component().window());
        let second = tree.build(host.component().window());
        let ids = |u: &TreeUpdate| u.nodes.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        assert_eq!(ids(&first), ids(&second));
        let button = find(&first, Role::Button).map(|(id, _)| *id);
        let item = button.and_then(|id| tree.item(id));
        assert_eq!(
            item.map(|i| i.accessible_role()),
            Some(AccessibleRole::Button)
        );
        Ok(())
    }

    #[test]
    fn screen_reader_actions_reach_the_component() -> Result<(), crate::Error> {
        let host = demo()?;
        let mut tree = AccessTree::default();
        let update = tree.build(host.component().window());
        let request = |id, action, data| ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: id,
            data,
        };

        let field = find(&update, Role::TextInput).map(|(id, _)| *id);
        let field = field.ok_or(crate::Error::NoWindow)?;
        let item = tree.item(field).ok_or(crate::Error::NoWindow)?;
        perform(&item, &request(field, Action::Focus, None));
        perform(
            &item,
            &request(
                field,
                Action::SetValue,
                Some(ActionData::Value("Ada".into())),
            ),
        );
        assert_eq!(
            host.component().get_property("name").ok(),
            Some(Value::String("Ada".into()))
        );
        // Focus moved to the field, so the next update reports it.
        let after = tree.update(host.component().window(), false);
        assert_eq!(after.map(|u| u.focus), Some(field));

        let button = find(&update, Role::Button).map(|(id, _)| *id);
        let button = button.ok_or(crate::Error::NoWindow)?;
        let item = tree.item(button).ok_or(crate::Error::NoWindow)?;
        perform(&item, &request(button, Action::Click, None));
        assert_eq!(
            host.component().get_property("submit-count").ok(),
            Some(Value::Number(1.0))
        );
        Ok(())
    }

    #[test]
    fn unchanged_scene_needs_no_update() -> Result<(), crate::Error> {
        let host = demo()?;
        let mut tree = AccessTree::default();
        let _ = tree.build(host.component().window());
        assert!(tree.update(host.component().window(), false).is_none());
        assert!(tree.update(host.component().window(), true).is_some());
        Ok(())
    }

    #[test]
    fn a_property_change_without_redraw_rebuilds() -> Result<(), crate::Error> {
        let host = demo()?;
        let mut tree = AccessTree::default();
        let _ = tree.build(host.component().window());
        let _ = host
            .component()
            .set_property("name", Value::String("Grace".into()));
        let update = tree.update(host.component().window(), false);
        let field = update
            .as_ref()
            .and_then(|u| find(u, Role::TextInput))
            .map(|(_, n)| n);
        assert_eq!(field.and_then(Node::value), Some("Grace"));
        Ok(())
    }
}
