//! GTK page navigation.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Stack, StackTransitionType, Widget};

use crate::Overview;

/// Navigator allowing transition between different [`SettingsPanel`]
/// implementations.
#[derive(Clone, Default)]
pub struct Navigator {
    nodes: Rc<RefCell<Vec<NavigatorNode>>>,
    stack: Stack,
}

impl Navigator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pop the current panel, returning to its parent.
    pub fn pop(&self) {
        let mut nodes = self.nodes.borrow_mut();

        // Update the visible element.
        let parent = nodes.len().checked_sub(2).and_then(|index| nodes.get(index));
        match parent {
            Some(NavigatorNode { name, .. }) => {
                self.stack.set_visible_child_full(name, StackTransitionType::SlideRight)
            },
            None => {
                self.stack.set_visible_child_full(Overview::id(), StackTransitionType::SlideRight)
            },
        }

        // Destroy node if it was a temporary child.
        if let Some(NavigatorNode { name, destroy_on_pop: true }) = nodes.pop() {
            if let Some(child) = self.stack.child_by_name(&name) {
                self.stack.remove(&child);
            }
        }
    }

    /// Show a different panel, adding it to the top of the stack.
    pub fn show(&self, name: &str) {
        let mut nodes = self.nodes.borrow_mut();
        nodes.push(NavigatorNode::new(name, false));
        self.stack.set_visible_child_full(name, StackTransitionType::SlideLeft);
    }

    /// Add an element to the underlying stack.
    pub fn add<P, W>(&self, page: &P)
    where
        P: Page<W>,
        W: IsA<Widget>,
    {
        self.stack.add_named(page.widget(), Some(P::id()));
    }

    /// Get the navigator's GTK widget.
    pub fn widget(&self) -> &Stack {
        &self.stack
    }
}

/// Node in the navigator chain.
#[derive(Default)]
struct NavigatorNode {
    name: String,
    destroy_on_pop: bool,
}

impl NavigatorNode {
    fn new(name: &str, destroy_on_pop: bool) -> Self {
        Self { destroy_on_pop, name: name.into() }
    }
}

/// Page in the GTK navigation stack.
pub trait Page<W: IsA<Widget>> {
    /// Navigation ID.
    fn id() -> &'static str;

    /// Root widget element.
    fn widget(&self) -> &W;

    /// Hook run before this view is shown.
    fn on_show(&mut self) {}
}
