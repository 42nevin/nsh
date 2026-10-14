//! UIKit backend for yf (iOS, iOS simulator and Mac Catalyst).
//!
//! - [`Surface`] hosts one yf app inside a `UIView`; embed it in any UIKit
//!   hierarchy, or let [`run_app`] create a window for a standalone app.
//! - Mount batches from the engine thread are applied on the main thread in
//!   one main-queue hop per commit.
//! - Text is measured on the engine thread with the same attributed strings
//!   the labels render, so layout and rendering agree.
//!
//! On non-UIKit targets this crate is empty.

#![cfg(target_os = "ios")]

mod anim;
mod app;
#[cfg(feature = "dev")]
pub mod dev;
mod image;
mod mount;
mod surface;
mod target;
mod text;
mod text_view;

pub use yf_engine::Bundle;

pub use crate::{
    app::{AppOptions, AppSource, run_app},
    image::AssetBase,
    surface::{Surface, SurfaceOptions},
    text::CoreTextEngine,
};
