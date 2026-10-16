//! Handles to the elements a component renders (TS: `ref`), to send
//! them commands: focus an input, scroll a scroll view, drive a web view or
//! a video, call a native component.

use std::{
    cell::Cell,
    fmt,
    rc::{Rc, Weak},
};

use super::task::{Action, Executor};
use crate::{Command, JsonValue, NodeId};

/// One element of a component's render: made once
/// ([`super::Context::element_ref`], in `setup`), put on an element each
/// render ([`super::Element::element_ref`]). Its commands go to whatever
/// element carries it now, in order with the component's other changes;
/// they do nothing while no rendered element carries it. Works from
/// listeners and tasks alike.
#[derive(Clone)]
pub struct ElementRef {
    /// The node, 0 while none carries it.
    node: Rc<Cell<NodeId>>,
    executor: Weak<Executor>,
}

impl ElementRef {
    pub(crate) fn new(executor: &Rc<Executor>) -> Self {
        Self {
            node: Rc::new(Cell::new(0)),
            executor: Rc::downgrade(executor),
        }
    }

    /// The element's node, while one is rendered.
    pub fn node(&self) -> Option<NodeId> {
        Some(self.node.get()).filter(|&n| n != 0)
    }

    pub(crate) fn attach(&self, node: NodeId) {
        self.node.set(node);
    }

    /// Forgets `node` when it is the one carrying the ref.
    pub(crate) fn detach(&self, node: NodeId) {
        if self.node.get() == node {
            self.node.set(0);
        }
    }

    fn send(&self, command: Command) {
        if let (Some(node), Some(executor)) = (self.node(), self.executor.upgrade()) {
            executor.act(Action::Command(node, command));
        }
    }

    /// Focuses an input (showing the keyboard).
    pub fn focus(&self) {
        self.send(Command::Focus);
    }

    /// Unfocuses an input.
    pub fn blur(&self) {
        self.send(Command::Blur);
    }

    /// Selects `start..end` of an input (UTF-16 units; a caret when equal).
    pub fn set_selection(&self, start: u32, end: u32) {
        self.send(Command::SetSelection { start, end });
    }

    /// Scrolls a scroll view to an offset.
    pub fn scroll_to(&self, x: f32, y: f32, animated: bool) {
        self.send(Command::ScrollTo { x, y, animated });
    }

    /// Scrolls a list to `offset` along its axis.
    pub fn scroll_to_offset(&self, offset: f32, animated: bool) {
        self.send(Command::ScrollTo {
            x: offset,
            y: offset,
            animated,
        });
    }

    /// Scrolls a scroll view or a list to the end of its content.
    pub fn scroll_to_end(&self, animated: bool) {
        self.send(Command::ScrollToEnd { animated });
    }

    /// Scrolls a list to row `index`: `view_position` 0 puts it at the
    /// start of the viewport, 1 at the end, `view_offset` points further.
    pub fn scroll_to_index(
        &self,
        index: u32,
        view_position: f32,
        view_offset: f32,
        animated: bool,
    ) {
        self.send(Command::ScrollToIndex {
            index,
            animated,
            view_position,
            view_offset,
        });
    }

    /// Zooms a scroll view to `scale`, around `center` (content points) or
    /// keeping the middle.
    pub fn zoom_to(&self, scale: f32, center: Option<(f32, f32)>, animated: bool) {
        self.send(Command::ZoomTo {
            scale,
            center,
            animated,
        });
    }

    /// Reloads a web view's page.
    pub fn reload(&self) {
        self.send(Command::Reload);
    }

    /// Goes back in a web view's history.
    pub fn go_back(&self) {
        self.send(Command::GoBack);
    }

    /// Goes forward in a web view's history.
    pub fn go_forward(&self) {
        self.send(Command::GoForward);
    }

    /// Stops a web view loading.
    pub fn stop_loading(&self) {
        self.send(Command::StopLoading);
    }

    /// Runs `script` in a web view's page.
    pub fn inject_java_script(&self, script: impl Into<String>) {
        self.send(Command::InjectJavaScript(script.into()));
    }

    /// Sends `data` to a web view's page (a `message` event on `window`).
    pub fn post_message(&self, data: impl Into<String>) {
        self.send(Command::PostMessage(data.into()));
    }

    /// Jumps a video to `seconds`.
    pub fn seek(&self, seconds: f64) {
        self.send(Command::Seek(seconds));
    }

    /// Calls command `name` of a native component with `args`.
    pub fn command(&self, name: &str, args: JsonValue) {
        self.send(Command::Native {
            name: name.into(),
            args,
        });
    }
}

impl PartialEq for ElementRef {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.node, &other.node)
    }
}

impl fmt::Debug for ElementRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ElementRef").field(&self.node.get()).finish()
    }
}
