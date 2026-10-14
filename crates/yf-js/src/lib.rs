//! QuickJS host for yf.
//!
//! [`Host`] owns one QuickJS runtime and context, installs the host APIs the
//! runtime needs (`console`, timers, the op bridge), decodes the op buffer
//! produced by `js/runtime/index.ts` into [`yf_core::Op`]s and dispatches
//! native events back into JS.
//!
//! A host is single-threaded (`!Send`): create and use it on the engine
//! thread.

mod bytecode;
mod console;
mod error;
mod host;
mod protocol;
mod timers;

pub use crate::{
    bytecode::{BUNDLE_NAME, BYTECODE_VERSION, compile_bytecode},
    error::JsError,
    host::{ErrorHandler, Host},
};
