//! The demos and the gallery's index.

mod animations;
mod canvas;
mod components;
mod form;
mod gestures;
mod images;
mod lists;
mod media;
mod mobile_ui;
mod navigation;
mod photos;
mod physics;
mod scrolling;
mod styles;
mod system;
mod text_demo;
mod typed_styles;

use nsh_core::ui::{AnyElement, Screen, prelude::*};

pub(crate) use self::navigation::BackIntercept;
use crate::{Gallery, nav, ui::colors};

/// A demo of the index: its name, what it shows and its screen's content.
struct Demo {
    name: &'static str,
    detail: &'static str,
    content: fn() -> AnyElement,
}

const DEMOS: &[Demo] = &[
    Demo {
        name: "Photos",
        detail: "A long recycled list: images, pull to refresh, paging",
        content: || photos::Photos::element(()).into_element(),
    },
    Demo {
        name: "Lists",
        detail: "Columns, sticky sections, a chat, horizontal lists",
        content: || lists::Lists::element(()).into_element(),
    },
    Demo {
        name: "Scrolling",
        detail: "Paging, snapping, zoom, scroll events, prepending, keyboard taps",
        content: || scrolling::Scrolling::element(()).into_element(),
    },
    Demo {
        name: "Navigation",
        detail: "Header options, modals, safe areas",
        content: || navigation::Navigation::element(()).into_element(),
    },
    Demo {
        name: "Form",
        detail: "Inputs, keyboard types, the keyboard over the content",
        content: || form::Form::element(()).into_element(),
    },
    Demo {
        name: "Mobile UI",
        detail: "nsh/ui/mobile from Rust: a ListView and a Group laying out rows",
        content: || mobile_ui::MobileUi::element(()).into_element(),
    },
    Demo {
        name: "Components",
        detail: "Press feedback, sheet, layout animation, input accessory, keyboard, modal options",
        content: || components::Components::element(()).into_element(),
    },
    Demo {
        name: "Gestures",
        detail: "Pinch, rotate, press states, hit slop, pointer events",
        content: || gestures::Gestures::element(()).into_element(),
    },
    Demo {
        name: "Physics",
        detail: "Drags and springs on the UI thread",
        content: || physics::Physics::element(()).into_element(),
    },
    Demo {
        name: "Animations",
        detail: "Easing, sequence / parallel / stagger / loop, springs, interpolation, diffClamp",
        content: || animations::Animations::element(()).into_element(),
    },
    Demo {
        name: "Images",
        detail: "Resize modes, tint, GIF / WebP, placeholders, blur, fade, events",
        content: || images::Images::element(()).into_element(),
    },
    Demo {
        name: "Canvas",
        detail: "2D drawing: paths, gradients, text, images, native animation",
        content: || canvas::CanvasDemo::element(()).into_element(),
    },
    Demo {
        name: "Typed styles",
        detail: "What Tailwind classes do, as typed style methods: variants, grid, shadows, gradients, motion",
        content: || typed_styles::TypedStyles::element(()).into_element(),
    },
    Demo {
        name: "Styles",
        detail: "Corners (also continuous), borders, shadows, clipping, 3D transforms",
        content: || styles::Styles::element(()).into_element(),
    },
    Demo {
        name: "Text",
        detail: "Fonts, spans, inline views and images, decorations, truncation, shadows",
        content: || text_demo::TextDemo::element(()).into_element(),
    },
    Demo {
        name: "Web and video",
        detail: "WKWebView / WebView, AVPlayer / MediaPlayer",
        content: || media::Media::element(()).into_element(),
    },
    Demo {
        name: "System",
        detail: "Status bar, alerts, action sheets, environment, timers",
        content: || system::System::element(()).into_element(),
    },
];

/// The index: a list of the demos; a tap pushes one.
pub(crate) fn index(cx: &mut RenderCx<'_, Gallery>) -> Screen {
    let rows = DEMOS.iter().enumerate().map(|(i, demo)| {
        view()
            .px_4()
            .py_3()
            .gap(px(2.0))
            .when(i > 0, |v| v.border_t_1().border_color(rgb(0xeaeef2)))
            .active(|s| s.opacity(0.5))
            .role(AccessibilityRole::Button)
            .on_press(cx.listener(move |_: &mut Gallery, _, cx| {
                let (name, content) = (demo.name, demo.content);
                nav::push(cx, name, move |_| screen().title(name).child(content()));
            }))
            .child(
                text(demo.name)
                    .text_size(px(17.0))
                    .text_color(colors::text()),
            )
            .child(
                text(demo.detail)
                    .text_size(px(13.0))
                    .text_color(colors::muted()),
            )
            .into_element()
    });
    let intercept = *cx.global::<BackIntercept>();
    screen().key("index").title("Rust Gallery").child(
        scroll()
            .flex_1()
            .bg(colors::background())
            .when(intercept.on, |s| {
                s.child(
                    text(format!(
                        "Back is intercepted ({} presses): release it in Navigation",
                        intercept.count
                    ))
                    .mx_4()
                    .mt_4()
                    .text_size(px(13.0))
                    .text_color(colors::muted()),
                )
            })
            .child(
                view()
                    .m_4()
                    .rounded(px(12.0))
                    .bg(colors::surface())
                    .overflow_hidden()
                    .children(rows),
            ),
    )
}
