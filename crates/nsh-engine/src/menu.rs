//! Native ownership of application-menu callbacks, without retaining a VM.

use std::{fmt, sync::Arc};

use nsh_core::desktop::MenuItem;

use crate::{EngineControl, Message};

/// Keeps one application menu's callbacks callable while its native owners
/// exist. Clone this into native action holders and menus kept while tracking.
/// Dropping the last clone retires the callbacks on their original engine,
/// after selection events already sent to that engine.
///
/// A menu rebuilt for the same items (for example, after a title-bar change)
/// shares its existing lease. Create a new lease only for a new `SetMenu`
/// command. The default lease owns no callbacks.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct MenuLease {
    state: Option<Arc<NativeMenu>>,
}

struct NativeMenu {
    control: EngineControl,
    generation: u32,
}

impl Drop for NativeMenu {
    fn drop(&mut self) {
        // The control is a sender, not the engine or a raw JS value. An old
        // menu can safely outlive a stopped/replaced engine; its send fails.
        let _ = self.control.tx.send(Message::MenuRetired(self.generation));
    }
}

impl EngineControl {
    /// Retains the callbacks of an application menu described by `items`.
    /// Keep the lease while the backend can still deliver choices from this
    /// menu. Native action holders that outlive replacement need a clone, or
    /// their stale dispatch must be invalidated before releasing the lease.
    /// Selection events use this same engine control, including across reload.
    ///
    /// Use the complete items, before filtering platform-specific roles.
    /// Application callback IDs are unique in a VM, so the highest ID names
    /// this menu's callback set; a menu without callbacks needs no lease.
    pub fn retain_menu_callbacks(&self, items: &[MenuItem]) -> MenuLease {
        let mut generation = 0;
        MenuItem::each_action(items, &mut |action| generation = generation.max(action.id));
        MenuLease {
            state: (generation != 0).then(|| {
                Arc::new(NativeMenu {
                    control: self.clone(),
                    generation,
                })
            }),
        }
    }
}

impl fmt::Debug for MenuLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MenuLease")
            .field(
                "generation",
                &self.state.as_ref().map(|state| state.generation),
            )
            .finish()
    }
}
