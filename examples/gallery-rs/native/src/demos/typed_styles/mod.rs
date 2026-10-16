//! Typed styles: What Tailwind classes do, as typed style methods: variants, grid, shadows, gradients, motion (as the TSX gallery's `demos/Tailwind.tsx`).
//!
//! The same cards as the Tailwind demo, written with `Styled`'s methods
//! instead of class strings; each card says how its classes map. `dark:`
//! colors are [`AdaptiveColor`]s (a light and a dark value, switched
//! natively without a render); what has no typed method is composed from
//! what has, or left out, and the card says which.

mod palette;

use std::sync::Arc;

use nsh_core::{
    Rect,
    canvas::{CanvasGradient, CanvasOp, Drawing, GradientShape, Paint},
    ui::{AnyElement, Text, View, prelude::*},
};

use self::palette::{
    AMBER, BLUE, EMERALD, FUCHSIA, GREEN, INDIGO, ORANGE, PINK, RED, ROSE, SKY, VIOLET, YELLOW,
    ZINC,
};

/// The photo of the filters' card.
const PHOTO: &str = "https://picsum.photos/seed/nsh-tailwind/600/400";

/// White.
const WHITE: Color = rgb(0xffffff);

/// The TSX gallery's `brand` color (its `nsh.json` Tailwind theme).
const BRAND_50: Color = rgb(0xf5f3ff);
const BRAND_500: Color = rgb(0x7c3aed);
const BRAND_600: Color = rgb(0x6d28d9);
const BRAND_700: Color = rgb(0x5b21b6);

/// The TSX gallery's `rounded-card` radius.
const CARD_RADIUS: f32 = 14.0;

/// `light`, and `dark` in dark appearance (Tailwind's `dark:`).
fn adaptive(light: Color, dark: Color) -> AdaptiveColor {
    AdaptiveColor::new(light, dark)
}

/// `text-zinc-500 dark:text-zinc-400`.
fn secondary() -> AdaptiveColor {
    adaptive(ZINC.shade(500), ZINC.shade(400))
}

/// A titled card: the Tailwind demo's `Section`, with a line on how its
/// classes map to typed methods.
fn card(title: &str, how: &str, body: impl IntoElement) -> AnyElement {
    view()
        .gap_2()
        .child(
            text(title.to_uppercase())
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                // `tracking-wide`: 0.025em.
                .letter_spacing(0.3)
                .text_color(secondary()),
        )
        .child(
            view()
                .gap_3()
                .rounded(px(CARD_RADIUS))
                .bg(adaptive(WHITE, ZINC.shade(800)))
                .p_4()
                .child(body)
                .child(text(how.to_owned()).text_xs().text_color(secondary())),
        )
        .into_element()
}

/// A caption under a sample.
fn label(content: impl Into<SharedString>) -> Text {
    text(content.into())
        .text_center()
        .text_size(px(11.0))
        .text_color(secondary())
}

/// A sample with its caption under it.
fn captioned(sample: impl IntoElement, caption: &str) -> AnyElement {
    view()
        .gap_1()
        .child(sample)
        .child(label(caption.to_owned()))
        .into_element()
}

/// The start and end of a CSS linear gradient along `direction` (a unit
/// vector) over a `w` × `h` box: through the center, long enough that the
/// corners get the end colors.
fn gradient_line(direction: (f32, f32), w: f32, h: f32) -> ((f32, f32), (f32, f32)) {
    let half = (w * direction.0.abs() + h * direction.1.abs()) / 2.0;
    let (cx, cy) = (w / 2.0, h / 2.0);
    (
        (cx - direction.0 * half, cy - direction.1 * half),
        (cx + direction.0 * half, cy + direction.1 * half),
    )
}

/// `colors` at even offsets from 0 to 1.
fn stops(colors: &[Color]) -> Vec<(f32, Color)> {
    let last = (colors.len() - 1).max(1) as f32;
    colors
        .iter()
        .enumerate()
        .map(|(i, &c)| (i as f32 / last, c))
        .collect()
}

/// A `w` × `h` canvas filled with a linear gradient along `direction`.
fn linear_gradient(direction: (f32, f32), colors: &[Color], w: f32, h: f32) -> CanvasView {
    let (from, to) = gradient_line(direction, w, h);
    canvas(
        Painter::new()
            .fill_linear_gradient(from, to, stops(colors))
            .fill_rect(0.0, 0.0, w, h),
    )
}

/// A `w` × `h` canvas filled with a radial or conic gradient: `Painter`
/// only has linear ones, so the fill style is the canvas operation itself.
fn shaped_gradient(shape: GradientShape, colors: &[Color], w: f32, h: f32) -> CanvasView {
    let mut drawing = Drawing::default();
    drawing
        .ops
        .push(CanvasOp::FillStyle(Paint::Gradient(Arc::new(
            CanvasGradient {
                shape,
                stops: stops(colors),
            },
        ))));
    canvas(Painter::from(drawing).fill_rect(0.0, 0.0, w, h))
}

/// The demo's page: the Tailwind demo's cards in typed styles.
#[derive(Default)]
pub(crate) struct TypedStyles {
    /// The first button's presses.
    count: u32,
    /// The third button is disabled.
    busy: bool,
    /// The group card is pressed (its line changes color).
    card_pressed: bool,
    /// The gradient card's size, once laid out (its canvas fills it).
    gradient_size: Option<(f32, f32)>,
    /// `animate-spin`: degrees.
    spin: Option<NativeValue>,
    /// `animate-ping`: 0 to 1 (scale 1 to 2, opacity 1 to 0).
    ping: Option<NativeValue>,
    /// `animate-pulse`: opacity.
    pulse: Option<NativeValue>,
    /// `animate-bounce`: points down.
    bounce: Option<NativeValue>,
    /// The loops moving them.
    motion: Vec<Task<()>>,
}

/// Animates `value` through `steps` (to, milliseconds, easing) from
/// `start`, over and over, until the task is dropped.
fn looped(
    cx: &mut Context<'_, TypedStyles>,
    value: &NativeValue,
    start: f32,
    steps: Vec<(f32, f32, Easing)>,
) -> Task<()> {
    let value = value.clone();
    cx.spawn(move |_, _| async move {
        loop {
            value.set(start);
            for (to, ms, easing) in steps.iter().cloned() {
                if !value.timing(to, ms, easing).await {
                    // Stopped: the page is going.
                    return;
                }
            }
        }
    })
}

impl Component for TypedStyles {
    const NAME: &'static str = "gallery.typed_styles";
    type Props = ();
    type Event = NoEvent;

    fn setup(&mut self, _: &(), cx: &mut Context<'_, Self>) {
        let spin = cx.native_value(0.0);
        let ping = cx.native_value(0.0);
        let pulse = cx.native_value(1.0);
        let bounce = cx.native_value(-8.0);
        // Tailwind's keyframes and curves.
        let pulse_curve = Easing::Bezier(0.4, 0.0, 0.6, 1.0);
        self.motion = vec![
            looped(cx, &spin, 0.0, vec![(360.0, 1000.0, Easing::Linear)]),
            looped(
                cx,
                &ping,
                0.0,
                vec![(1.0, 1000.0, Easing::Bezier(0.0, 0.0, 0.2, 1.0))],
            ),
            looped(
                cx,
                &pulse,
                1.0,
                vec![(0.5, 1000.0, pulse_curve), (1.0, 1000.0, pulse_curve)],
            ),
            // `translateY(-25%)` of its 32pt.
            looped(
                cx,
                &bounce,
                -8.0,
                vec![
                    (0.0, 500.0, Easing::Bezier(0.0, 0.0, 0.2, 1.0)),
                    (-8.0, 500.0, Easing::Bezier(0.8, 0.0, 1.0, 1.0)),
                ],
            ),
        ];
        self.spin = Some(spin);
        self.ping = Some(ping);
        self.pulse = Some(pulse);
        self.bounce = Some(bounce);
    }

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        // The cards' content width: the page's and the card's padding off
        // the window (grid tracks and grow factors are computed from it).
        let content = (cx.window().width - 64.0).max(0.0);
        let cards = [
            card(
                "Buttons: active, hover, transition, disabled",
                ".active(|s| s.bg(..)), .hover(..), .active(|s| s.scale(0.95)); disabled: \
                 .disabled(b) and .when(b, |v| v.opacity(0.4)). No typed transitions \
                 (transition-colors, duration-150) nor decoration styles (dotted, offset).",
                self.buttons(cx),
            ),
            card(
                "Cards: shadow, ring, group, gradient",
                ".shadow_lg(), .active(|s| s.scale(0.98)); ring-1 is a 1pt border; group-active \
                 is press-in / press-out state and a render; the gradient is a canvas. No shadow \
                 colors (shadow-brand-500/40); the logo is a photo (a Rust crate ships no assets).",
                self.cards(cx),
            ),
            card(
                "Flex, grid, breakpoints",
                ".flex_row(), .flex_1(), .w_10(); flex-[2] and the grid's col-span / row-span have \
                 no typed methods (.grid() has no tracks): those widths come from cx.window(), as \
                 does sm: (.when(width >= 640.0, |v| v.flex_row())).",
                layout(cx.window().width, content),
            ),
            card(
                "Palette",
                ".bg(color): the typed API has no Tailwind palette, so the demo copies the shades.",
                colors(),
            ),
            card(
                "Shadows, rings, outlines",
                ".shadow_xs() to .shadow_2xl(). Inset shadows, rings, ring offsets, outlines \
                 (dashed, offset) and shadow colors have no typed methods: borders stand in.",
                shadows(),
            ),
            card(
                "Gradients",
                "No typed gradients: canvases draw them (Painter::fill_linear_gradient; radial and \
                 conic with the canvas operation, which Painter has no method for).",
                gradients(),
            ),
            card(
                "Typography",
                ".text_3xl(), .font_weight(..), .italic(), .font_family(\"serif\"), \
                 .letter_spacing(..), .underline(), .line_through(), number_of_lines(..). No \
                 decoration styles or colors, tabular-nums, uppercase (the string is) nor text \
                 shadows.",
                typography(),
            ),
            card(
                "Filters, backdrop blur, blend modes",
                "Image::blur_radius(8.0) is blur-sm; brightness-50 is a black/50 overlay. \
                 Grayscale, sepia, backdrop blur and blend modes have no typed methods (✗).",
                effects(),
            ),
            card(
                "Animations and transforms",
                "Native values looped in tasks: .bind_rotate(), .bind_scale(), .bind_opacity(), \
                 .bind_translate_y() with timing(..) in Tailwind's curves; .rotate(12.0) and \
                 .scale(0.9) (no skew).",
                self.motion(),
            ),
            card(
                "first:, even:, last:",
                "Plain Rust from the row's index (.when(i % 2 == 1, ..)); .active(|s| s.bg(..)).",
                lists(),
            ),
            card(
                "Inputs",
                ".focus(|s| s.border_color(..)), .placeholder_color(..), .selection_color(..) \
                 (caret). No focus ring.",
                inputs(cx),
            ),
        ];
        view().flex_1().child(
            scroll()
                .flex_1()
                .bg(adaptive(ZINC.shade(100), ZINC.shade(900)))
                .child(view().gap_5().p_4().children(cards)),
        )
    }
}

impl TypedStyles {
    /// Press states, hover, disabled.
    fn buttons(&self, cx: &mut RenderCx<'_, Self>) -> AnyElement {
        let busy = self.busy;
        view()
            .gap_3()
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        view()
                            .rounded_lg()
                            .bg(BRAND_600)
                            .px_4()
                            .py_2()
                            .active(|s| s.bg(BRAND_700))
                            .hover(|s| s.bg(BRAND_500))
                            .on_press(cx.listener(|this: &mut Self, _, cx| {
                                this.count += 1;
                                cx.notify();
                            }))
                            .child(
                                text(format!("Pressed {}×", self.count))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(WHITE),
                            ),
                    )
                    .child(
                        view()
                            .rounded_lg()
                            .border_1()
                            .border_color(adaptive(ZINC.shade(300), ZINC.shade(600)))
                            .px_4()
                            .py_2()
                            .active(|s| {
                                s.scale(0.95).bg(adaptive(ZINC.shade(100), ZINC.shade(700)))
                            })
                            .on_press(cx.listener(|this: &mut Self, _, cx| {
                                this.count = 0;
                                cx.notify();
                            }))
                            .child(
                                text("Reset")
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(adaptive(ZINC.shade(900), WHITE)),
                            ),
                    )
                    .child(
                        view()
                            .rounded_full()
                            .bg(EMERALD.shade(500))
                            .px_4()
                            .py_2()
                            .disabled(busy)
                            .when(busy, |v| v.opacity(0.4))
                            .active(|s| s.bg(EMERALD.shade(600)))
                            .on_press(cx.listener(|this: &mut Self, _, cx| {
                                this.busy = !this.busy;
                                cx.notify();
                            }))
                            .child(
                                text(if busy { "Disabled" } else { "Enabled" })
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(WHITE),
                            ),
                    ),
            )
            .child(
                view()
                    .self_start()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .active(|s| s.bg(adaptive(ZINC.shade(100), ZINC.shade(700))))
                    .on_press(cx.listener(|this: &mut Self, _, cx| {
                        this.busy = false;
                        cx.notify();
                    }))
                    .child(
                        text("Enable again")
                            .text_sm()
                            .text_color(BRAND_600)
                            .underline(),
                    ),
            )
            .into_element()
    }

    /// A card reacting to its press, and a gradient card.
    fn cards(&self, cx: &mut RenderCx<'_, Self>) -> AnyElement {
        let pressed = self.card_pressed;
        let press = |down: bool| {
            cx.listener(move |this: &mut Self, _, cx| {
                this.card_pressed = down;
                cx.notify();
            })
        };
        let group = view()
            .flex_row()
            .items_center()
            .gap_3()
            .rounded_2xl()
            .bg(adaptive(WHITE, ZINC.shade(700)))
            .p_3()
            .shadow_lg()
            .border_1()
            .border_color(rgba(0x0000_000d))
            .active(|s| s.scale(0.98))
            .on_press_in(press(true))
            .on_press_out(press(false))
            .child(image(PHOTO).size_12().rounded_xl())
            .child(
                view()
                    .flex_1()
                    .gap_0p5()
                    .child(
                        text("Group press")
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(adaptive(ZINC.shade(900), WHITE)),
                    )
                    .child(
                        text("Press the card: this line changes color")
                            .text_sm()
                            .text_color(if pressed {
                                BRAND_600.into()
                            } else {
                                adaptive(ZINC.shade(500), ZINC.shade(300))
                            }),
                    ),
            )
            .child(
                text("›")
                    .text_2xl()
                    .text_color(ZINC.shade(300))
                    .when(pressed, |t| t.translate_x(4.0)),
            );

        let gradient = view()
            .overflow_hidden()
            .rounded_2xl()
            .bg(BRAND_500)
            .p_5()
            .shadow_xl()
            .on_layout(cx.listener(|this: &mut Self, frame: &Rect, cx| {
                let size = Some((frame.width, frame.height));
                if this.gradient_size != size {
                    this.gradient_size = size;
                    cx.notify();
                }
            }))
            .children(self.gradient_size.map(|(w, h)| {
                // `bg-linear-to-br`: along the box's other diagonal's normal.
                let length = w.hypot(h).max(1.0);
                linear_gradient(
                    (h / length, w / length),
                    &[BRAND_500, FUCHSIA.shade(500), ORANGE.shade(400)],
                    w,
                    h,
                )
                .absolute()
                .inset_0()
            }))
            .child(
                text("Gradient card")
                    .text_xl()
                    .font_weight(FontWeight::BOLD)
                    .text_color(WHITE),
            )
            .child(
                text(
                    "canvas: Painter::fill_linear_gradient(..) brand-500, fuchsia-500, orange-400",
                )
                .mt_1()
                .text_sm()
                .text_color(rgba(0xffff_ffcc)),
            );
        view().gap_3().child(group).child(gradient).into_element()
    }

    /// Spinning, pinging, pulsing and bouncing, and a fixed transform.
    fn motion(&self) -> AnyElement {
        let (Some(spin), Some(ping), Some(pulse), Some(bounce)) =
            (&self.spin, &self.ping, &self.pulse, &self.bounce)
        else {
            // The values are made in `setup`, before the first render.
            return view().into_element();
        };
        let sample = |shape: AnyElement, caption: &str| {
            view()
                .gap_2()
                .items_center()
                .child(shape)
                .child(label(caption.to_owned()))
        };
        view()
            .flex_row()
            .items_center()
            .justify_between()
            .px_2()
            .py_3()
            .child(sample(
                view()
                    .size_8()
                    .rounded_md()
                    .bg(BRAND_500)
                    .bind_rotate(spin)
                    .into_element(),
                "spin",
            ))
            .child(sample(
                view()
                    .size_8()
                    .items_center()
                    .justify_center()
                    .child(
                        view()
                            .absolute()
                            .size_4()
                            .rounded_full()
                            .bg(SKY.shade(400))
                            .bind_scale(&ping.interpolate(&[0.0, 1.0], &[1.0, 2.0]))
                            .bind_opacity(&ping.interpolate(&[0.0, 1.0], &[1.0, 0.0])),
                    )
                    .child(view().size_4().rounded_full().bg(SKY.shade(500)))
                    .into_element(),
                "ping",
            ))
            .child(sample(
                view()
                    .h_8()
                    .w_14()
                    .rounded_md()
                    .bg(ZINC.shade(300))
                    .bind_opacity(pulse)
                    .into_element(),
                "pulse",
            ))
            .child(sample(
                view()
                    .size_8()
                    .rounded_full()
                    .bg(ORANGE.shade(400))
                    .bind_translate_y(bounce)
                    .into_element(),
                "bounce",
            ))
            .child(sample(
                view()
                    .size_8()
                    .rotate(12.0)
                    .scale(0.9)
                    .rounded_md()
                    .bg(EMERALD.shade(500))
                    .into_element(),
                "transform",
            ))
            .into_element()
    }
}

/// Flex rows, a grid and a breakpoint. `width` is the window's, `content`
/// the card's content width.
fn layout(width: f32, content: f32) -> AnyElement {
    // `flex-1` and `flex-[2]` share what `w-10` and the gaps leave, 1 : 2.
    let free = (content - 40.0 - 16.0).max(0.0);
    // `grid-cols-3 gap-2`: three equal tracks.
    let track = ((content - 16.0) / 3.0).max(0.0);
    let cell = |shade: u16| view().h_12().rounded_md().bg(VIOLET.shade(shade));
    view()
        .gap_3()
        .child(
            view()
                .flex_row()
                .gap_2()
                .child(
                    view()
                        .h_10()
                        .w(px(free / 3.0))
                        .rounded_md()
                        .bg(SKY.shade(200)),
                )
                .child(view().h_10().flex_1().rounded_md().bg(SKY.shade(400)))
                .child(view().h_10().w_10().rounded_md().bg(SKY.shade(600))),
        )
        .child(
            // Rows of the grid; the span-2 cell is the middle column of
            // the last two.
            view()
                .gap_2()
                .child(
                    view()
                        .flex_row()
                        .gap_2()
                        .child(
                            cell(200)
                                .flex_1()
                                .items_center()
                                .justify_center()
                                .child(text("col-span-2").text_xs().text_color(VIOLET.shade(900))),
                        )
                        .child(cell(300).w(px(track))),
                )
                .child(
                    view()
                        .flex_row()
                        .gap_2()
                        .child(view().flex_1().gap_2().child(cell(400)).child(cell(700)))
                        .child(view().flex_1().rounded_md().bg(VIOLET.shade(500)))
                        .child(view().flex_1().gap_2().child(cell(600)).child(cell(800))),
                ),
        )
        .child(
            view()
                .flex_col()
                .gap_2()
                .when(width >= 640.0, |v| v.flex_row())
                .child(view().h_8().flex_1().rounded_md().bg(AMBER.shade(300)))
                .child(view().h_8().flex_1().rounded_md().bg(AMBER.shade(500))),
        )
        .child(label(
            "The last row is a column on phones and a row from 640pt (sm:) up.",
        ))
        .into_element()
}

/// Five shades of eight colors.
fn colors() -> AnyElement {
    let hues = [
        ("red", RED),
        ("orange", ORANGE),
        ("amber", AMBER),
        ("emerald", EMERALD),
        ("sky", SKY),
        ("indigo", INDIGO),
        ("pink", PINK),
        ("zinc", ZINC),
    ];
    view()
        .gap_1()
        .children(hues.map(|(name, hue)| {
            view()
                .flex_row()
                .gap_1()
                .child(text(name).w_16().text_xs().text_color(ZINC.shade(500)))
                .children(
                    [100, 300, 500, 700, 900]
                        .map(|shade| view().h_6().flex_1().rounded_sm().bg(hue.shade(shade))),
                )
        }))
        .into_element()
}

/// The shadow presets, and what stands in for inset shadows, rings and
/// outlines.
fn shadows() -> AnyElement {
    type Shadow = fn(View) -> View;
    let sizes: [(&str, Shadow); 6] = [
        ("xs", |v| v.shadow_xs()),
        ("sm", |v| v.shadow_sm()),
        ("md", |v| v.shadow_md()),
        ("lg", |v| v.shadow_lg()),
        ("xl", |v| v.shadow_xl()),
        ("2xl", |v| v.shadow_2xl()),
    ];
    let tile = || view().size_14().rounded_xl();
    view()
        .gap_4()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .justify_between()
                .gap_y_5()
                .px_1()
                .py_2()
                .children(sizes.map(|(name, shadow)| {
                    captioned(shadow(tile().bg(adaptive(WHITE, ZINC.shade(600)))), name)
                })),
        )
        .child(
            view()
                .flex_row()
                .justify_between()
                .px_1()
                .child(captioned(tile().bg(ZINC.shade(100)), "inset ✗"))
                .child(captioned(
                    tile().bg(WHITE).border_2().border_color(SKY.shade(500)),
                    "ring (border)",
                ))
                .child(captioned(
                    tile()
                        .rounded_full()
                        .bg(WHITE)
                        .border_2()
                        .border_color(PINK.shade(500)),
                    "outline (border)",
                ))
                .child(captioned(
                    tile().bg(EMERALD.shade(500)).shadow_lg(),
                    "colored ✗",
                )),
        )
        .into_element()
}

/// Linear, angled, radial and conic gradients.
fn gradients() -> AnyElement {
    const SIZE: f32 = 64.0;
    let diagonal = std::f32::consts::FRAC_1_SQRT_2;
    let half = SIZE / 2.0;
    let samples = [
        // `bg-linear-to-r`.
        linear_gradient((1.0, 0.0), &[SKY.shade(400), BLUE.shade(600)], SIZE, SIZE),
        // `bg-linear-45`: towards the top right.
        linear_gradient(
            (diagonal, -diagonal),
            &[ROSE.shade(400), FUCHSIA.shade(500), INDIGO.shade(500)],
            SIZE,
            SIZE,
        ),
        // `bg-radial`: a circle to the farthest corner.
        shaped_gradient(
            GradientShape::Radial([half, half, 0.0, half, half, half.hypot(half)]),
            &[YELLOW.shade(200), ORANGE.shade(500)],
            SIZE,
            SIZE,
        ),
        // `bg-conic`: from the top (a canvas starts at 3 o'clock).
        shaped_gradient(
            GradientShape::Conic([-std::f32::consts::FRAC_PI_2, half, half]),
            &[RED.shade(500), GREEN.shade(500), RED.shade(500)],
            SIZE,
            SIZE,
        ),
    ];
    view()
        .flex_row()
        .justify_between()
        .children(samples.map(|c| c.size_16().rounded_xl().overflow_hidden()))
        .into_element()
}

/// Sizes, weights, families, decorations, truncation.
fn typography() -> AnyElement {
    let body = adaptive(ZINC.shade(700), ZINC.shade(200));
    view()
        .gap_2()
        .child(
            text("Display")
                .text_3xl()
                .font_weight(FontWeight::BLACK)
                // `tracking-tight`: -0.025em.
                .letter_spacing(-0.75)
                .text_color(adaptive(ZINC.shade(900), WHITE)),
        )
        .child(
            text("Light, large and relaxed")
                .text_lg()
                .font_weight(FontWeight::LIGHT)
                .text_color(body),
        )
        .child(
            text("Serif italic")
                .font_family("serif")
                .text_lg()
                .italic()
                .text_color(body),
        )
        .child(
            text("font_family(\"monospace\") 0123456789")
                .font_family("monospace")
                .text_sm()
                .text_color(body),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap_x_4()
                .gap_y_1()
                .text_color(adaptive(ZINC.shade(800), ZINC.shade(100)))
                .child(text("wavy ✗").underline())
                .child(text("double ✗").underline())
                .child(text("dashed ✗").underline())
                .child(text("struck").line_through())
                .child(
                    text("CAPS")
                        // `tracking-widest`: 0.1em.
                        .letter_spacing(1.2)
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(BRAND_600),
                ),
        )
        .child(
            text("number_of_lines(1): a single line that is far too long to fit in the card cut with an ellipsis")
                .number_of_lines(1)
                .text_sm()
                .text_color(ZINC.shade(500)),
        )
        .child(
            text(
                "number_of_lines(2): Text is laid out once on the engine thread and drawn on the main thread; typed styles reach it through the same style path as everything else, so the third line of this paragraph never shows.",
            )
            .number_of_lines(2)
            .text_sm()
            // `leading-relaxed`: 1.625 × 14pt.
            .line_height(px(22.75))
            .text_color(ZINC.shade(500)),
        )
        .child(
            text("text-shadow-md ✗")
                .text_xl()
                .font_weight(FontWeight::BOLD)
                .text_color(WHITE)
                .bg(SKY.shade(500))
                .rounded_md()
                .px_2()
                .self_start(),
        )
        .into_element()
}

/// Filters on a photo, and what stands in for backdrop blur and blending.
fn effects() -> AnyElement {
    let photo = || image(PHOTO).size_14().rounded_lg();
    view()
        .gap_3()
        .child(
            view()
                .flex_row()
                .justify_between()
                .child(captioned(photo(), "none"))
                .child(captioned(photo(), "grayscale ✗"))
                .child(captioned(photo(), "sepia ✗"))
                .child(captioned(photo().blur_radius(8.0), "blur-sm"))
                .child(captioned(
                    view()
                        .size_14()
                        .rounded_lg()
                        .overflow_hidden()
                        .child(image(PHOTO).absolute().inset_0().size_full())
                        .child(view().absolute().inset_0().bg(rgba(0x0000_0080))),
                    "brightness-50",
                )),
        )
        .child(
            view()
                .h_28()
                .overflow_hidden()
                .rounded_xl()
                .child(image(PHOTO).absolute().inset_0().size_full())
                .child(
                    view()
                        .absolute()
                        .left_3()
                        .right_3()
                        .bottom_3()
                        .rounded_lg()
                        .bg(rgba(0xffff_ff66))
                        .px_3()
                        .py_2()
                        .child(
                            text("bg white/40 (backdrop-blur-md ✗)")
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(ZINC.shade(900)),
                        ),
                )
                .child(
                    view()
                        .absolute()
                        .right_3()
                        .top_3()
                        .size_10()
                        .rounded_full()
                        .bg(YELLOW.shade(300)),
                ),
        )
        .into_element()
}

/// Rows styled by their place.
fn lists() -> AnyElement {
    let items = ["Inbox", "Drafts", "Sent", "Archive", "Trash"];
    let count = items.len();
    let border = adaptive(ZINC.shade(200), ZINC.shade(700));
    view()
        .overflow_hidden()
        .rounded_xl()
        .border_1()
        .border_color(border)
        .children(items.iter().enumerate().map(|(i, item)| {
            let (first, last, even) = (i == 0, i + 1 == count, i % 2 == 1);
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .when(!first, |v| v.border_t_1().border_color(border))
                .px_3()
                .py_2p5()
                .when(even, |v| v.bg(adaptive(ZINC.shade(50), ZINC.shade(900))))
                .when(last, |v| v.bg(adaptive(ROSE.shade(50), ROSE.shade(950))))
                .active(|s| s.bg(BRAND_50))
                .child(text(*item).text_color(adaptive(ZINC.shade(800), ZINC.shade(100))))
                .child(text("›").text_xs().text_color(ZINC.shade(400)))
        }))
        .into_element()
}

/// An input with focus, placeholder and caret colors.
fn inputs(cx: &mut RenderCx<'_, TypedStyles>) -> AnyElement {
    view()
        .gap_2()
        .child(
            input()
                .placeholder("Focus me")
                .placeholder_color(cx.color(ZINC.shade(400)))
                .selection_color(BRAND_600)
                .rounded_lg()
                .border_1()
                .border_color(adaptive(ZINC.shade(300), ZINC.shade(600)))
                .px_3()
                .py_2()
                .text_base()
                .text_color(adaptive(ZINC.shade(900), WHITE))
                .focus(|s| s.border_color(BRAND_500)),
        )
        .child(label("focus: border, placeholder color, caret color"))
        .into_element()
}
