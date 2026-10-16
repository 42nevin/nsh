//! The iterators of the classes' pair lists (`URLSearchParams`, `Headers`,
//! `FormData`): over a snapshot of the items, inheriting from
//! `%IteratorPrototype%` (so they have `map`, `toArray`, ...).

use super::{
    Class, Key, Object, Spec, data, instance,
    js::{self, Ctx, R, V, native, trace},
};

/// What an iterator over pairs yields.
#[derive(Clone, Copy)]
pub(crate) enum Part {
    Entries,
    Keys,
    Values,
}

struct ListIterator {
    items: std::collections::VecDeque<V>,
}

trace!(ListIterator { items });

impl Object for ListIterator {}

/// An iterator yielding `items`.
pub(crate) fn new(ctx: Ctx, items: Vec<V>) -> R<V> {
    instance(
        ctx,
        js::UNDEFINED,
        Class::Iterator,
        ListIterator {
            items: items.into(),
        },
    )
}

/// One item of an iterator over string pairs.
pub(crate) fn pair_item(ctx: Ctx, k: &str, v: &str, part: Part) -> R<V> {
    pair_value(ctx, k, js::string(ctx, v)?, part)
}

/// One item of an iterator over pairs with value `v`.
pub(crate) fn pair_value(ctx: Ctx, k: &str, v: V, part: Part) -> R<V> {
    match part {
        Part::Keys => js::string(ctx, k),
        Part::Values => Ok(v),
        Part::Entries => js::new_array(ctx, vec![js::string(ctx, k)?, v]),
    }
}

native!(
    fn next(ctx, this, args) {
        let item = data::<ListIterator>(ctx, this, "Iterator")?.items.pop_front();
        match item {
            Some(v) => js::iter_result(ctx, v, false),
            None => js::iter_result(ctx, V::undefined(), true),
        }
    }
);

pub(crate) const SPECS: &[Spec] = &[Spec {
    class: Class::Iterator,
    name: c"Iterator",
    parent: None,
    constructor: None,
    methods: &[(Key::Name(c"next"), next, 0)],
    accessors: &[],
    statics: &[],
    constants: &[],
    global: false,
    ts: crate::web::ClassTs::NONE,
}];
