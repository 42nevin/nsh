//! The host elements beyond views, texts and images: inputs, scroll views,
//! navigation stacks and their screens, modals, canvases, web views, videos
//! and the embedder's native components, with their props and typed events. Their props are the
//! core's [`Prop`]s, kept per element and diffed by kind: a prop a render
//! no longer sets goes back to its default, as when JS clears it.

use std::sync::Arc;

use super::{
    Accessible, AnyElement, Interactive,
    element::{ElNode, Element, IntoElement, Listener, ParentElement},
    events::{
        self, NativeEvent, ScrollEvent, ScrollOffset, Selection, VideoInfo, VideoProgress,
        WebNavigation,
    },
};
use crate::{
    AutoCapitalize, AutoFill, Color, EdgeInsets, EventKind, HeaderSlot, JsonValue,
    KeyboardDismissMode, KeyboardTaps, KeyboardType, ModalAnimation, ModalPresentation, ModalProp,
    NativeProp, Prop, ResizeMode, ReturnKeyType, ScreenAnimation, ScreenProp, ScrollProp, Size,
    SnapAlignment, Tag, WebSource,
    canvas::{
        CanvasFont, CanvasGradient, CanvasImage, CanvasOp, CanvasTextAlign, CanvasTextBaseline,
        Composite, Drawing, FillRule, GradientShape, LineCap, LineJoin, Num, Paint, Path, PathOp,
    },
    js::host::HostProps,
};

/// Whether `a` and `b` set the same property (their values aside).
pub(crate) fn same_kind(a: &Prop, b: &Prop) -> bool {
    match (a, b) {
        (Prop::ComponentProp(a, _), Prop::ComponentProp(b, _)) => a == b,
        _ => HostProps::same_kind(a, b),
    }
}

/// `prop`'s property at its default (what clearing it from JS sets), for
/// the props elements set; `None` for those set once.
pub(crate) fn reset(prop: &Prop) -> Option<Prop> {
    match prop {
        Prop::ComponentProp(name, _) => Some(Prop::ComponentProp(name.clone(), None)),
        _ => HostProps::reset(prop),
    }
}

/// Sets `prop` on `node`, replacing the value of the same property.
pub(crate) fn set(node: &mut ElNode, prop: Prop) {
    let props = &mut node.attrs.props;
    match props.iter_mut().find(|p| same_kind(p, &prop)) {
        Some(p) => *p = prop,
        None => props.push(prop),
    }
}

/// Adds a typed listener of `kind`.
fn listen<T: ?Sized + ToOwned + 'static>(
    node: &mut ElNode,
    kind: EventKind,
    listener: Listener<T>,
    decode: events::Decode<T>,
) {
    node.handlers.push((kind, events::typed(listener, decode)));
}

macro_rules! host_element {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone)]
        pub struct $name(ElNode);

        impl Element for $name {
            fn node(&mut self) -> &mut ElNode {
                &mut self.0
            }
        }

        impl IntoElement for $name {
            fn into_element(self) -> AnyElement {
                AnyElement(self.0)
            }
        }

        impl Accessible for $name {}
        impl Interactive for $name {}

        // Not every element has props of its own and events.
        #[allow(dead_code)]
        impl $name {
            fn set(mut self, prop: Prop) -> Self {
                set(&mut self.0, prop);
                self
            }

            fn listen<T: ?Sized + ToOwned + 'static>(
                mut self,
                kind: EventKind,
                listener: Listener<T>,
                decode: events::Decode<T>,
            ) -> Self {
                listen(&mut self.0, kind, listener, decode);
                self
            }
        }
    };
}

host_element! {
    /// A text field (`<input>`), made by [`input`]: one line, or several
    /// with [`Input::multiline`]. Its text style comes from its styles.
    Input
}

host_element! {
    /// A scroll view (`<scroll>`), made by [`scroll`]: its children are its
    /// content.
    Scroll
}

host_element! {
    /// A native navigation stack (`<stack>`), made by [`stack`]:
    /// UINavigationController on iOS (swipe back, native header), a stack
    /// with a toolbar and predictive back on Android. Its children are its
    /// [`Screen`]s, bottom to top.
    Stack
}

host_element! {
    /// One screen of a [`Stack`] (`<screen>`), made by [`screen`]: its
    /// header options and its content (its children).
    Screen
}

host_element! {
    /// A modal (`<modal>`), made by [`modal`]: presented while it is in the
    /// render, over everything else; its children are its content.
    Modal
}

host_element! {
    /// A view drawn with a [`Drawing`] (`<canvas>`), made by [`canvas`].
    CanvasView
}

host_element! {
    /// A web page (`<webview>`), made by [`web_view`].
    WebView
}

host_element! {
    /// A video player (`<video>`), made by [`video`].
    Video
}

host_element! {
    /// A native component the embedding app registered (`<native>`), made
    /// by [`native`].
    NativeView
}

impl ParentElement for Scroll {}
impl ParentElement for Stack {}
impl ParentElement for Screen {}
impl ParentElement for Modal {}

/// A text field.
pub fn input() -> Input {
    Input(ElNode::new(Tag::Input))
}

/// A vertical scroll view ([`Scroll::horizontal`] for a horizontal one).
pub fn scroll() -> Scroll {
    Scroll(ElNode::new(Tag::Scroll))
}

/// A native navigation stack of [`screen`]s: put a screen in to push it,
/// leave it out to pop it. Give each screen a [`Element::key`] (its route)
/// so the screens below keep their state.
pub fn stack() -> Stack {
    Stack(ElNode::new(Tag::Stack))
}

/// A screen of a [`stack`]. What it renders reads its insets below the
/// header ([`super::RenderCx::insets`]).
pub fn screen() -> Screen {
    let mut node = ElNode::new(Tag::Screen);
    // Its insets come with this event.
    node.handlers.push((EventKind::Insets, Listener::noop()));
    Screen(node)
}

/// A modal presenting its children. Put it in the render to present it,
/// leave it out to dismiss it.
pub fn modal() -> Modal {
    let mut node = ElNode::new(Tag::Modal);
    // Its insets (`RenderCx::insets` inside it) come with this event.
    node.handlers.push((EventKind::Insets, Listener::noop()));
    Modal(node)
}

/// A canvas showing what `painter` recorded (draw again, e.g. from
/// `on_layout`'s size, to change it; native values in it move natively).
pub fn canvas(painter: impl Into<Painter>) -> CanvasView {
    let painter = painter.into();
    let mut node = ElNode::new(Tag::Canvas);
    node.attrs.drawing = Some(Arc::new(painter.drawing));
    node.attrs.bound = painter.nodes;
    CanvasView(node)
}

/// A web view loading `url`.
pub fn web_view(url: &str) -> WebView {
    WebView(ElNode::new(Tag::WebView))
        .set(Prop::Native(NativeProp::Source(WebSource::Uri(url.into()))))
}

/// A video playing the file or stream at `url`.
pub fn video(url: &str) -> Video {
    Video(ElNode::new(Tag::Video)).set(Prop::Native(NativeProp::Source(WebSource::Uri(url.into()))))
}

/// The embedder's native component registered as `name`.
pub fn native(name: &str) -> NativeView {
    NativeView(ElNode::new(Tag::Native)).set(Prop::Component(name.into()))
}

impl Input {
    /// The text, when the component controls it (update it from
    /// [`Self::on_change_text`]).
    pub fn value(self, text: impl Into<String>) -> Self {
        self.set(Prop::Value(text.into()))
    }

    /// The first text, when the input keeps its own.
    pub fn default_value(self, text: impl Into<String>) -> Self {
        self.set(Prop::DefaultValue(text.into()))
    }

    /// Shown while it is empty.
    pub fn placeholder(self, text: impl Into<String>) -> Self {
        self.set(Prop::Placeholder(text.into()))
    }

    /// The placeholder's color (`cx.color(theme.muted_foreground)`).
    pub fn placeholder_color(self, color: Color) -> Self {
        self.set(Prop::PlaceholderColor(Some(color)))
    }

    /// The caret's and the selection's color.
    pub fn selection_color(self, color: Color) -> Self {
        self.set(Prop::SelectionColor(Some(color)))
    }

    /// Hides what is typed (passwords).
    pub fn secure(self, secure: bool) -> Self {
        self.set(Prop::Secure(secure))
    }

    /// The software keyboard's layout.
    pub fn keyboard_type(self, kind: KeyboardType) -> Self {
        self.set(Prop::KeyboardType(kind))
    }

    /// The return key's label.
    pub fn return_key_type(self, kind: ReturnKeyType) -> Self {
        self.set(Prop::ReturnKeyType(kind))
    }

    /// Automatic capitals.
    pub fn auto_capitalize(self, mode: AutoCapitalize) -> Self {
        self.set(Prop::AutoCapitalize(mode))
    }

    /// Spelling correction (default on).
    pub fn auto_correct(self, on: bool) -> Self {
        self.set(Prop::AutoCorrect(on))
    }

    /// What it holds, for autofill and suggestions.
    pub fn auto_fill(self, kind: AutoFill) -> Self {
        self.set(Prop::AutoFill(kind))
    }

    /// Several lines, growing with the text (return adds a line).
    pub fn multiline(self, multiline: bool) -> Self {
        self.set(Prop::Multiline(multiline))
    }

    /// At most `length` characters.
    pub fn max_length(self, length: u32) -> Self {
        self.set(Prop::MaxLength(Some(length)))
    }

    /// Editable (default yes).
    pub fn editable(self, editable: bool) -> Self {
        self.set(Prop::Editable(editable))
    }

    /// The return key unfocuses it (default yes, for one line).
    pub fn blur_on_submit(self, blur: bool) -> Self {
        self.set(Prop::BlurOnSubmit(blur))
    }

    /// Focused when it appears.
    pub fn auto_focus(self, focus: bool) -> Self {
        self.set(Prop::AutoFocus(focus))
    }

    /// No caret.
    pub fn caret_hidden(self, hidden: bool) -> Self {
        self.set(Prop::CaretHidden(hidden))
    }

    /// Shows the input accessory with this `nativeID` over the keyboard.
    pub fn input_accessory(self, native_id: &str) -> Self {
        self.set(Prop::InputAccessoryViewId(Some(native_id.into())))
    }

    /// The text changed (the new text).
    pub fn on_change_text(self, listener: Listener<str>) -> Self {
        self.listen(EventKind::ChangeText, listener, events::text)
    }

    /// The return key was pressed (the text).
    pub fn on_submit_editing(self, listener: Listener<str>) -> Self {
        self.listen(EventKind::SubmitEditing, listener, events::text)
    }

    /// It became focused.
    pub fn on_focus(self, listener: Listener<()>) -> Self {
        self.listen(EventKind::Focus, listener, events::unit)
    }

    /// It lost focus.
    pub fn on_blur(self, listener: Listener<()>) -> Self {
        self.listen(EventKind::Blur, listener, events::unit)
    }

    /// The selection or caret moved.
    pub fn on_selection_change(self, listener: Listener<Selection>) -> Self {
        self.listen(EventKind::SelectionChange, listener, events::selection)
    }

    /// A key was pressed: the character, `Backspace` or `Enter`.
    pub fn on_key_press(self, listener: Listener<str>) -> Self {
        self.listen(EventKind::KeyPress, listener, events::key)
    }

    /// The text's size changed (multiline inputs growing).
    pub fn on_content_size_change(self, listener: Listener<Size>) -> Self {
        self.listen(EventKind::ContentSizeChange, listener, events::size)
    }
}

impl Scroll {
    /// Scrolls horizontally.
    pub fn horizontal(self) -> Self {
        self.set(Prop::Horizontal(true))
    }

    /// Whether the scroll indicator shows (default yes).
    pub fn shows_indicator(self, shows: bool) -> Self {
        self.set(Prop::ShowsIndicator(shows))
    }

    /// Stops on multiples of its size.
    pub fn paging(self, paging: bool) -> Self {
        self.set(Prop::Scroll(ScrollProp::Paging(paging)))
    }

    /// Stops on multiples of `points`.
    pub fn snap_interval(self, points: f32) -> Self {
        self.set(Prop::Scroll(ScrollProp::SnapInterval(points)))
    }

    /// Stops at these offsets.
    pub fn snap_offsets(self, offsets: Vec<f32>) -> Self {
        self.set(Prop::Scroll(ScrollProp::SnapOffsets(offsets)))
    }

    /// Where a snapped item sits.
    pub fn snap_alignment(self, alignment: SnapAlignment) -> Self {
        self.set(Prop::Scroll(ScrollProp::SnapAlignment(alignment)))
    }

    /// How fast a fling slows down (0.99 fast, 0.998 normal).
    pub fn deceleration(self, rate: f32) -> Self {
        self.set(Prop::Scroll(ScrollProp::Deceleration(rate)))
    }

    /// Bounces at the ends (default yes).
    pub fn bounces(self, bounces: bool) -> Self {
        self.set(Prop::Scroll(ScrollProp::Bounces(bounces)))
    }

    /// Space around the content that scrolls with it.
    pub fn content_inset(self, insets: EdgeInsets) -> Self {
        self.set(Prop::Scroll(ScrollProp::ContentInset(insets)))
    }

    /// Where it starts.
    pub fn content_offset(self, x: f32, y: f32) -> Self {
        self.set(Prop::Scroll(ScrollProp::ContentOffset(x, y)))
    }

    /// Taps while the keyboard shows: dismiss it, or reach the content.
    pub fn keyboard_taps(self, taps: KeyboardTaps) -> Self {
        self.set(Prop::Scroll(ScrollProp::KeyboardTaps(taps)))
    }

    /// Dragging dismisses the keyboard.
    pub fn keyboard_dismiss_mode(self, mode: KeyboardDismissMode) -> Self {
        self.set(Prop::KeyboardDismissMode(mode))
    }

    /// At most one `on_scroll` per `ms` milliseconds (default: every frame).
    pub fn event_throttle(self, ms: f32) -> Self {
        self.set(Prop::Scroll(ScrollProp::EventThrottle(ms)))
    }

    /// Pinch zoom between `min` and `max`.
    pub fn zoom_range(self, min: f32, max: f32) -> Self {
        self.set(Prop::Scroll(ScrollProp::MinZoom(min)))
            .set(Prop::Scroll(ScrollProp::MaxZoom(max)))
    }

    /// The zoom.
    pub fn zoom(self, scale: f32) -> Self {
        self.set(Prop::Scroll(ScrollProp::Zoom(scale)))
    }

    /// Content smaller than the view sits in its center (zoomed images).
    pub fn center_content(self, center: bool) -> Self {
        self.set(Prop::Scroll(ScrollProp::CenterContent(center)))
    }

    /// Shows the pull-to-refresh spinner (start it from
    /// [`Self::on_refresh`], clear it when done).
    pub fn refreshing(self, refreshing: bool) -> Self {
        self.set(Prop::Refreshing(refreshing))
    }

    /// Follows the scroll offset (along its axis) with a native value:
    /// headers that shrink, parallax, with no render per frame.
    pub fn scroll_value(mut self, value: &super::NativeValue) -> Self {
        set(&mut self.0, Prop::ScrollValue(Some(value.node().id())));
        self.0.attrs.bound.push(value.node().clone());
        self
    }

    /// Pulled down to refresh (enables pull to refresh).
    pub fn on_refresh(self, listener: Listener<()>) -> Self {
        self.listen(EventKind::Refresh, listener, events::unit)
    }

    /// It moved (at most once per frame).
    pub fn on_scroll(self, listener: Listener<ScrollEvent>) -> Self {
        self.listen(EventKind::Scroll, listener, events::scroll)
    }

    /// The user started dragging it.
    pub fn on_scroll_begin_drag(self, listener: Listener<ScrollOffset>) -> Self {
        self.listen(EventKind::ScrollBeginDrag, listener, events::offset)
    }

    /// The user let go.
    pub fn on_scroll_end_drag(self, listener: Listener<ScrollOffset>) -> Self {
        self.listen(EventKind::ScrollEndDrag, listener, events::offset)
    }

    /// It started gliding after a fling.
    pub fn on_momentum_scroll_begin(self, listener: Listener<ScrollOffset>) -> Self {
        self.listen(EventKind::MomentumScrollBegin, listener, events::offset)
    }

    /// It stopped gliding.
    pub fn on_momentum_scroll_end(self, listener: Listener<ScrollOffset>) -> Self {
        self.listen(EventKind::MomentumScrollEnd, listener, events::offset)
    }

    /// It came to rest.
    pub fn on_scroll_end(self, listener: Listener<ScrollOffset>) -> Self {
        self.listen(EventKind::ScrollEnd, listener, events::offset)
    }
}

impl Screen {
    fn header_item(mut self, slot: HeaderSlot, key: &'static str, item: impl IntoElement) -> Self {
        let mut node = ElNode::new(Tag::HeaderItem);
        node.key = Some(key.into());
        set(&mut node, Prop::HeaderSlot(slot));
        node.children.push(item.into_element().0);
        self.0.children.push(node);
        self
    }

    fn screen(self, prop: ScreenProp) -> Self {
        self.set(Prop::Screen(prop))
    }

    /// The header's title.
    pub fn title(self, title: impl Into<Arc<str>>) -> Self {
        self.screen(ScreenProp::Title(Some(title.into())))
    }

    /// Whether the header shows (default true).
    pub fn header_shown(self, shown: bool) -> Self {
        self.screen(ScreenProp::HeaderShown(shown))
    }

    /// A large title (iOS); the content scrolls under the header.
    pub fn header_large_title(self, large: bool) -> Self {
        self.screen(ScreenProp::HeaderLargeTitle(large))
    }

    /// The content extends under the header.
    pub fn header_transparent(self, transparent: bool) -> Self {
        self.screen(ScreenProp::HeaderTransparent(transparent))
    }

    /// The back button's and bar buttons' color.
    pub fn header_tint_color(self, color: impl Into<Color>) -> Self {
        self.screen(ScreenProp::HeaderTintColor(Some(color.into())))
    }

    /// The header's background.
    pub fn header_background_color(self, color: impl Into<Color>) -> Self {
        self.screen(ScreenProp::HeaderBackgroundColor(Some(color.into())))
    }

    /// The title's color.
    pub fn header_title_color(self, color: impl Into<Color>) -> Self {
        self.screen(ScreenProp::HeaderTitleColor(Some(color.into())))
    }

    /// The label of the back button leading back to this screen (iOS).
    pub fn header_back_title(self, title: impl Into<Arc<str>>) -> Self {
        self.screen(ScreenProp::HeaderBackTitle(Some(title.into())))
    }

    /// Whether the back button shows (default true).
    pub fn header_back_visible(self, visible: bool) -> Self {
        self.screen(ScreenProp::HeaderBackVisible(visible))
    }

    /// The interactive back gesture (iOS swipe back, Android predictive
    /// back); the back buttons pop either way (default true).
    pub fn gesture_enabled(self, enabled: bool) -> Self {
        self.screen(ScreenProp::GestureEnabled(enabled))
    }

    /// The user cannot leave the screen: no swipe back, and the back
    /// buttons report [`Self::on_back_request`] instead of popping. Decided
    /// natively when the system asks, so set it before the user acts.
    pub fn prevent_back(self, prevent: bool) -> Self {
        self.screen(ScreenProp::PreventBack(prevent))
    }

    /// How the screen enters and leaves.
    pub fn animation(self, animation: ScreenAnimation) -> Self {
        self.screen(ScreenProp::Animation(animation))
    }

    /// An element at the header's start (instead of the back button).
    pub fn header_left(self, item: impl IntoElement) -> Self {
        self.header_item(HeaderSlot::Left, "header-left", item)
    }

    /// An element at the header's end.
    pub fn header_right(self, item: impl IntoElement) -> Self {
        self.header_item(HeaderSlot::Right, "header-right", item)
    }

    /// An element replacing the title.
    pub fn header_title(self, item: impl IntoElement) -> Self {
        self.header_item(HeaderSlot::Title, "header-title", item)
    }

    /// The user popped the screen natively (back button, swipe): leave it
    /// out of the next render.
    pub fn on_dismiss(self, listener: Listener<()>) -> Self {
        self.listen(EventKind::Dismiss, listener, events::unit)
    }

    /// The user tried to leave a screen that [`Self::prevent_back`]s.
    pub fn on_back_request(self, listener: Listener<()>) -> Self {
        self.listen(EventKind::BackRequest, listener, events::unit)
    }
}

impl Modal {
    /// How it comes in (default: slides up).
    pub fn animation(self, animation: ModalAnimation) -> Self {
        self.set(Prop::Modal(ModalProp::Animation(animation)))
    }

    /// Full screen, or a sheet (iOS).
    pub fn presentation(self, presentation: ModalPresentation) -> Self {
        self.set(Prop::Modal(ModalProp::Presentation(presentation)))
    }

    /// What is behind shows through (its content draws the dimming).
    pub fn transparent(self, transparent: bool) -> Self {
        self.set(Prop::Modal(ModalProp::Transparent(transparent)))
    }

    /// Under the status bar (Android).
    pub fn status_bar_translucent(self, translucent: bool) -> Self {
        self.set(Prop::Modal(ModalProp::StatusBarTranslucent(translucent)))
    }

    /// Swipes down and the back button dismiss it, reporting
    /// [`Self::on_dismiss`] (default: they ask with
    /// [`Self::on_request_close`]).
    pub fn dismissible(self, dismissible: bool) -> Self {
        self.set(Prop::Modal(ModalProp::Dismissible(dismissible)))
    }

    /// The user asked to close it (back button, swipe down): leave it out
    /// of the next render to close it.
    pub fn on_request_close(self, listener: Listener<()>) -> Self {
        self.listen(EventKind::RequestClose, listener, events::unit)
    }

    /// It finished appearing.
    pub fn on_show(self, listener: Listener<()>) -> Self {
        self.listen(EventKind::Show, listener, events::unit)
    }

    /// The user or the system dismissed it.
    pub fn on_dismiss(self, listener: Listener<()>) -> Self {
        self.listen(EventKind::Dismiss, listener, events::unit)
    }
}

impl WebView {
    /// Loads `html` (relative links against `base_url`) instead of a URL.
    pub fn html(self, html: &str, base_url: Option<&str>) -> Self {
        self.set(Prop::Native(NativeProp::Source(WebSource::Html {
            html: html.into(),
            base_url: base_url.map(Into::into),
        })))
    }

    /// Runs after each page loads.
    pub fn injected_java_script(self, script: &str) -> Self {
        self.set(Prop::Native(NativeProp::InjectedJavaScript(Some(
            script.into(),
        ))))
    }

    /// Runs before each page's own scripts.
    pub fn injected_java_script_before_content(self, script: &str) -> Self {
        self.set(Prop::Native(NativeProp::InjectedJavaScriptBefore(Some(
            script.into(),
        ))))
    }

    /// The user agent it sends.
    pub fn user_agent(self, agent: &str) -> Self {
        self.set(Prop::Native(NativeProp::UserAgent(Some(agent.into()))))
    }

    /// Scrolls (default yes).
    pub fn scroll_enabled(self, enabled: bool) -> Self {
        self.set(Prop::Native(NativeProp::ScrollEnabled(enabled)))
    }

    /// Runs the pages' JavaScript (default yes).
    pub fn java_script_enabled(self, enabled: bool) -> Self {
        self.set(Prop::Native(NativeProp::JavaScriptEnabled(enabled)))
    }

    /// Swipes go back and forward (iOS).
    pub fn back_forward_gestures(self, enabled: bool) -> Self {
        self.set(Prop::Native(NativeProp::BackForwardGestures(enabled)))
    }

    /// A page started loading.
    pub fn on_load_start(self, listener: Listener<WebNavigation>) -> Self {
        self.listen(EventKind::LoadStart, listener, events::navigation)
    }

    /// A page loaded.
    pub fn on_load(self, listener: Listener<WebNavigation>) -> Self {
        self.listen(EventKind::Load, listener, events::navigation)
    }

    /// A page failed to load (the message).
    pub fn on_error(self, listener: Listener<str>) -> Self {
        self.listen(EventKind::Error, listener, events::message)
    }

    /// The page called `window.ReactNativeWebView.postMessage(data)`.
    pub fn on_message(self, listener: Listener<str>) -> Self {
        self.listen(EventKind::Message, listener, events::data)
    }

    /// It navigated.
    pub fn on_navigation_state_change(self, listener: Listener<WebNavigation>) -> Self {
        self.listen(
            EventKind::NavigationStateChange,
            listener,
            events::navigation,
        )
    }
}

impl Video {
    /// Paused.
    pub fn paused(self, paused: bool) -> Self {
        self.set(Prop::Native(NativeProp::Paused(paused)))
    }

    /// Without sound.
    pub fn muted(self, muted: bool) -> Self {
        self.set(Prop::Native(NativeProp::Muted(muted)))
    }

    /// 0..1.
    pub fn volume(self, volume: f32) -> Self {
        self.set(Prop::Native(NativeProp::Volume(volume)))
    }

    /// Playback speed.
    pub fn rate(self, rate: f32) -> Self {
        self.set(Prop::Native(NativeProp::Rate(rate)))
    }

    /// Starts over at the end.
    pub fn repeat(self, repeat: bool) -> Self {
        self.set(Prop::Native(NativeProp::Repeat(repeat)))
    }

    /// How the picture fits (default: contain).
    pub fn resize_mode(self, mode: ResizeMode) -> Self {
        self.set(Prop::Native(NativeProp::VideoResizeMode(mode)))
    }

    /// The platform's playback controls.
    pub fn controls(self, controls: bool) -> Self {
        self.set(Prop::Native(NativeProp::Controls(controls)))
    }

    /// Milliseconds between [`Self::on_progress`] events (default 250).
    pub fn progress_interval(self, ms: f32) -> Self {
        self.set(Prop::Native(NativeProp::ProgressInterval(ms)))
    }

    /// It started loading.
    pub fn on_load_start(self, listener: Listener<()>) -> Self {
        self.listen(EventKind::LoadStart, listener, events::unit)
    }

    /// It loaded: length and size.
    pub fn on_load(self, listener: Listener<VideoInfo>) -> Self {
        self.listen(EventKind::Load, listener, events::video)
    }

    /// It played on.
    pub fn on_progress(self, listener: Listener<VideoProgress>) -> Self {
        self.listen(EventKind::Progress, listener, events::progress)
    }

    /// It played to its end.
    pub fn on_end(self, listener: Listener<()>) -> Self {
        self.listen(EventKind::End, listener, events::unit)
    }

    /// It started (`true`) or stopped buffering.
    pub fn on_buffer(self, listener: Listener<bool>) -> Self {
        self.listen(EventKind::Buffer, listener, events::buffering)
    }

    /// It failed (the message).
    pub fn on_error(self, listener: Listener<str>) -> Self {
        self.listen(EventKind::Error, listener, events::message)
    }
}

impl NativeView {
    /// Its prop `name` (any JSON value), as JS's `nativeComponent` passes it.
    pub fn prop(self, name: &str, value: JsonValue) -> Self {
        self.set(Prop::ComponentProp(name.into(), Some(value)))
    }

    /// It emitted an event.
    pub fn on_native_event(self, listener: Listener<NativeEvent>) -> Self {
        self.listen(EventKind::NativeEvent, listener, events::native)
    }
}

/// A number in a drawing: a constant, or a native value the canvas reads
/// while it draws (it draws again natively as the value moves).
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasArg(pub(crate) CanvasArgKind);

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CanvasArgKind {
    Const(f32),
    Node(super::NativeNode),
}

impl From<f32> for CanvasArg {
    fn from(n: f32) -> Self {
        Self(CanvasArgKind::Const(n))
    }
}

impl<A: super::Animated> From<&A> for CanvasArg {
    fn from(a: &A) -> Self {
        Self(CanvasArgKind::Node(a.native_node()))
    }
}

/// Records a [`Drawing`] with the HTML Canvas 2D operations (Rust names):
/// `Painter::new().fill_color(red).fill_rect(0.0, 0.0, w, h).finish()`.
/// Coordinates are points from the canvas's top left.
#[derive(Debug, Clone, Default)]
pub struct Painter {
    drawing: Drawing,
    /// The native values the drawing reads, kept alive with it.
    nodes: Vec<super::NativeNode>,
}

impl Painter {
    /// An empty drawing.
    pub fn new() -> Self {
        Self::default()
    }

    /// The drawing.
    pub fn finish(self) -> Drawing {
        self.drawing
    }

    fn num(&mut self, n: impl Into<CanvasArg>) -> Num {
        match n.into().0 {
            CanvasArgKind::Const(n) => Num::Const(n),
            CanvasArgKind::Node(node) => {
                let slot = self.drawing.slot(node.id());
                self.nodes.push(node);
                Num::Slot(slot)
            }
        }
    }

    fn op(mut self, op: CanvasOp) -> Self {
        self.drawing.ops.push(op);
        self
    }

    fn path(self, op: PathOp) -> Self {
        self.op(CanvasOp::Path(op))
    }

    /// Saves the state (styles, transform, clip).
    pub fn save(self) -> Self {
        self.op(CanvasOp::Save)
    }

    /// Restores the state saved last.
    pub fn restore(self) -> Self {
        self.op(CanvasOp::Restore)
    }

    /// Moves the origin.
    pub fn translate(mut self, x: impl Into<CanvasArg>, y: impl Into<CanvasArg>) -> Self {
        let (x, y) = (self.num(x), self.num(y));
        self.op(CanvasOp::Translate(x, y))
    }

    /// Scales the axes.
    pub fn scale(mut self, x: impl Into<CanvasArg>, y: impl Into<CanvasArg>) -> Self {
        let (x, y) = (self.num(x), self.num(y));
        self.op(CanvasOp::Scale(x, y))
    }

    /// Rotates the axes (radians, clockwise).
    pub fn rotate(mut self, angle: impl Into<CanvasArg>) -> Self {
        let a = self.num(angle);
        self.op(CanvasOp::Rotate(a))
    }

    /// Starts a new path.
    pub fn begin_path(self) -> Self {
        self.op(CanvasOp::BeginPath)
    }

    /// Starts a subpath at (`x`, `y`).
    pub fn move_to(mut self, x: impl Into<CanvasArg>, y: impl Into<CanvasArg>) -> Self {
        let (x, y) = (self.num(x), self.num(y));
        self.path(PathOp::MoveTo(x, y))
    }

    /// A straight line to (`x`, `y`).
    pub fn line_to(mut self, x: impl Into<CanvasArg>, y: impl Into<CanvasArg>) -> Self {
        let (x, y) = (self.num(x), self.num(y));
        self.path(PathOp::LineTo(x, y))
    }

    /// A cubic Bézier curve to (`x`, `y`).
    pub fn bezier_curve_to(self, c1: (f32, f32), c2: (f32, f32), to: (f32, f32)) -> Self {
        let c = Num::Const;
        self.path(PathOp::CubicTo([
            c(c1.0),
            c(c1.1),
            c(c2.0),
            c(c2.1),
            c(to.0),
            c(to.1),
        ]))
    }

    /// A circular arc around (`x`, `y`); angles in radians, clockwise from
    /// the x axis.
    pub fn arc(
        mut self,
        x: impl Into<CanvasArg>,
        y: impl Into<CanvasArg>,
        radius: impl Into<CanvasArg>,
        start: impl Into<CanvasArg>,
        end: impl Into<CanvasArg>,
    ) -> Self {
        let (x, y, radius, start, end) = (
            self.num(x),
            self.num(y),
            self.num(radius),
            self.num(start),
            self.num(end),
        );
        self.path(PathOp::Arc {
            x,
            y,
            radius,
            start,
            end,
            ccw: false,
        })
    }

    /// A rectangle subpath.
    pub fn rect(self, x: f32, y: f32, w: f32, h: f32) -> Self {
        let c = Num::Const;
        self.path(PathOp::Rect([c(x), c(y), c(w), c(h)]))
    }

    /// A rectangle subpath with round corners (one radius for all).
    pub fn round_rect(self, x: f32, y: f32, w: f32, h: f32, radius: f32) -> Self {
        let c = Num::Const;
        self.path(PathOp::RoundRect([c(x), c(y), c(w), c(h)], [c(radius); 4]))
    }

    /// A straight line back to the subpath's start.
    pub fn close_path(self) -> Self {
        self.path(PathOp::Close)
    }

    /// Fills the current path.
    pub fn fill(self) -> Self {
        self.op(CanvasOp::Fill(None, FillRule::NonZero))
    }

    /// Fills `path` (SVG path data, parsed now).
    pub fn fill_svg(self, data: &str) -> Self {
        let path: Path = crate::canvas::parse_svg_path(data).into();
        self.op(CanvasOp::Fill(Some(path), FillRule::NonZero))
    }

    /// Strokes the current path.
    pub fn stroke(self) -> Self {
        self.op(CanvasOp::Stroke(None))
    }

    /// Clips to the current path.
    pub fn clip(self) -> Self {
        self.op(CanvasOp::Clip(None, FillRule::NonZero))
    }

    /// Fills a rectangle.
    pub fn fill_rect(
        mut self,
        x: f32,
        y: f32,
        w: impl Into<CanvasArg>,
        h: impl Into<CanvasArg>,
    ) -> Self {
        let (w, h) = (self.num(w), self.num(h));
        self.op(CanvasOp::FillRect([Num::Const(x), Num::Const(y), w, h]))
    }

    /// Strokes a rectangle.
    pub fn stroke_rect(self, x: f32, y: f32, w: f32, h: f32) -> Self {
        let c = Num::Const;
        self.op(CanvasOp::StrokeRect([c(x), c(y), c(w), c(h)]))
    }

    /// Makes a rectangle transparent.
    pub fn clear_rect(self, x: f32, y: f32, w: f32, h: f32) -> Self {
        let c = Num::Const;
        self.op(CanvasOp::ClearRect([c(x), c(y), c(w), c(h)]))
    }

    /// Fills with `color`.
    pub fn fill_color(self, color: Color) -> Self {
        self.op(CanvasOp::FillStyle(Paint::Color(color)))
    }

    /// Strokes with `color`.
    pub fn stroke_color(self, color: Color) -> Self {
        self.op(CanvasOp::StrokeStyle(Paint::Color(color)))
    }

    /// Fills with a linear gradient from (`x0`, `y0`) to (`x1`, `y1`).
    pub fn fill_linear_gradient(
        self,
        from: (f32, f32),
        to: (f32, f32),
        stops: Vec<(f32, Color)>,
    ) -> Self {
        self.op(CanvasOp::FillStyle(Paint::Gradient(Arc::new(
            CanvasGradient {
                shape: GradientShape::Linear([from.0, from.1, to.0, to.1]),
                stops,
            },
        ))))
    }

    /// The stroke width.
    pub fn line_width(mut self, width: impl Into<CanvasArg>) -> Self {
        let w = self.num(width);
        self.op(CanvasOp::LineWidth(w))
    }

    /// How strokes end.
    pub fn line_cap(self, cap: LineCap) -> Self {
        self.op(CanvasOp::LineCap(cap))
    }

    /// How stroke segments join.
    pub fn line_join(self, join: LineJoin) -> Self {
        self.op(CanvasOp::LineJoin(join))
    }

    /// Dashes: lengths of dashes and gaps.
    pub fn line_dash(self, segments: &[f32]) -> Self {
        self.op(CanvasOp::LineDash(segments.into()))
    }

    /// The opacity of what is drawn next.
    pub fn global_alpha(mut self, alpha: impl Into<CanvasArg>) -> Self {
        let a = self.num(alpha);
        self.op(CanvasOp::GlobalAlpha(a))
    }

    /// How what is drawn next combines with the canvas.
    pub fn composite(self, composite: Composite) -> Self {
        self.op(CanvasOp::Composite(composite))
    }

    /// Shadows of what is drawn next.
    pub fn shadow(self, color: Color, blur: f32, offset: (f32, f32)) -> Self {
        self.op(CanvasOp::ShadowColor(color))
            .op(CanvasOp::ShadowBlur(blur))
            .op(CanvasOp::ShadowOffset(offset.0, offset.1))
    }

    /// The font (CSS `font` shorthand: `"600 14px Inter"`).
    pub fn font(self, font: &str) -> Self {
        match CanvasFont::parse(font) {
            Some(font) => self.op(CanvasOp::Font(font)),
            None => self,
        }
    }

    /// Where text sits relative to its x.
    pub fn text_align(self, align: CanvasTextAlign) -> Self {
        self.op(CanvasOp::TextAlign(align))
    }

    /// Where text sits relative to its y.
    pub fn text_baseline(self, baseline: CanvasTextBaseline) -> Self {
        self.op(CanvasOp::TextBaseline(baseline))
    }

    /// Fills `text` at (`x`, `y`).
    pub fn fill_text(self, text: &str, x: f32, y: f32) -> Self {
        self.op(CanvasOp::FillText {
            text: text.into(),
            x: Num::Const(x),
            y: Num::Const(y),
            max_width: None,
        })
    }

    /// Draws the image at `uri` (a URL or `asset:/path`) into the rectangle.
    pub fn draw_image(self, uri: &str, x: f32, y: f32, w: f32, h: f32) -> Self {
        let c = Num::Const;
        self.op(CanvasOp::DrawImage {
            image: CanvasImage { uri: uri.into() },
            source: None,
            dest: [c(x), c(y), c(w), c(h)],
        })
    }
}

impl From<Drawing> for Painter {
    fn from(drawing: Drawing) -> Self {
        Self {
            drawing,
            nodes: Vec::new(),
        }
    }
}
