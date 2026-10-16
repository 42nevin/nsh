//! Styles: corners (also continuous), borders, shadows, clipping, 3D
//! transforms (as the TSX gallery's `demos/Styles.tsx`).
//!
//! Many of TS's style properties have no typed style method yet: one
//! corner's radius, per-side border widths and colors, border styles and
//! curves, shadows of any color, radius, opacity and offset (the cards take
//! the nearest of Tailwind's shadows), static 3D and skew transforms (bound
//! to native values of constant value here), `transformOrigin`,
//! `backfaceVisibility` (the flip card fades each face out edge-on),
//! `fontVariant`, `writingDirection`. The comments on each card say what
//! it leaves out.

use nsh_core::ui::{AnyElement, NativeValue, Text, TransformKind, View, prelude::*};

fn card() -> View {
    view()
        .w(px(150.0))
        .h(px(110.0))
        .items_center()
        .justify_center()
        .bg(rgb(0xffffff))
}

fn label(content: impl Into<SharedString>) -> Text {
    text(content)
        .text_size(px(13.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(0x1f2328))
}

/// A label under a card's bottom edge.
fn label_below(content: &'static str) -> View {
    view().absolute().bottom(px(-18.0)).child(label(content))
}

/// Two faces back to back: each hides while turned away.
#[derive(Debug, Default)]
struct FlipCard {
    angle: Option<NativeValue>,
    perspective: Option<NativeValue>,
    flipped: bool,
}

impl Component for FlipCard {
    const NAME: &'static str = "gallery.styles.flip_card";
    type Props = ();
    type Event = NoEvent;

    fn setup(&mut self, _: &(), cx: &mut Context<'_, Self>) {
        self.angle = Some(cx.native_value(0.0));
        self.perspective = Some(cx.native_value(600.0));
    }

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        let flip =
            view()
                .w(px(150.0))
                .h(px(110.0))
                .on_press(cx.listener(|this: &mut Self, _, _| {
                    this.flipped = !this.flipped;
                    if let Some(angle) = &this.angle {
                        let to = if this.flipped { 180.0 } else { 0.0 };
                        let _ = angle.timing(to, 500.0, Easing::EaseInOut);
                    }
                }));
        let (Some(angle), Some(perspective)) = (&self.angle, &self.perspective) else {
            return flip;
        };
        let face = |color: u32| {
            card()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .rounded(px(14.0))
                .bg(rgb(color))
                .bind_transform(TransformKind::Perspective, perspective)
        };
        // TS hides each face while it is turned away
        // (`backfaceVisibility: "hidden"`), which the typed styles lack:
        // each face fades out when the angle turns it edge-on.
        let shown = angle.interpolate(&[89.0, 91.0], &[1.0, 0.0]);
        let hidden = angle.interpolate(&[89.0, 91.0], &[0.0, 1.0]);
        flip.child(
            face(0xddf4ff)
                .bind_transform(TransformKind::RotateY, angle)
                .bind_opacity(&shown)
                .child(label("Front (tap)")),
        )
        .child(
            face(0xfff8c5)
                .bind_transform(
                    TransformKind::RotateY,
                    &angle.interpolate(&[0.0, 180.0], &[180.0, 360.0]),
                )
                .bind_opacity(&hidden)
                .child(label("Back: backfaceVisibility")),
        )
    }
}

/// Constant transforms of the 3D cards (TS: `{ perspective: 400 }`,
/// `{ rotateY: "35deg" }`, ...), bound as native values: the typed styles
/// have `rotate`, `scale` and translations only.
#[derive(Debug)]
struct Tilts {
    perspective: NativeValue,
    rotate_y: NativeValue,
    rotate_x: NativeValue,
    skew_x: NativeValue,
}

/// The demo's page.
#[derive(Debug, Default)]
pub(crate) struct Styles {
    /// The yellow card shows what overflows it.
    unclipped: bool,
    /// The square of the zIndex card on top.
    front: usize,
    tilts: Option<Tilts>,
}

impl Styles {
    fn clipping(&self, cx: &RenderCx<'_, Self>) -> AnyElement {
        card()
            .bg(rgb(0xfff8c5))
            .rounded(px(24.0))
            .border_2()
            .border_color(rgb(0x9a6700))
            .when(!self.unclipped, |v| v.overflow_hidden())
            // TS: shadowOpacity 0.3, shadowRadius 8, offset (0, 4).
            .shadow_md()
            .on_press(cx.listener(|this: &mut Self, _, cx| {
                this.unclipped = !this.unclipped;
                cx.notify();
            }))
            .child(
                view()
                    .absolute()
                    .left(px(-30.0))
                    .top(px(-30.0))
                    .size(px(90.0))
                    .rounded(px(45.0))
                    .bg(rgb(0xbf3989)),
            )
            .child(label(if self.unclipped {
                "Visible (tap)"
            } else {
                "Clipped (tap)"
            }))
            .into_element()
    }

    fn stacking(&self, cx: &RenderCx<'_, Self>) -> AnyElement {
        let squares = [0xfd8c73, 0x54aeff, 0x4ac26b]
            .into_iter()
            .enumerate()
            .map(|(i, color)| {
                view()
                    .absolute()
                    .left(px(20.0 + i as f32 * 30.0))
                    .top(px(10.0 + i as f32 * 22.0))
                    .w(px(70.0))
                    .h(px(56.0))
                    .rounded(px(10.0))
                    .bg(rgb(color))
                    .z_index(if self.front == i { 10 } else { 0 })
                    // TS: shadowOpacity 0.2, shadowRadius 4, offset (0, 2).
                    .shadow_sm()
            });
        card()
            .bg(Color::TRANSPARENT)
            .on_press(cx.listener(|this: &mut Self, _, cx| {
                this.front = (this.front + 1) % 3;
                cx.notify();
            }))
            .children(squares)
            .child(label_below("zIndex (tap)"))
            .into_element()
    }

    fn tilted(&self) -> [AnyElement; 3] {
        let card = |color: u32, content: &'static str| card().bg(rgb(color)).child(label(content));
        let Some(t) = &self.tilts else {
            return [
                card(0xddf4ff, "rotateY + perspective").into_element(),
                card(0xfbefff, "rotateX").into_element(),
                card(0xfff1e5, "skewX (iOS)").into_element(),
            ];
        };
        [
            card(0xddf4ff, "rotateY + perspective")
                .bind_transform(TransformKind::Perspective, &t.perspective)
                .bind_transform(TransformKind::RotateY, &t.rotate_y)
                .into_element(),
            card(0xfbefff, "rotateX")
                .bind_transform(TransformKind::Perspective, &t.perspective)
                .bind_transform(TransformKind::RotateX, &t.rotate_x)
                .into_element(),
            card(0xfff1e5, "skewX (iOS)")
                .bind_transform(TransformKind::SkewX, &t.skew_x)
                .into_element(),
        ]
    }

    /// Rows, `marginStart` and text under a right-to-left layout.
    fn right_to_left(cx: &RenderCx<'_, Self>) -> AnyElement {
        let boxes = [(0xfd8c73, "1"), (0x54aeff, "2"), (0x4ac26b, "3")]
            .into_iter()
            .enumerate()
            .map(|(i, (color, n))| {
                // TS also rounds its top-start corner (22): the typed
                // styles have no start / end corners.
                view()
                    .size(px(44.0))
                    .rounded(px(8.0))
                    .items_center()
                    .justify_center()
                    .bg(rgb(color))
                    .when(i == 0, |v| v.ms(px(16.0)))
                    .child(
                        text(n)
                            .text_color(rgb(0xffffff))
                            .font_weight(FontWeight::BOLD),
                    )
            });
        let tile = |n: usize, color: u32| {
            view()
                .size(px(40.0))
                .rounded(px(6.0))
                .bg(rgb(color))
                .items_center()
                .justify_center()
                .child(text(n.to_string()))
        };
        let muted = rgb(0x6e7781);
        view()
            .w_full()
            .gap(px(10.0))
            .p(px(14.0))
            .bg(rgb(0xffffff))
            .rounded(px(12.0))
            // TS's "Flip" button calls `I18nManager.forceRTL`: a Rust
            // component can read the direction (`cx.rtl()`), not set it.
            .child(
                view().flex_row().items_center().gap_2().child(
                    text(if cx.rtl() {
                        "Right to left"
                    } else {
                        "Left to right"
                    })
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD),
                ),
            )
            .child(view().flex_row().gap_2().children(boxes))
            .child(text(
                "Text aligns to the start side; the first box has marginStart 16 and a round top-start corner.",
            ))
            .child(
                text("A horizontal scroll view and a horizontal list start at the start side:")
                    .text_size(px(13.0))
                    .text_color(muted),
            )
            .child(
                scroll()
                    .horizontal()
                    .shows_indicator(false)
                    .h(px(40.0))
                    .child(
                        view()
                            .flex_row()
                            .gap(px(6.0))
                            .px_1()
                            .children((1..=12).map(|n| tile(n, 0xddf4ff))),
                    ),
            )
            .child(
                virtual_list(30, move |i, _| tile(i + 1, 0xfff8c5).me(px(6.0)))
                    .horizontal()
                    .estimated_item_size(46.0)
                    .shows_indicator(false)
                    .h(px(40.0)),
            )
            .child(
                text("textAlign left (the start side right to left)")
                    .text_left()
                    .text_color(muted),
            )
            // TS sets `writingDirection: "rtl"`: the typed styles have no
            // writing direction.
            .child(text("مرحبا بالعالم — writingDirection rtl").text_color(muted))
            .into_element()
    }
}

impl Component for Styles {
    const NAME: &'static str = "gallery.styles";
    type Props = ();
    type Event = NoEvent;

    fn setup(&mut self, _: &(), cx: &mut Context<'_, Self>) {
        self.tilts = Some(Tilts {
            perspective: cx.native_value(400.0),
            rotate_y: cx.native_value(35.0),
            rotate_x: cx.native_value(40.0),
            skew_x: cx.native_value(-12.0),
        });
    }

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        let purple = rgb(0x8250df);
        let blue = rgb(0x0969da);
        let green = rgb(0x1a7f37);
        let figures = view()
            .flex_row()
            .flex_wrap()
            .gap_6()
            .p_6()
            .justify_center()
            // TS: shadowOpacity 0.18, shadowRadius 12, offset (0, 6).
            .child(card().rounded(px(16.0)).shadow_lg().child(label("Shadow")))
            .child(
                card()
                    .rounded_t(px(28.0))
                    .border_2()
                    .border_color(purple)
                    .child(label("Top corners")),
            )
            // TS rounds the corners 44 / 6 / 28 / 0 and casts a blue
            // shadow (opacity 0.35, radius 10, offset (4, 8)).
            .child(
                card()
                    .bg(rgb(0xddf4ff))
                    .border(3.0)
                    .border_color(blue)
                    .shadow_lg()
                    .child(label("Mixed radii")),
            )
            .child(self.clipping(cx))
            // TS rounds its top left and bottom right corners (36), with a
            // shadow of opacity 0.3, radius 10, offset (0, 6).
            .child(
                image("https://picsum.photos/seed/nsh/300/220")
                    .w(px(150.0))
                    .h(px(110.0))
                    .bg(rgb(0xe1e4e8))
                    .shadow_lg(),
            )
            // TS: shadowOpacity 0.5, shadowRadius 3, offset (2, 2) on a
            // transparent view, shadowing its text.
            .child(
                card().bg(Color::TRANSPARENT).shadow_sm().child(
                    text("Text shadow")
                        .text_size(px(22.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(0x1f2328)),
                ),
            )
            // TS rounds the text's top left and bottom right corners (14).
            .child(
                view().w(px(150.0)).items_center().child(
                    text("Text corners")
                        .text_size(px(15.0))
                        .text_color(rgb(0xffffff))
                        .bg(green)
                        .px_3()
                        .py(px(6.0)),
                ),
            )
            // TS's borders: top 6 red, right 2 green, bottom 4 blue, left 1
            // purple; the typed styles have one border color, and per-side
            // widths of 1 only.
            .child(card().child(label("Per-side borders")))
            // TS dashes the first border and dots the second
            // (`borderStyle`).
            .child(
                card()
                    .gap_2()
                    .child(
                        view()
                            .w(px(120.0))
                            .h(px(30.0))
                            .rounded(px(8.0))
                            .border_2()
                            .border_color(blue),
                    )
                    .child(
                        view()
                            .w(px(120.0))
                            .h(px(30.0))
                            .rounded(px(8.0))
                            .border(3.0)
                            .border_color(rgb(0xbf3989)),
                    )
                    .child(label("Dashed / dotted")),
            )
            // TS curves the last three continuously (`borderCurve`) and
            // gives the third a top left corner of 4.
            .child(
                card()
                    .w(px(324.0))
                    .gap_2()
                    .child(
                        view()
                            .flex_row()
                            .gap_3()
                            .child(view().size(px(64.0)).rounded(px(20.0)).bg(purple))
                            .child(view().size(px(64.0)).rounded(px(20.0)).bg(purple))
                            .child(
                                view()
                                    .size(px(64.0))
                                    .rounded(px(20.0))
                                    .border(3.0)
                                    .border_color(blue),
                            )
                            .child(view().w(px(64.0)).h(px(30.0)).rounded(px(15.0)).bg(green)),
                    )
                    .child(label("Circular / continuous corners")),
            )
            .child(self.stacking(cx))
            .children(self.tilted())
            .child(
                card().bg(rgb(0x1f2328)).opacity(0.45).child(
                    text("opacity 0.45")
                        .text_color(rgb(0xffffff))
                        .font_weight(FontWeight::SEMIBOLD),
                ),
            )
            .child(
                card()
                    .rounded(px(16.0))
                    .elevation(10.0)
                    .child(label("elevation 10 (Android)")),
            )
            // TS dashes the outline and rotates the square about its top
            // left corner (`transformOrigin`); here it turns about its
            // center.
            .child(
                card()
                    .bg(Color::TRANSPARENT)
                    .child(
                        view()
                            .size(px(60.0))
                            .border_1()
                            .border_color(rgb(0xd0d7de))
                            .absolute()
                            .left(px(45.0))
                            .top(px(15.0)),
                    )
                    .child(
                        view()
                            .size(px(60.0))
                            .absolute()
                            .left(px(45.0))
                            .top(px(15.0))
                            .bg(rgba(0x8250_df99))
                            .rotate(30.0),
                    )
                    .child(label_below("transformOrigin top left")),
            )
            .child(FlipCard::element(()))
            // TS's last two lines use tabular digits, the next card small
            // caps and old-style digits (`fontVariant`).
            .child(
                card()
                    .items_start()
                    .px_4()
                    .gap(px(2.0))
                    .child(
                        text("Proportional / tabular")
                            .text_size(px(13.0))
                            .text_color(rgb(0x6e7781)),
                    )
                    .children(
                        ["1,111.11", "8,888.88", "1,111.11", "8,888.88"]
                            .map(|n| text(n).text_size(px(18.0))),
                    ),
            )
            .child(
                card()
                    .child(text("Small Caps").text_size(px(18.0)))
                    .child(text("Oldstyle 1234567890").text_size(px(18.0))),
            )
            .child(Self::right_to_left(cx));
        view().flex_1().child(scroll().flex_1().child(figures))
    }
}
