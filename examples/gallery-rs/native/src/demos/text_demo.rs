//! Text: fonts, spans, inline views and images, decorations, truncation,
//! shadows (as the TSX gallery's `demos/TextDemo.tsx`).
//!
//! A Rust text has no children: TS's nested spans, and the views and
//! images inside a text, are separate texts and views in a wrapping row
//! here (they wrap piece by piece, not word by word).

use nsh_core::ui::{AnyElement, EllipsizeMode, Text, View, prelude::*};

const LOREM: &str = "Native text, typeset once on the engine thread with CoreText and drawn on the main thread without any layout work.";

/// The gallery's logo, as the TSX gallery imports it (`../logo@2x.png`).
const LOGO: &str = "asset:/logo@2x.png";

fn caption(content: impl Into<SharedString>) -> AnyElement {
    text(content)
        .text_size(px(12.0))
        .text_color(rgb(0x6e7781))
        .mt(px(10.0))
        .into_element()
}

fn link(content: &'static str) -> Text {
    text(content).text_color(rgb(0x0969da)).underline()
}

/// Pieces of one paragraph, wrapping like its words would.
fn paragraph(pieces: impl IntoIterator<Item = AnyElement>) -> View {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .children(pieces)
}

/// `words` with each word capitalized (TS: `textTransform: "capitalize"`).
fn capitalize(words: &str) -> String {
    words
        .split(' ')
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// The demo's page.
#[derive(Debug, Default)]
pub(crate) struct TextDemo {
    /// What was pressed in the sentence.
    tapped: String,
    /// The count of the button inside a sentence.
    likes: u32,
}

impl TextDemo {
    fn sentence(&self, cx: &RenderCx<'_, Self>) -> AnyElement {
        let tap = |what: &'static str| {
            cx.listener(move |this: &mut Self, _: &(), cx| {
                this.tapped = what.to_owned();
                cx.notify();
            })
        };
        let bold = |content: Text| content.font_weight(FontWeight::BOLD).into_element();
        paragraph([
            text("By continuing you agree to the ").into_element(),
            link("Terms of service")
                .on_press(tap("Terms of service"))
                .on_long_press(tap("Terms (long press)"))
                .into_element(),
            text(" and the ").into_element(),
            link("Privacy policy")
                .on_press(tap("Privacy policy"))
                .into_element(),
            text(", which mention ").into_element(),
            bold(text("nested ")),
            bold(link("links").on_press(tap("a link inside bold text"))),
            bold(text(" too")),
            text(".").into_element(),
        ])
        .text_size(px(16.0))
        .line_height(px(24.0))
        .on_press(tap("the sentence (not a link)"))
        .into_element()
    }

    fn inline(&self, cx: &RenderCx<'_, Self>) -> AnyElement {
        paragraph([
            text("Made with ").into_element(),
            image(LOGO).size(px(18.0)).rounded(px(4.0)).into_element(),
            text(" nsh. Status ").into_element(),
            view()
                .bg(rgb(0x1a7f37))
                .rounded(px(9.0))
                .px(px(7.0))
                .h(px(18.0))
                .justify_center()
                .child(
                    text("LIVE")
                        .text_color(rgb(0xffffff))
                        .text_size(px(11.0))
                        .font_weight(FontWeight::BOLD),
                )
                .into_element(),
            text(" and a button inside the sentence ").into_element(),
            view()
                .bg(rgb(0xddf4ff))
                .rounded(px(6.0))
                .px_2()
                .h(px(22.0))
                .justify_center()
                .on_press(cx.listener(|this: &mut Self, _, cx| {
                    this.likes += 1;
                    cx.notify();
                }))
                .child(
                    text(format!("♥ {}", self.likes))
                        .text_color(rgb(0x0969da))
                        .text_size(px(13.0)),
                )
                .into_element(),
            text(" that takes presses of its own.").into_element(),
        ])
        .text_size(px(16.0))
        .line_height(px(26.0))
        .into_element()
    }
}

impl Component for TextDemo {
    const NAME: &'static str = "gallery.text_demo";
    type Props = ();
    type Event = NoEvent;

    fn setup(&mut self, _: &(), _: &mut Context<'_, Self>) {
        self.tapped = "Tap a link in the sentence".to_owned();
        self.likes = 3;
    }

    fn render(&self, _: &(), cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        let size = |content: Text, points: f32| content.text_size(px(points)).into_element();
        let lorem = || text(LOREM).text_size(px(15.0));
        view().flex_1().child(
            scroll().flex_1().child(
                view()
                    .p_5()
                    .gap_1()
                    .child(self.sentence(cx))
                    .child(caption(format!("Pressed: {}", self.tapped)))
                    .child(caption("Views and images inside text (they wrap with it)"))
                    .child(self.inline(cx))
                    // TS imports the font file, which registers its family;
                    // Rust cannot bundle nor register a font, so this shows
                    // only where the app registered Pacifico.
                    .child(
                        text("Custom fonts")
                            .font_family("Pacifico")
                            .text_size(px(30.0))
                            .text_color(rgb(0x8250df)),
                    )
                    .child(caption("Styles"))
                    .child(
                        paragraph([
                            text("Light ").font_weight(FontWeight::LIGHT).into_element(),
                            text("Bold ").font_weight(FontWeight::BOLD).into_element(),
                            text("Italic ").italic().into_element(),
                            text("Both ")
                                .font_weight(FontWeight::BOLD)
                                .italic()
                                .into_element(),
                            text("Spaced").letter_spacing(4.0).into_element(),
                        ])
                        .text_size(px(17.0)),
                    )
                    // TS colors the strikethrough (`textDecorationColor`) and
                    // draws both lines on "Both": the typed styles have
                    // one decoration line, and no decoration color.
                    .child(
                        paragraph([
                            text("Underline").underline().into_element(),
                            text(" ").into_element(),
                            text("Strikethrough").line_through().into_element(),
                            text(" ").into_element(),
                            text("Both")
                                .underline()
                                .text_color(rgb(0x0969da))
                                .into_element(),
                        ])
                        .text_size(px(17.0)),
                    )
                    // TS transforms the case in the style (`textTransform`);
                    // the typed styles have none, so the strings are.
                    .child(
                        paragraph([
                            text("uppercase ".to_uppercase()).into_element(),
                            text(capitalize("capitalized words here")).into_element(),
                        ])
                        .text_size(px(17.0)),
                    )
                    // TS gives it a shadow (`textShadow*`): the typed styles
                    // have no text shadow.
                    .child(
                        text("Text shadow")
                            .text_size(px(26.0))
                            .font_weight(FontWeight::EXTRA_BOLD)
                            .text_color(rgb(0xffffff))
                            .bg(rgb(0x54aeff))
                            .px_2(),
                    )
                    .child(caption(format!(
                        "System text size (fontScale {:.2})",
                        cx.font_scale()
                    )))
                    .child(size(text("Scales with the system setting"), 17.0))
                    // TS sets `allowFontScaling={false}` and
                    // `maxFontSizeMultiplier={1.2}` on the next two: a Rust
                    // text has neither, both scale fully.
                    .child(size(text("Fixed size (allowFontScaling false)"), 17.0))
                    .child(size(text("At most 1.2× (maxFontSizeMultiplier)"), 17.0))
                    .child(caption("numberOfLines=2 (tail)"))
                    .child(lorem().number_of_lines(2))
                    .child(caption("numberOfLines=1 head / middle / clip"))
                    .children(
                        [
                            EllipsizeMode::Head,
                            EllipsizeMode::Middle,
                            EllipsizeMode::Clip,
                        ]
                        .map(|mode| lorem().number_of_lines(1).ellipsize_mode(mode)),
                    )
                    .child(caption("Centered, 3 lines"))
                    .child(
                        text(format!("{LOREM} {LOREM}"))
                            .number_of_lines(3)
                            .text_size(px(15.0))
                            .text_center()
                            .line_height(px(22.0)),
                    ),
            ),
        )
    }
}
