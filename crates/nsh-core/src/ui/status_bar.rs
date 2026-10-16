//! The status bar's settings, layered: what functions set (JS's root
//! `statusBar` command, a UI set's module, a task) at the bottom, the
//! settings components render over it, the most recently added winning
//! field by field. It is a global of the surface, so every component,
//! module and task shares it; the core sends the merged result once per
//! commit, when it changed.

use super::element::Globals;
use crate::{StatusBarConfig, StatusBarSetting};

/// A setting a component rendered ([`super::Context::add_status_bar`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StatusBarEntry(u64);

#[derive(Debug, Default)]
pub(crate) struct StatusBars {
    base: StatusBarSetting,
    stack: Vec<(u64, StatusBarSetting)>,
    next: u64,
    /// Changed since the last commit.
    dirty: bool,
    shown: Option<StatusBarConfig>,
}

impl StatusBars {
    /// Runs `f` on the surface's settings.
    pub(crate) fn update<R>(globals: &Globals, f: impl FnOnce(&mut StatusBars) -> R) -> R {
        let mut globals = globals.borrow_mut();
        let bars = globals
            .entry(std::any::TypeId::of::<StatusBars>())
            .or_insert_with(|| Box::new(StatusBars::default()))
            .downcast_mut::<StatusBars>()
            .expect("keyed by its type");
        bars.dirty = true;
        f(bars)
    }

    /// The merged config, when it changed since the last call (once per
    /// commit).
    pub(crate) fn take_changed(globals: &Globals) -> Option<StatusBarConfig> {
        let mut globals = globals.borrow_mut();
        let bars = globals
            .get_mut(&std::any::TypeId::of::<StatusBars>())?
            .downcast_mut::<StatusBars>()?;
        if !std::mem::take(&mut bars.dirty) {
            return None;
        }
        bars.changed()
    }

    /// Patches the base (fields `patch` sets).
    pub(crate) fn set_base(&mut self, patch: StatusBarSetting) {
        self.base = patch.over(self.base);
    }

    pub(crate) fn add(&mut self, setting: StatusBarSetting) -> StatusBarEntry {
        self.next += 1;
        self.stack.push((self.next, setting));
        StatusBarEntry(self.next)
    }

    pub(crate) fn set(&mut self, entry: StatusBarEntry, setting: StatusBarSetting) {
        if let Some((_, s)) = self.stack.iter_mut().find(|(id, _)| *id == entry.0) {
            *s = setting;
        }
    }

    pub(crate) fn remove(&mut self, entry: StatusBarEntry) {
        self.stack.retain(|(id, _)| *id != entry.0);
    }

    fn changed(&mut self) -> Option<StatusBarConfig> {
        let merged = self
            .stack
            .iter()
            .fold(self.base, |below, (_, s)| s.over(below));
        let config = StatusBarConfig {
            style: merged.style.unwrap_or_default(),
            hidden: merged.hidden.unwrap_or(false),
            animated: merged.animated.unwrap_or(false),
            background: merged.background,
        };
        (self.shown != Some(config)).then(|| {
            self.shown = Some(config);
            config
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StatusBarStyle, ui::Color};

    fn style(style: StatusBarStyle) -> StatusBarSetting {
        StatusBarSetting {
            style: Some(style),
            ..Default::default()
        }
    }

    fn hidden() -> StatusBarSetting {
        StatusBarSetting {
            hidden: Some(true),
            ..Default::default()
        }
    }

    #[test]
    fn the_last_entry_wins_field_by_field_over_the_base() {
        let globals = Globals::default();
        let shown = |f: &dyn Fn(&mut StatusBars)| {
            StatusBars::update(&globals, f);
            StatusBars::take_changed(&globals).map(|c| (c.style, c.hidden, c.background))
        };
        let red = Color::parse("#f00");
        assert_eq!(StatusBars::take_changed(&globals), None, "nothing set");
        assert_eq!(
            shown(&|b| b.set_base(StatusBarSetting {
                background: red,
                ..style(StatusBarStyle::Dark)
            })),
            Some((StatusBarStyle::Dark, false, red))
        );
        let first = StatusBars::update(&globals, |b| b.add(style(StatusBarStyle::Light)));
        let second = StatusBars::update(&globals, |b| b.add(hidden()));
        // Both in one commit.
        assert_eq!(
            StatusBars::take_changed(&globals).map(|c| (c.style, c.hidden)),
            Some((StatusBarStyle::Light, true))
        );
        assert_eq!(shown(&|b| b.set(second, hidden())), None, "unchanged");
        assert_eq!(
            shown(&|b| b.remove(first)),
            Some((StatusBarStyle::Dark, true, red))
        );
    }
}
