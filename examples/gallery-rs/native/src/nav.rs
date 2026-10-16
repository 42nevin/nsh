//! The gallery's navigation: the screens pushed over the index, a global
//! every component of the gallery can push to.

use std::rc::Rc;

use nsh_core::ui::{Component, Context, RenderCx, Screen, SharedString};

use crate::Gallery;

/// What renders a pushed screen: a `screen()` with its header options and
/// content, in the gallery's render (so it can read the globals its
/// options depend on: a Rust component cannot render its own screen, its
/// root is a view).
pub(crate) type ScreenFn = Rc<dyn Fn(&mut RenderCx<'_, Gallery>) -> Screen>;

/// A screen over the index: its key (unique in the stack) and what it
/// renders.
#[derive(Clone)]
pub(crate) struct Route {
    pub(crate) key: SharedString,
    pub(crate) screen: ScreenFn,
}

/// The pushed screens, bottom to top.
#[derive(Default)]
pub(crate) struct Routes(Vec<Route>);

impl Routes {
    pub(crate) fn screens(&self) -> Vec<Route> {
        self.0.clone()
    }

    /// Pops to `depth` screens over the index.
    pub(crate) fn truncate(&mut self, depth: usize) {
        self.0.truncate(depth);
    }
}

/// Pushes the screen `screen` makes (`key`: unique in the stack).
pub(crate) fn push<C: Component>(
    cx: &mut Context<'_, C>,
    key: impl Into<SharedString>,
    screen: impl Fn(&mut RenderCx<'_, Gallery>) -> Screen + 'static,
) {
    let route = Route {
        key: key.into(),
        screen: Rc::new(screen),
    };
    cx.update_global::<Routes, _>(|routes, _| {
        // A screen already in the stack (a double tap) is not pushed twice.
        if !routes.0.iter().any(|r| r.key == route.key) {
            routes.0.push(route);
        }
    });
}

/// Pops the top screen.
pub(crate) fn pop<C: Component>(cx: &mut Context<'_, C>) {
    cx.update_global::<Routes, _>(|routes, _| {
        routes.0.pop();
    });
}

/// Pops every screen: back to the index.
pub(crate) fn pop_to_top<C: Component>(cx: &mut Context<'_, C>) {
    cx.update_global::<Routes, _>(|routes, _| routes.0.clear());
}

/// How many screens are over the index.
pub(crate) fn depth<C: Component>(cx: &mut Context<'_, C>) -> usize {
    cx.update_global::<Routes, _>(|routes, _| routes.0.len())
}
