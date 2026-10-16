//! Mobile UI: `nsh/ui/mobile`'s Rust components laying out the demo's
//! content (as the TSX gallery's `demos/MobileUi.tsx`): text fields, a
//! sheet, a select list, groups, steppers and buttons above a long list of
//! recycled rows.
//!
//! TS hands `ListView` its rows (`each`, a row renderer, a
//! `ListHeaderComponent`); a Rust component cannot fill another one's
//! `Items` prop, so the list here is a [`virtual_list`] of the demo's own
//! rows, styled as `ListView` styles its rows, with the header as its first
//! row. That row is recycled when it scrolls far away, so the header keeps
//! its state in a global, as TS keeps `inset` at module level.

use std::rc::Rc;

use nsh_core::ui::{AnyElement, prelude::*};
use nsh_ui_mobile::{
    Button, ButtonEvent, ButtonProps, ButtonVariant, Group, GroupProps, SelectList,
    SelectListEvent, SelectListProps, Sheet, SheetEvent, SheetProps, Stepper, StepperEvent,
    StepperProps, TextField, TextFieldEvent, TextFieldProps,
};

/// The muted gray of the header's notes.
fn note_color() -> Color {
    rgb(0x57606a)
}

/// Plain or inset rows (TS: a signal at module level).
#[derive(Debug, Default)]
struct Inset(bool);

/// The header's state (in a global: its row may be recycled).
#[derive(Debug)]
struct HeaderState {
    taps: u32,
    /// The controlled stepper's value.
    quantity: f64,
    /// What asking the other stepper answered.
    asked: String,
    /// The other stepper's value, as its changes report it.
    own: f64,
    /// A new key makes a new stepper (back at its default value).
    own_key: u32,
    name: String,
    sheet: bool,
    city: Option<u32>,
}

impl Default for HeaderState {
    fn default() -> Self {
        Self {
            taps: 0,
            quantity: 2.0,
            asked: String::new(),
            own: 5.0,
            own_key: 0,
            name: "Ada".to_owned(),
            sheet: false,
            city: None,
        }
    }
}

/// A row of the list.
#[derive(Debug, Clone)]
struct Stock {
    id: usize,
    symbol: String,
    price: String,
    change: f64,
}

fn stocks() -> Vec<Stock> {
    (0..300)
        .map(|i| Stock {
            id: i,
            symbol: format!("SYM{i:03}"),
            price: format!(
                "{:.2}",
                50.0 + ((i * 37) % 400) as f64 + (i % 7) as f64 * 0.13
            ),
            change: (((i * 13) % 21) as f64 - 10.0) / 4.0,
        })
        .collect()
}

/// A text of the header.
fn note(content: impl Into<SharedString>) -> AnyElement {
    text(content.into())
        .text_size(px(13.0))
        .text_color(note_color())
        .into_element()
}

/// A mobile UI button.
fn mobile_button(
    title: impl Into<SharedString>,
    variant: ButtonVariant,
) -> ComponentElement<Button> {
    Button::element(ButtonProps {
        title: title.into(),
        variant,
        ..ButtonProps::default()
    })
}

/// The list's header: the components.
#[derive(Debug, Default)]
struct Header {
    cities: Rc<Vec<SharedString>>,
}

impl Component for Header {
    const NAME: &'static str = "gallery.mobile_ui.header";
    type Props = ();
    type Event = NoEvent;

    fn setup(&mut self, _: &(), _: &mut Context<'_, Self>) {
        self.cities = Rc::new((1..=500).map(|i| format!("City {i}").into()).collect());
    }

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        // `Inset` first: reading a global for the first time makes it, which
        // `state`'s borrow would not allow.
        let inset = cx.global::<Inset>().0;
        let state = cx.global::<HeaderState>();
        let update = |f: fn(&mut HeaderState)| {
            cx.listener(move |_: &mut Self, _: &ButtonEvent, cx| {
                cx.update_global::<HeaderState, _>(|s, _| f(s));
            })
        };
        let chosen = match state.city {
            Some(i) => self.cities[i as usize].to_string(),
            None => "none".to_owned(),
        };
        let between = || view().flex_row().items_center().justify_between();
        view()
            .p_4()
            .gap_4()
            .child(
                TextField::element(TextFieldProps {
                    label: Some("Name (controlled; × clears and focuses)".into()),
                    placeholder: Some("Your name".into()),
                    value: Some(state.name.clone().into()),
                    ..TextFieldProps::default()
                })
                .on_event(cx.listener(|_: &mut Self, e: &TextFieldEvent, cx| {
                    if let TextFieldEvent::Change(v) = e {
                        let v = v.clone();
                        cx.update_global::<HeaderState, _>(|s, _| s.name = v);
                    }
                })),
            )
            .child(TextField::element(TextFieldProps {
                label: Some("Password".into()),
                placeholder: Some("At least 8 characters".into()),
                secure: true,
                ..TextFieldProps::default()
            }))
            .child(note(format!(
                "Hello, {}",
                if state.name.is_empty() {
                    "nobody"
                } else {
                    &state.name
                }
            )))
            .child(
                mobile_button("Open the sheet (drag it down)", ButtonVariant::Primary)
                    .on_event(update(|s| s.sheet = true)),
            )
            .child(
                Sheet::element(SheetProps {
                    open: state.sheet,
                    ..SheetProps::default()
                })
                .on_event(cx.listener(|_: &mut Self, _: &SheetEvent, cx| {
                    cx.update_global::<HeaderState, _>(|s, _| s.sheet = false);
                }))
                .child(
                    text("A sheet in Rust")
                        .text_size(px(17.0))
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(
                    text("It follows the finger natively and springs back; drag it down, tap outside or press back to close.")
                        .text_color(note_color()),
                )
                .child(
                    mobile_button("Done", ButtonVariant::Primary)
                        .on_event(update(|s| s.sheet = false)),
                ),
            )
            .child(note(format!(
                "SelectList: 500 rows of Rust data, chosen: {chosen}"
            )))
            .child(
                view().h(px(240.0)).rounded(px(12.0)).overflow_hidden().child(
                    SelectList::element(SelectListProps {
                        options: self.cities.to_vec(),
                        value: state.city,
                    })
                    .on_event(cx.listener(|_: &mut Self, e: &SelectListEvent, cx| {
                        let SelectListEvent::Select(i) = *e;
                        cx.update_global::<HeaderState, _>(|s, _| s.city = Some(i));
                    })),
                ),
            )
            .child(
                Group::element(GroupProps::default())
                    .fill("header", text("Group: each child is a row"))
                    .fill(
                        "footer",
                        text("The rows are the demo's elements; the group adds the box and the dividers."),
                    )
                    .child(text("Notifications"))
                    .child(text("Privacy"))
                    .child(
                        view()
                            .flex_row()
                            .justify_between()
                            .child(text("Taps"))
                            .child(text(state.taps.to_string())),
                    ),
            )
            .child(
                Group::element(GroupProps::default())
                    .fill(
                        "header",
                        text("Stepper: state and timers in Rust (hold a button)"),
                    )
                    .child(
                        between().child(text("Own value")).child(
                            // Starts where it was when its row comes back.
                            Stepper::element(StepperProps {
                                default_value: state.own,
                                min: 0.0,
                                max: 50.0,
                                ..StepperProps::default()
                            })
                            .key(format!("own-{}", state.own_key))
                            .on_event(cx.listener(|_: &mut Self, e: &StepperEvent, cx| {
                                let StepperEvent::Change(v) = *e;
                                cx.update_global::<HeaderState, _>(|s, _| s.own = v);
                            })),
                        ),
                    )
                    .child(
                        between()
                            .child(text(format!("Controlled: {}", state.quantity)))
                            .child(
                                Stepper::element(StepperProps {
                                    value: Some(state.quantity),
                                    min: 0.0,
                                    max: 10.0,
                                    step: 0.5,
                                    ..StepperProps::default()
                                })
                                .on_event(cx.listener(
                                    |_: &mut Self, e: &StepperEvent, cx| {
                                        let StepperEvent::Change(v) = *e;
                                        cx.update_global::<HeaderState, _>(|s, _| s.quantity = v);
                                    },
                                )),
                            ),
                    ),
            )
            .child(
                // TS calls the stepper's methods (`value()`, `setValue(5)`);
                // a Rust component cannot call another one's methods, so this
                // reads the value its changes reported and makes a new
                // stepper.
                mobile_button(
                    if state.asked.is_empty() {
                        "Ask the stepper, then reset it".to_owned()
                    } else {
                        state.asked.clone()
                    },
                    ButtonVariant::Secondary,
                )
                .on_event(update(|s| {
                    s.asked = format!("It was {}; reset to 5", s.own);
                    s.own = 5.0;
                    s.own_key += 1;
                })),
            )
            .child(
                mobile_button("Tap", ButtonVariant::Secondary).on_event(update(|s| s.taps += 1)),
            )
            .child(
                mobile_button(
                    if inset { "Plain rows" } else { "Inset rows" },
                    ButtonVariant::Plain,
                )
                .on_event(cx.listener(|_: &mut Self, _: &ButtonEvent, cx| {
                    cx.update_global::<Inset, _>(|inset, _| inset.0 = !inset.0);
                })),
            )
            .child(note(
                "The list below: 300 rows the demo renders, recycled natively.",
            ))
    }
}

/// The demo's page: the header and the stocks in one recycled list.
#[derive(Debug, Default)]
pub(crate) struct MobileUi {
    stocks: Rc<Vec<Stock>>,
}

impl Component for MobileUi {
    const NAME: &'static str = "gallery.mobile_ui";
    type Props = ();
    type Event = NoEvent;

    fn setup(&mut self, _: &(), _: &mut Context<'_, Self>) {
        self.stocks = Rc::new(stocks());
    }

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        let theme = cx.theme();
        let inset = cx.global::<Inset>().0;
        // The last rows scroll clear of the home indicator.
        let below = cx.safe_area().bottom;
        // As `ListView` draws its rows; they outlive the render, so they keep
        // copies of the colors.
        let (border, surface, radius) = (theme.border, theme.background, px(theme.radius_lg));
        let stocks = self.stocks.clone();
        let count = stocks.len() + 1;
        let keys: Vec<usize> = std::iter::once(usize::MAX)
            .chain(stocks.iter().map(|s| s.id))
            .collect();
        view()
            .flex_1()
            .bg(if inset { theme.muted } else { theme.background })
            .text_color(theme.foreground)
            .child(
                virtual_list(count, move |i, at| {
                    if i == 0 {
                        return Header::element(()).into_element();
                    }
                    let stock = &stocks[i - 1];
                    let (first, last) = (i == 1, at.is_last());
                    let up = stock.change >= 0.0;
                    view()
                        .px_4()
                        .py_3()
                        .min_h(px(48.0))
                        .justify_center()
                        .when(!first, |v| v.border_t_1().border_color(border))
                        .when(inset, |v| {
                            v.bg(surface)
                                .when(first, |v| v.rounded_t(radius))
                                .when(last, |v| v.rounded_b(radius))
                        })
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .child(
                                    text(stock.symbol.clone())
                                        .flex_1()
                                        .font_weight(FontWeight::SEMIBOLD),
                                )
                                .child(text(stock.price.clone()).w(px(80.0)).text_right())
                                .child(
                                    text(format!(
                                        "{}{:.2}%",
                                        if up { "+" } else { "" },
                                        stock.change
                                    ))
                                    .w(px(64.0))
                                    .text_right()
                                    .text_color(if up {
                                        rgb(0x1a7f37)
                                    } else {
                                        rgb(0xcf222e)
                                    }),
                                ),
                        )
                        .into_element()
                })
                .keys(keys)
                .estimated_item_size(48.0)
                .when(inset, |l| l.content_padding(16.0))
                .content_padding_bottom(below + if inset { 16.0 } else { 0.0 })
                .flex_1(),
            )
    }
}
