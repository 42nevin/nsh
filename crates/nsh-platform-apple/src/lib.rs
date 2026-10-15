//! UIKit backend for nsh (iOS, iOS simulator and Mac Catalyst).
//!
//! - [`AppView`] runs one nsh app inside a `UIView`: embed it in any UIKit
//!   hierarchy (it can also load from a dev server in development builds),
//!   or let [`run_app`] create a window for a standalone app. [`Surface`]
//!   is the lower-level engine host it is built on.
//! - Mount batches from the engine thread are applied on the main thread in
//!   one main-queue hop per commit.
//! - Text is measured on the engine thread with the same attributed strings
//!   the labels render, so layout and rendering agree.
//!
//! On non-UIKit targets this crate is empty.

#![cfg(target_os = "ios")]

mod anim;
mod app;
mod blur;
mod canvas;
mod decor;
#[cfg(feature = "dev")]
pub mod dev;
mod image;
mod input;
mod interaction;
mod mount;
mod native_view;
mod navigation;
mod net;
mod press;
mod surface;
mod system;
mod target;
mod text;
mod text_view;
mod touch;
mod video;
mod view;
mod visual;
mod web;
mod zoom;

pub use nsh_engine::Bundle;

pub use crate::{
    app::{AppOptions, AppSource, run_app},
    image::AssetBase,
    surface::{Surface, SurfaceOptions},
    system::{preferred_status_bar_style, prefers_status_bar_hidden, status_bar_animation},
    text::CoreTextEngine,
    view::AppView,
};
