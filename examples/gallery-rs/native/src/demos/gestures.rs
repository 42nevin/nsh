//! Gestures: a card dragged natively, a ball dragged through touch events,
//! raw touches, press events, hit slop, pointer events and accessibility
//! labels (as the TSX gallery's `demos/Gestures.tsx`).

use nsh_core::{
    Ripple, TouchPhase, TouchPoint,
    ui::{Text, TouchEvent, prelude::*},
};

use crate::ui::{button, colors, page, section};

/// A muted note under a demo.
fn note(content: impl Into<SharedString>) -> Text {
    text(content)
        .text_size(px(13.0))
        .text_color(colors::muted())
}

/// The fingers still down during `touch` (an end or cancel lists the lifted
/// ones too), and the one it is about.
fn fingers(touch: &TouchEvent) -> (Vec<&TouchPoint>, Option<&TouchPoint>) {
    let lifted = matches!(touch.phase, TouchPhase::End | TouchPhase::Cancel);
    let down = touch
        .points
        .iter()
        .filter(|p| !lifted || !p.changed)
        .collect();
    (down, touch.points.iter().find(|p| p.changed))
}

/// A card dragged with one finger, natively: the drag moves its values on
/// the UI thread.
///
/// TS also pinches and rotates it (`pinch={{ scale }}`, `rotation={{ angle
/// }}`); the Rust API has no way to give a view those values.
#[derive(Debug, Default)]
struct Card {
    x: Option<NativeValue>,
    y: Option<NativeValue>,
}

impl Card {
    fn values(&self) -> (&NativeValue, &NativeValue) {
        match (&self.x, &self.y) {
            (Some(x), Some(y)) => (x, y),
            _ => unreachable!("set up"),
        }
    }
}

impl Component for Card {
    const NAME: &'static str = "gallery.gestures.card";
    type Props = ();
    type Event = NoEvent;

    fn setup(&mut self, _: &(), cx: &mut Context<'_, Self>) {
        self.x = Some(cx.native_value(0.0));
        self.y = Some(cx.native_value(0.0));
    }

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        let (x, y) = self.values();
        view()
            .gap_3()
            .items_center()
            .child(
                view()
                    .h(px(220.0))
                    .self_stretch()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .child(
                        view()
                            .w(px(200.0))
                            .h(px(150.0))
                            .rounded(px(14.0))
                            .overflow_hidden()
                            .pan_values(Some(x), Some(y))
                            .bind_translate_x(x)
                            .bind_translate_y(y)
                            .child(image("https://picsum.photos/seed/nsh-card/400/300").flex_1()),
                    ),
            )
            .child(note(
                "Drag with one finger (pinch and rotation are not in the Rust API yet).",
            ))
            .child(button("Reset", colors::gray()).on_press(cx.listener(
                |this: &mut Self, _, _| {
                    let (x, y) = this.values();
                    x.spring(0.0, Spring::default());
                    y.spring(0.0, Spring::default());
                },
            )))
    }
}

/// A drag followed through touch events (TS: `PanResponder`'s gesture
/// state): the translation of the fingers' centroid since the drag began
/// and its velocity, smoothed over a few samples.
#[derive(Debug, Default)]
struct Drag {
    dx: f32,
    dy: f32,
    /// Points per millisecond.
    vx: f32,
    vy: f32,
    /// The centroid, time and number of fingers of the last update.
    last: (f32, f32, f64, usize),
}

impl Drag {
    fn centroid(down: &[&TouchPoint]) -> (f32, f32) {
        let n = down.len().max(1) as f32;
        let x = down.iter().map(|p| p.page_x).sum::<f32>() / n;
        let y = down.iter().map(|p| p.page_y).sum::<f32>() / n;
        (x, y)
    }

    /// Starts a drag at the fingers `down`.
    fn begin(down: &[&TouchPoint], timestamp: f64) -> Self {
        let (x, y) = Self::centroid(down);
        Self {
            last: (x, y, timestamp, down.len()),
            ..Self::default()
        }
    }

    /// Follows the fingers to `down` (a finger going down or up makes the
    /// centroid jump, which is no movement).
    fn update(&mut self, down: &[&TouchPoint], timestamp: f64) {
        let (lx, ly, lt, ln) = self.last;
        let (x, y) = Self::centroid(down);
        if timestamp == lt && down.len() == ln {
            return;
        }
        if down.len() == ln {
            let dt = (timestamp - lt).max(1.0) as f32;
            self.dx += x - lx;
            self.dy += y - ly;
            self.vx = self.vx * 0.3 + (x - lx) / dt * 0.7;
            self.vy = self.vy * 0.3 + (y - ly) / dt * 0.7;
        }
        self.last = (x, y, timestamp, down.len());
    }
}

/// A ball dragged through touch events (positions go through Rust, a
/// render per move).
///
/// TS's `PanResponder` becomes the responder and blocks the page from
/// scrolling while it drags; the Rust API cannot claim the touches, so
/// a vertical drag may be taken over by the page.
#[derive(Debug)]
struct Ball {
    /// Where the ball is, from where it started.
    pos: (f32, f32),
    /// Where it was when the drag began.
    start: (f32, f32),
    /// The drag going on.
    drag: Option<Drag>,
    /// What happened last.
    state: String,
}

impl Default for Ball {
    fn default() -> Self {
        Self {
            pos: (0.0, 0.0),
            start: (0.0, 0.0),
            drag: None,
            state: "idle".into(),
        }
    }
}

impl Ball {
    fn touch(&mut self, touch: &TouchEvent) {
        let (down, _) = fingers(touch);
        match touch.phase {
            TouchPhase::Start if self.drag.is_none() => {
                self.start = self.pos;
                self.drag = Some(Drag::begin(&down, touch.timestamp));
                self.state = "dragging".into();
            }
            TouchPhase::Start | TouchPhase::Move | TouchPhase::End => {
                let Some(drag) = &mut self.drag else { return };
                drag.update(&down, touch.timestamp);
                self.pos = (self.start.0 + drag.dx, self.start.1 + drag.dy);
                if down.is_empty() {
                    self.state = format!("released at {:.2}, {:.2} pt/ms", drag.vx, drag.vy);
                    self.drag = None;
                }
            }
            TouchPhase::Cancel => {
                if self.drag.take().is_some() {
                    self.state = "taken over".into();
                }
            }
        }
    }
}

impl Component for Ball {
    const NAME: &'static str = "gallery.gestures.ball";
    type Props = ();
    type Event = NoEvent;

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        view()
            .gap_2()
            .child(
                view()
                    .h(px(180.0))
                    .rounded(px(10.0))
                    .bg(rgb(0xf6f8fa))
                    .overflow_hidden()
                    .child(
                        view()
                            .absolute()
                            .left(px(130.0))
                            .top(px(60.0))
                            .size(px(60.0))
                            .rounded(px(30.0))
                            .bg(colors::accent())
                            .translate_x(self.pos.0)
                            .translate_y(self.pos.1)
                            .on_touch(cx.listener(|this: &mut Self, touch: &TouchEvent, cx| {
                                this.touch(touch);
                                cx.notify();
                            })),
                    ),
            )
            .child(note(self.state.clone()))
            .child(button("Reset", colors::gray()).on_press(cx.listener(
                |this: &mut Self, _, cx| {
                    this.pos = (0.0, 0.0);
                    cx.notify();
                },
            )))
    }
}

/// Raw touch events, with every finger.
#[derive(Debug, Default)]
struct TouchLog {
    /// The latest first.
    log: Vec<String>,
}

impl Component for TouchLog {
    const NAME: &'static str = "gallery.gestures.touch_log";
    type Props = ();
    type Event = NoEvent;

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        let log = if self.log.is_empty() {
            "No touches yet".to_owned()
        } else {
            self.log.join("\n")
        };
        view()
            .gap_2()
            .child(
                view()
                    .h(px(100.0))
                    .rounded(px(10.0))
                    .bg(rgb(0xddf4ff))
                    .items_center()
                    .justify_center()
                    .on_touch(cx.listener(|this: &mut Self, touch: &TouchEvent, cx| {
                        let (down, about) = fingers(touch);
                        let Some(p) = about else { return };
                        let name = match touch.phase {
                            TouchPhase::Start => "start",
                            TouchPhase::Move => "move",
                            TouchPhase::End => "end",
                            TouchPhase::Cancel => "cancel",
                        };
                        let entry = format!(
                            "{name} at {},{} (page {},{}), {} down",
                            p.location_x.round(),
                            p.location_y.round(),
                            p.page_x.round(),
                            p.page_y.round(),
                            down.len()
                        );
                        this.log.insert(0, entry);
                        this.log.truncate(4);
                        cx.notify();
                    }))
                    .child(text("Touch here (try two fingers)").text_color(colors::accent())),
            )
            .child(note(log))
    }
}

/// The demo's page; it keeps the log of the press events and the counts of
/// the hit slop and pointer events demos.
#[derive(Debug, Default)]
pub(crate) struct Gestures {
    /// The latest first.
    log: Vec<String>,
    /// Taps on the small dot.
    slop: u32,
    /// Taps through the overlay.
    through: u32,
}

impl Gestures {
    /// A listener adding `event` to the log.
    fn add(cx: &RenderCx<'_, Self>, event: &'static str) -> Listener<()> {
        cx.listener(move |this: &mut Self, _, cx| {
            this.log.insert(0, event.to_owned());
            this.log.truncate(6);
            cx.notify();
        })
    }
}

impl Component for Gestures {
    const NAME: &'static str = "gallery.gestures";
    type Props = ();
    type Event = NoEvent;

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        let log = if self.log.is_empty() {
            "No events yet".to_owned()
        } else {
            self.log.join(" · ")
        };
        let press = [
            view()
                .p(px(18.0))
                .rounded(px(10.0))
                .bg(rgb(0xddf4ff))
                .items_center()
                .active_opacity(0.5)
                .on_press_in(Self::add(cx, "pressIn"))
                .on_press_out(Self::add(cx, "pressOut"))
                .on_press(Self::add(cx, "press"))
                .on_long_press(Self::add(cx, "longPress"))
                .child(text("Tap, hold, or slide off").font_weight(FontWeight::SEMIBOLD))
                .into_element(),
            text(log).text_color(colors::muted()).into_element(),
            view()
                .flex_row()
                .gap(px(10.0))
                .child(
                    view()
                        .flex_1()
                        .p(px(14.0))
                        .rounded(px(10.0))
                        .bg(rgb(0xf6f8fa))
                        .items_center()
                        .on_press(Self::add(cx, "ripple"))
                        .on_long_press(Self::add(cx, "fast long press"))
                        .long_press_delay(250.0)
                        .ripple(Ripple {
                            color: Some(rgba(0x0969_da4d)),
                            ..Ripple::default()
                        })
                        .child(text("Ripple, long press 250 ms")),
                )
                .child(
                    view()
                        .size(px(56.0))
                        .items_center()
                        .justify_center()
                        .on_press(Self::add(cx, "borderless"))
                        .ripple(Ripple {
                            borderless: true,
                            radius: Some(40.0),
                            ..Ripple::default()
                        })
                        .child(text("★").text_size(px(22.0))),
                )
                .into_element(),
            text("Text with activeOpacity")
                .text_color(colors::accent())
                .active_opacity(0.3)
                .on_press(Self::add(cx, "text"))
                .into_element(),
        ];
        let hit_slop = view()
            .flex_row()
            .items_center()
            .gap_4()
            .child(
                view()
                    .size(px(12.0))
                    .rounded(px(6.0))
                    .bg(rgb(0xcf222e))
                    .m_5()
                    .hit_slop(EdgeInsets::all(30.0))
                    .on_press(cx.listener(|this: &mut Self, _, cx| {
                        this.slop += 1;
                        cx.notify();
                    })),
            )
            .child(text(format!("The dot takes taps 30 pt around it: {}", self.slop)).flex_1());
        let pointer_events = [
            view()
                .h(px(70.0))
                .child(
                    view()
                        .flex_1()
                        .rounded(px(10.0))
                        .bg(rgb(0x1a7f37))
                        .items_center()
                        .justify_center()
                        .on_press(cx.listener(|this: &mut Self, _, cx| {
                            this.through += 1;
                            cx.notify();
                        }))
                        .child(
                            text(format!("Button under an overlay: {}", self.through))
                                .text_color(rgb(0xffffff)),
                        ),
                )
                .child(
                    // TS draws the border dashed; `Styled` has no border
                    // style.
                    view()
                        .absolute()
                        .inset(px(0.0))
                        .rounded(px(10.0))
                        .border(3.0)
                        .border_color(rgb(0xbf8700))
                        .pointer_events(PointerEvents::None),
                )
                .into_element(),
            note("The bordered overlay has pointer_events(None): taps reach the button.")
                .into_element(),
        ];
        let accessibility =
            [("Play", "Starts the song"), ("Like", "Adds to favorites")].map(|(label, hint)| {
                view()
                    .accessible(true)
                    .role(AccessibilityRole::Button)
                    .label(label)
                    .hint(hint)
                    .on_press(Self::add(cx, label))
                    .p_3()
                    .rounded(px(10.0))
                    .bg(rgb(0xeaeef2))
                    .child(text(format!("{label} (labelled for VoiceOver / TalkBack)")))
                    .into_element()
            });
        view().flex_1().child(page([
            section("Pan on the UI thread", [Card::element(()).into_element()]).into_element(),
            section(
                "Touch events driving a drag (drag the ball)",
                [Ball::element(()).into_element()],
            )
            .into_element(),
            section("Touch events", [TouchLog::element(()).into_element()]).into_element(),
            section("Press events", press).into_element(),
            section("Hit slop", [hit_slop.into_element()]).into_element(),
            section("Pointer events", pointer_events).into_element(),
            section("Accessibility", accessibility).into_element(),
        ]))
    }
}
