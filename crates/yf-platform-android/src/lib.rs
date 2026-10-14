//! Android backend for yf.
//!
//! Built as `libyf.so`, loaded by the Java runtime in `java/dev/yf`. The
//! split mirrors the Apple backend:
//!
//! - The engine thread runs JS and layout. Text is typeset there with
//!   `StaticLayout` (through JNI), and every commit is encoded into one
//!   `byte[]` (+ an object array for strings and layouts) that a single JNI
//!   call hands to the UI thread, where `Mounter.java` applies it.
//! - Native animations (the `yf-anim` graph) run on the UI thread in Rust:
//!   `Choreographer` ticks call in, and changed values come back as a
//!   compact array.
//!
//! On non-Android targets this crate is empty.

#![cfg(target_os = "android")]

mod anim;
#[cfg(feature = "dev")]
mod dev;
mod encode;
mod host;
mod java;
mod logger;
mod net;
mod text;

use std::ffi::c_void;

use jni::{JavaVM, sys::jint};

/// Called by the VM when `System.loadLibrary("yf")` runs.
#[unsafe(no_mangle)]
pub extern "system" fn JNI_OnLoad(vm: JavaVM, _: *mut c_void) -> jint {
    logger::init();
    if let Err(err) = java::init(vm) {
        log::error!("yf: JNI setup failed: {err}");
    }
    jni::sys::JNI_VERSION_1_6
}
