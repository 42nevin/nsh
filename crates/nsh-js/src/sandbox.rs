//! The VM sandbox: resource limits, per-call time budgets and language
//! hardening.
//!
//! Every surface runs its own QuickJS runtime, so the limits are per VM. The
//! heap and stack ceilings are runtime settings. Time is budgeted per call
//! from Rust into JS (a bundle evaluation, an event, a timer, the microtask
//! drain, ...): QuickJS calls the interrupt handler periodically while it
//! interprets, and returning `true` unwinds the call with an exception no
//! `catch` can swallow. A long-lived app therefore never accumulates time
//! toward a limit; only a single call that does not return in time is cut
//! off.
//!
//! The JS runtime (solid-js, `js/runtime`) is compiled into every bundle, so
//! it is exactly as trusted as the app's own code: limits and checks are
//! enforced here, in Rust, never in JS.

use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

use rquickjs::Ctx;

/// Resource limits of one VM.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// QuickJS heap, in bytes (0: no limit). Running out stops the VM.
    pub memory_bytes: usize,
    /// Interpreter stack, in bytes: deep recursion becomes a `RangeError`
    /// rather than a native stack overflow. Must stay below the engine
    /// thread's stack.
    pub stack_bytes: usize,
    /// Evaluating the bundle.
    pub load_budget: Duration,
    /// Any other call into JS: one event (with the rendering it causes), one
    /// list bind, one timer, one frame's callbacks, the microtask drain.
    pub call_budget: Duration,
    /// Values one commit may send (the JS op buffer's length). The buffer is
    /// decoded into Rust memory, which the heap limit does not cover.
    pub max_op_values: usize,
    /// Operations one canvas drawing may record.
    pub max_canvas_ops: usize,
    /// Nodes the core keeps (each may have a native view, whose memory the
    /// heap limit does not cover either).
    pub max_nodes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_bytes: 256 << 20,
            stack_bytes: 4 << 20,
            load_budget: Duration::from_secs(10),
            call_budget: Duration::from_secs(2),
            max_op_values: 8_000_000,
            max_canvas_ops: 1_000_000,
            max_nodes: 200_000,
        }
    }
}

/// The kind of call into JS, which decides its budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Evaluating the bundle (or a development update).
    Load,
    /// Everything else.
    Call,
}

/// The clock of the call in progress, shared with the interrupt handler.
pub(crate) struct Budget {
    limits: Limits,
    /// When the running call must have returned.
    deadline: Rc<Cell<Option<Instant>>>,
    /// The call that overran, until its error is reported.
    tripped: Rc<Cell<Option<Phase>>>,
    /// The running call's kind.
    phase: Rc<Cell<Phase>>,
}

impl Budget {
    pub(crate) fn new(limits: Limits) -> Self {
        Self {
            limits,
            deadline: Rc::default(),
            tripped: Rc::default(),
            phase: Rc::new(Cell::new(Phase::Call)),
        }
    }

    pub(crate) fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Starts the clock of a call, unless one is running (nested calls
    /// share the outermost call's budget). Returns whether it started one,
    /// which the caller passes to [`Budget::end`].
    pub(crate) fn begin(&self, phase: Phase) -> bool {
        if self.deadline.get().is_some() {
            return false;
        }
        let budget = match phase {
            Phase::Load => self.limits.load_budget,
            Phase::Call => self.limits.call_budget,
        };
        self.phase.set(phase);
        self.tripped.set(None);
        self.deadline.set(Instant::now().checked_add(budget));
        true
    }

    pub(crate) fn end(&self, started: bool) {
        if started {
            self.deadline.set(None);
        }
    }

    /// Why the last call was cut off, if it overran its budget.
    pub(crate) fn take_trip(&self) -> Option<String> {
        self.tripped.take().map(|phase| {
            let (what, budget) = match phase {
                Phase::Load => ("loading the bundle", self.limits.load_budget),
                Phase::Call => ("a call into JS", self.limits.call_budget),
            };
            format!(
                "the script was interrupted: {what} exceeded its budget of {} ms",
                budget.as_millis()
            )
        })
    }

    /// The runtime's interrupt handler: true once the running call is past
    /// its deadline or the engine is stopping.
    pub(crate) fn interrupt_handler(
        &self,
        stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Box<dyn FnMut() -> bool> {
        let deadline = self.deadline.clone();
        let tripped = self.tripped.clone();
        let phase = self.phase.clone();
        Box::new(move || {
            if stop
                .as_ref()
                .is_some_and(|s| s.load(std::sync::atomic::Ordering::Relaxed))
            {
                return true;
            }
            match deadline.get() {
                Some(d) if Instant::now() > d => {
                    tripped.set(Some(phase.get()));
                    true
                }
                _ => false,
            }
        })
    }
}

/// Removes every way from a string to code (`eval`, the function
/// constructors) and freezes the prototypes all modules share, so one
/// module cannot change `Array.prototype` or `for…in` for the others.
/// Development bundles keep them (REPL-style debugging; hot updates are
/// evaluated from Rust and need neither).
const HARDEN: &str = r#"
(() => {
  const deny = (label) => function () {
    throw new TypeError(label + " is disabled: nsh runs no code from strings (development builds allow it)");
  };
  const replace = (fn, value) => {
    Object.defineProperty(Object.getPrototypeOf(fn), "constructor", {
      value, writable: false, enumerable: false, configurable: false,
    });
  };
  const blocked = deny("the Function constructor");
  blocked.prototype = Function.prototype;
  replace(function () {}, blocked);
  replace(async function () {}, deny("the AsyncFunction constructor"));
  replace(function* () {}, deny("the GeneratorFunction constructor"));
  replace(async function* () {}, deny("the AsyncGeneratorFunction constructor"));
  Object.defineProperty(globalThis, "Function", { value: blocked, writable: false, configurable: false });
  delete globalThis.eval;
  // Not Error.prototype: `this.name = "MyError"` in a subclass constructor
  // would then fail (an inherited read-only property blocks the assignment).
  for (const proto of [
    Object.prototype, Array.prototype, Function.prototype, String.prototype, Number.prototype,
    Boolean.prototype, Symbol.prototype, RegExp.prototype, Promise.prototype, Map.prototype,
    Set.prototype, WeakMap.prototype, WeakSet.prototype, Date.prototype,
    Object.getPrototypeOf(Uint8Array.prototype),
  ]) {
    Object.freeze(proto);
  }
})();
"#;

/// Installs the hardening. Runs after the host's own globals and before the
/// bundle.
pub(crate) fn harden(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.eval::<(), _>(HARDEN)
}
