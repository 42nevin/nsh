//! Opt-in public AppKit accessibility regression fixture.
//!
//! Build with `cargo test -p nsh-platform-macos --features accessibility-tests
//! --test accessibility --no-run`, then drive the produced executable with the
//! adjacent external SDK client in a trusted, isolated macOS test environment.
//! The external client supplies fresh owned IPC/state paths and finite budgets.
//! Ordinary package tests do not enable this target or start AppKit.

#[cfg(target_os = "macos")]
mod target;

fn main() {
    #[cfg(target_os = "macos")]
    target::run();
    #[cfg(not(target_os = "macos"))]
    panic!("the public AppKit accessibility fixture requires macOS");
}
