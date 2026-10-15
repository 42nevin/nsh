use std::sync::Arc;

use yf_anim::AnimOp;

use crate::{
    props::{AccessibilityProp, EdgeInsets, LayoutAnimation, PointerEvents},
    style::{Color, EllipsizeMode, Style, StylePatch},
};

/// Identifier of a node, assigned by the JS renderer.
///
/// Ids are small, dense and increasing, so the core indexes them directly.
pub type NodeId = u32;

/// Id of the surface root view. It exists before any JS runs.
pub const ROOT_ID: NodeId = 1;

/// Element tag of a host node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    /// A container view (`<view>`).
    View,
    /// A text block, or a span when nested in another text (`<text>`).
    Text,
    /// An image (`<image>`).
    Image,
    /// A scrolling container (`<scroll>`).
    Scroll,
    /// A virtualized list (`<list>`, created by the runtime's `List`).
    List,
    /// A recycled list cell (created by the runtime's `List`).
    Cell,
    /// A text field, or a text area when `multiline` (`<input>`).
    Input,
    /// A native navigation stack (`<stack>`); its children are screens.
    Stack,
    /// One screen of a stack (`<screen>`), laid out at the stack's size.
    Screen,
    /// A modal presented over everything (`<modal>`), laid out at the size
    /// the platform presents it at.
    Modal,
    /// A view placed in a screen's native header (`<headeritem slot>`),
    /// sized to its content.
    HeaderItem,
    /// A bar over the keyboard of the inputs naming its `nativeID`
    /// (`<inputaccessory>`, iOS), as wide as the window and as tall as its
    /// content.
    InputAccessory,
    /// A 2D drawing (`<canvas>`, filled by [`Op::Draw`]).
    Canvas,
    /// A web page (`<webview>`).
    WebView,
    /// A video player (`<video>`).
    Video,
}

impl Tag {
    /// Parses a JSX intrinsic element name.
    #[inline]
    pub fn from_name(name: &str) -> Option<Tag> {
        Some(match name {
            "view" => Tag::View,
            "text" => Tag::Text,
            "image" => Tag::Image,
            "scroll" => Tag::Scroll,
            "list" => Tag::List,
            "cell" => Tag::Cell,
            "input" => Tag::Input,
            "stack" => Tag::Stack,
            "screen" => Tag::Screen,
            "modal" => Tag::Modal,
            "headeritem" => Tag::HeaderItem,
            "inputaccessory" => Tag::InputAccessory,
            "canvas" => Tag::Canvas,
            "webview" => Tag::WebView,
            "video" => Tag::Video,
            _ => return None,
        })
    }
}

macro_rules! events {
    ($($variant:ident = $prop:literal: $doc:literal,)*) => {
        /// An event a node can listen to.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[repr(u8)]
        pub enum EventKind {
            $(#[doc = $doc] $variant,)*
        }

        impl EventKind {
            /// Every kind, in discriminant order (`ALL[k as usize] == k`).
            pub const ALL: &'static [EventKind] = &[$(EventKind::$variant,)*];

            /// Parses an event prop name such as `onPress`.
            #[inline]
            pub fn from_prop(name: &str) -> Option<EventKind> {
                match name {
                    $($prop => Some(EventKind::$variant),)*
                    _ => None,
                }
            }

            /// The JS prop name of this event.
            #[inline]
            pub const fn prop_name(self) -> &'static str {
                match self {
                    $(EventKind::$variant => $prop,)*
                }
            }
        }
    };
}

// The order is part of the platform protocols (`encode.rs`, `Native.java`):
// append new kinds at the end.
events! {
    Press = "onPress": "A tap / click.",
    LongPress = "onLongPress": "A press held for about half a second.",
    PanStart = "onPanStart": "A drag started (payload: [`EventPayload::Pan`]).",
    PanEnd = "onPanEnd": "A drag ended (payload: [`EventPayload::Pan`] with velocity).",
    Refresh = "onRefresh": "Pull-to-refresh was triggered on a scroll or list.",
    EndReached = "onEndReached": "A list was scrolled close to its end.",
    ScrollEnd = "onScrollEnd": "Scrolling came to rest (payload: [`EventPayload::Scroll`]).",
    Load = "onLoad": "An image loaded (payload: [`EventPayload::Image`]).",
    Error = "onError": "An image failed to load (payload: [`EventPayload::Error`]).",
    ChangeText = "onChangeText": "The user edited an input (payload: [`EventPayload::Text`]).",
    SubmitEditing = "onSubmitEditing": "The return key was pressed in an input (payload: [`EventPayload::Text`]).",
    Focus = "onFocus": "An input became focused.",
    Blur = "onBlur": "An input lost focus.",
    PressIn = "onPressIn": "A finger went down on a pressable view.",
    PressOut = "onPressOut": "The press ended, inside or outside the view.",
    Layout = "onLayout": "The frame changed (payload: [`EventPayload::Layout`], relative to the parent).",
    PinchStart = "onPinchStart": "A pinch started (payload: `scale`, `velocity`).",
    PinchEnd = "onPinchEnd": "A pinch ended (payload: `scale`, `velocity`).",
    RotationStart = "onRotationStart": "A two-finger rotation started (payload: `rotation`, `velocity`).",
    RotationEnd = "onRotationEnd": "A two-finger rotation ended (payload: `rotation`, `velocity`).",
    AccessibilityAction = "onAccessibilityAction": "An assistive technology performed an action (payload: `actionName`).",
    Dismiss = "onDismiss": "A screen was popped or a modal dismissed by the user or the system.",
    RequestClose = "onRequestClose": "The user asked to close a modal (back button, swipe down).",
    Show = "onShow": "A modal finished appearing.",
    Insets = "onInsets": "The insets of a screen or modal changed (payload: `top`, `right`, `bottom`, `left`).",
    LoadStart = "onLoadStart": "A web view, video or image started loading.",
    Message = "onMessage": "A web page posted a message (payload: `data`).",
    NavigationStateChange = "onNavigationStateChange": "A web view navigated (payload: `url`, `title`, `loading`, `canGoBack`, `canGoForward`).",
    Progress = "onProgress": "Video playback progressed (payload: `currentTime`, `duration`).",
    End = "onEnd": "A video played to its end.",
    Buffer = "onBuffer": "A video started or stopped buffering (payload: `isBuffering`).",
    ViewableItemsChanged = "onViewableItemsChanged": "The range of visible list items changed (payload: `first`, `last`).",
    BackPress = "onBackPress": "The Android back button was pressed and nothing native handled it (root only).",
    Scroll = "onScroll": "A scroll view moved, at most once per frame (payload: `x`, `y`, `contentWidth`, `contentHeight`, `width`, `height`).",
    ScrollBeginDrag = "onScrollBeginDrag": "The user started dragging a scroll view (payload: [`EventPayload::Scroll`]).",
    ScrollEndDrag = "onScrollEndDrag": "The user let go of a scroll view (payload: [`EventPayload::Scroll`]).",
    MomentumScrollBegin = "onMomentumScrollBegin": "A scroll view started gliding after a fling (payload: [`EventPayload::Scroll`]).",
    MomentumScrollEnd = "onMomentumScrollEnd": "A scroll view stopped gliding (payload: [`EventPayload::Scroll`]).",
    Dialog = "onDialog": "An alert or action sheet closed (root only; payload: `id`, `index`, -1 when dismissed).",
    Touch = "onTouch": "Touches on the view or inside it, for the JS touch and responder handlers (payload: [`EventPayload::Touch`]).",
    SelectionChange = "onSelectionChange": "The selection (or caret) of an input moved (payload: `start`, `end` in UTF-16 units).",
    KeyPress = "onKeyPress": "A key was pressed in an input (payload: `key`: the character, `Backspace` or `Enter`).",
    LoadEnd = "onLoadEnd": "An image finished loading, or failed.",
    ImageInfo = "onImageInfo": "An `Image.getSize` / `Image.prefetch` request finished (root only; payload: `id`, `width` and `height` in pixels, or `error`).",
    TextLayout = "onTextLayout": "A text block was laid out (payload: [`EventPayload::TextLayout`]).",
    ContentSizeChange = "onContentSizeChange": "The size of an input's text changed (payload: `width`, `height`; raised by the core).",
}

/// What an input holds, for the system's autofill and suggestions
/// (`autoComplete`, `textContentType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoFill {
    /// The platform decides.
    #[default]
    Default,
    /// No autofill.
    Off,
    /// Account name.
    Username,
    /// Existing password.
    Password,
    /// A password being chosen.
    NewPassword,
    /// A code from a text message or e-mail.
    OneTimeCode,
    /// E-mail address.
    Email,
    /// Phone number.
    Tel,
    /// Full name.
    Name,
    /// First name.
    GivenName,
    /// Last name.
    FamilyName,
    /// Middle name.
    MiddleName,
    /// Nickname.
    Nickname,
    /// Company.
    Organization,
    /// Street address, all lines.
    StreetAddress,
    /// First address line.
    AddressLine1,
    /// Second address line.
    AddressLine2,
    /// City.
    City,
    /// State or region.
    Region,
    /// Postal code.
    PostalCode,
    /// Country.
    Country,
    /// Credit card number.
    CreditCardNumber,
    /// Credit card expiry date.
    CreditCardExpiration,
    /// Credit card security code.
    CreditCardSecurityCode,
    /// Name on the credit card.
    CreditCardName,
    /// Web address.
    Url,
    /// Birth date.
    Birthdate,
}

impl AutoFill {
    /// Parses React Native's `autoComplete` values and iOS's
    /// `textContentType` names.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "off" | "none" => Self::Off,
            "username" | "username-new" => Self::Username,
            "password" | "current-password" => Self::Password,
            "new-password" | "newPassword" | "password-new" => Self::NewPassword,
            "one-time-code" | "oneTimeCode" | "sms-otp" => Self::OneTimeCode,
            "email" | "emailAddress" => Self::Email,
            "tel" | "telephoneNumber" | "tel-national" | "tel-device" | "tel-country-code" => {
                Self::Tel
            }
            "name" | "name-full" => Self::Name,
            "given-name" | "givenName" | "name-given" => Self::GivenName,
            "family-name" | "familyName" | "name-family" => Self::FamilyName,
            "additional-name" | "middleName" | "name-middle" => Self::MiddleName,
            "nickname" => Self::Nickname,
            "organization" | "organizationName" => Self::Organization,
            "street-address" | "fullStreetAddress" | "postal-address" => Self::StreetAddress,
            "address-line1" | "streetAddressLine1" => Self::AddressLine1,
            "address-line2" | "streetAddressLine2" => Self::AddressLine2,
            "address-level2" | "addressCity" | "postal-address-locality" => Self::City,
            "address-level1" | "addressState" | "postal-address-region" => Self::Region,
            "postal-code" | "postalCode" => Self::PostalCode,
            "country" | "countryName" | "postal-address-country" => Self::Country,
            "cc-number" | "creditCardNumber" => Self::CreditCardNumber,
            "cc-exp" | "creditCardExpiration" | "cc-exp-month" | "cc-exp-year" => {
                Self::CreditCardExpiration
            }
            "cc-csc" | "creditCardSecurityCode" => Self::CreditCardSecurityCode,
            "cc-name" | "creditCardName" => Self::CreditCardName,
            "url" | "URL" => Self::Url,
            "birthdate-full" | "birthdate" | "birthdate-day" | "birthdate-month"
            | "birthdate-year" => Self::Birthdate,
            _ => return None,
        })
    }
}

/// The phase of a [`TouchEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    /// Fingers went down (the changed points).
    Start,
    /// Fingers moved.
    Move,
    /// Fingers lifted (the changed points).
    End,
    /// The system or a native gesture took the touches over.
    Cancel,
}

/// One finger of a [`TouchEvent`], in points.
#[derive(Debug, Clone, PartialEq)]
pub struct TouchPoint {
    /// Identifies the finger while it is down.
    pub id: u32,
    /// The view under the finger when it went down.
    pub target: NodeId,
    /// The views around `target` (it included) that listen to touches,
    /// innermost first.
    pub path: Vec<NodeId>,
    /// Position in the surface.
    pub page_x: f32,
    /// Position in the surface.
    pub page_y: f32,
    /// Position in `target`.
    pub location_x: f32,
    /// Position in `target`.
    pub location_y: f32,
    /// This finger is one the event is about (it went down, moved or
    /// lifted).
    pub changed: bool,
}

/// Touches for the JS responder system.
#[derive(Debug, Clone, PartialEq)]
pub struct TouchEvent {
    /// What happened.
    pub phase: TouchPhase,
    /// Milliseconds since an arbitrary origin.
    pub timestamp: f64,
    /// The fingers down during the event (with those that lifted).
    pub points: Vec<TouchPoint>,
}

/// A set of [`EventKind`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EventMask(u64);

impl EventMask {
    /// Returns true if `event` is in the set.
    #[inline]
    pub const fn contains(self, event: EventKind) -> bool {
        self.0 & (1 << event as u8) != 0
    }

    /// The raw bit set, indexed by `EventKind as u8`.
    #[inline]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Adds or removes `event`.
    #[inline]
    pub fn set(&mut self, event: EventKind, enabled: bool) {
        if enabled {
            self.0 |= 1 << event as u8;
        } else {
            self.0 &= !(1 << event as u8);
        }
    }

    /// Returns true if no event is set.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A value in an event payload.
#[derive(Debug, Clone, PartialEq)]
pub enum PayloadValue {
    /// A number.
    Number(f64),
    /// A boolean.
    Bool(bool),
    /// A string.
    Str(String),
    /// `null`.
    Null,
}

/// Data delivered with an event.
#[derive(Debug, Clone, PartialEq)]
pub enum EventPayload {
    /// No data.
    None,
    /// A single value handed to the handler as is (`onValueChange(value)`).
    Value(PayloadValue),
    /// An object with named fields.
    Map(Vec<(&'static str, PayloadValue)>),
    /// A frame relative to the parent (`onLayout`).
    Layout {
        /// Left edge.
        x: f32,
        /// Top edge.
        y: f32,
        /// Width.
        width: f32,
        /// Height.
        height: f32,
    },
    /// A drag: translation since the start and velocity (points / second).
    Pan {
        /// Horizontal translation.
        translation_x: f32,
        /// Vertical translation.
        translation_y: f32,
        /// Horizontal velocity.
        velocity_x: f32,
        /// Vertical velocity.
        velocity_y: f32,
        /// The `pan.x` value now (`None` without one).
        value_x: Option<f32>,
        /// The `pan.y` value now.
        value_y: Option<f32>,
    },
    /// A scroll position.
    Scroll {
        /// Horizontal content offset.
        x: f32,
        /// Vertical content offset.
        y: f32,
    },
    /// A loaded image's source size in pixels (before downsampling).
    Image {
        /// Width.
        width: f32,
        /// Height.
        height: f32,
    },
    /// An error message.
    Error {
        /// The message.
        message: String,
    },
    /// The text of an input after an edit.
    Text {
        /// The whole text.
        text: String,
        /// Number of edits the input has reported so far. A value set by JS
        /// based on an older edit is stale and must not overwrite newer
        /// typing.
        count: u32,
    },
    /// Touches (`onTouch`).
    Touch(Box<TouchEvent>),
    /// The lines of a text block.
    TextLayout(Vec<crate::text::TextLine>),
}

impl TouchEvent {
    /// Folds a later move into this pending one: the later positions,
    /// changed if either moved them.
    pub fn merge_move(&mut self, later: TouchEvent) {
        let mut later = later;
        for p in &mut later.points {
            if let Some(old) = self.points.iter().find(|o| o.id == p.id) {
                p.changed |= old.changed;
            }
        }
        *self = later;
    }
}

/// How an image fits its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResizeMode {
    /// Fill the box, cropping (default).
    #[default]
    Cover,
    /// Fit inside the box, letterboxing.
    Contain,
    /// Stretch to the box.
    Stretch,
    /// Center at natural size.
    Center,
}

/// An image source.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSource {
    /// `https://…`, `asset:/path` (bundled file) or a file path.
    pub uri: Arc<str>,
    /// Natural size in points, when known up front (bundled assets): used
    /// as the intrinsic layout size.
    pub size: Option<(f32, f32)>,
}

/// Keyboard layout of an input (`keyboardType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardType {
    /// Full keyboard.
    #[default]
    Default,
    /// Digits only (`number-pad`).
    Number,
    /// Digits and a decimal separator (`decimal-pad`).
    Decimal,
    /// E-mail addresses (`email-address`).
    Email,
    /// Phone numbers (`phone-pad`).
    Phone,
    /// URLs (`url`).
    Url,
}

/// Label of the return key (`returnKeyType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReturnKeyType {
    /// Return / new line.
    #[default]
    Default,
    /// Done.
    Done,
    /// Go.
    Go,
    /// Next.
    Next,
    /// Search.
    Search,
    /// Send.
    Send,
}

/// Automatic capitalization (`autoCapitalize`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoCapitalize {
    /// Never.
    None,
    /// First letter of each sentence (default).
    #[default]
    Sentences,
    /// First letter of each word.
    Words,
    /// Every letter.
    Characters,
}

/// When a scroll view dismisses the keyboard (`keyboardDismissMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardDismissMode {
    /// Never (default).
    #[default]
    None,
    /// As soon as a drag starts.
    OnDrag,
    /// Follows the finger while dragging down (iOS).
    Interactive,
}

/// Which point of a page a snapping scroll view aligns to its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SnapAlignment {
    /// The snap point at the viewport's start.
    #[default]
    Start,
    /// At its center.
    Center,
    /// At its end.
    End,
}

/// Whether a tap in a scroll view dismisses the keyboard
/// (`keyboardShouldPersistTaps`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardTaps {
    /// The tap dismisses the keyboard and reaches nothing else (default).
    #[default]
    Never,
    /// Taps never dismiss the keyboard.
    Always,
    /// Taps that nothing handles dismiss the keyboard.
    Handled,
}

/// How an `<image>` is shown, besides its source.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageProp {
    /// `defaultSource`: a placeholder until the image loads.
    DefaultSource(Option<ImageSource>),
    /// `blurRadius` in points.
    BlurRadius(f32),
    /// `fadeDuration` in milliseconds (`None`: the platform's default).
    FadeDuration(Option<f32>),
}

/// A behavior property of a scroll view or list.
#[derive(Debug, Clone, PartialEq)]
pub enum ScrollProp {
    /// `pagingEnabled`: stops at multiples of the viewport.
    Paging(bool),
    /// `snapToInterval`: stops at multiples of this length (0: off).
    SnapInterval(f32),
    /// `snapToOffsets`: stops at these offsets.
    SnapOffsets(Vec<f32>),
    /// `snapToAlignment`.
    SnapAlignment(SnapAlignment),
    /// `decelerationRate`: the velocity kept per millisecond after a fling
    /// (0.998 normal, 0.99 fast; 0 is the platform's default).
    Deceleration(f32),
    /// `bounces`: overscroll past the edges.
    Bounces(bool),
    /// `contentInset`: room around the content.
    ContentInset(EdgeInsets),
    /// `contentOffset`: the initial offset.
    ContentOffset(f32, f32),
    /// `keyboardShouldPersistTaps`.
    KeyboardTaps(KeyboardTaps),
    /// `scrollEventThrottle`: minimum milliseconds between `onScroll`.
    EventThrottle(f32),
    /// `disableIntervalMomentum`: a fling moves one snap point at most.
    DisableIntervalMomentum(bool),
    /// `minimumZoomScale`.
    MinZoom(f32),
    /// `maximumZoomScale`.
    MaxZoom(f32),
    /// `zoomScale`: zooms there when it changes.
    Zoom(f32),
    /// `bouncesZoom`.
    BouncesZoom(bool),
    /// `pinchGestureEnabled`.
    Pinch(bool),
    /// `centerContent`.
    CenterContent(bool),
}

/// What a list cell holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellSlot {
    /// A recycled item cell in this slot.
    Item(u32),
    /// `ListHeaderComponent`.
    Header,
    /// `ListFooterComponent`.
    Footer,
    /// `ListEmptyComponent` (shown while there are no items).
    Empty,
}

/// A list's `contentContainerStyle`: padding around the content and gaps
/// between rows and columns.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ListContentStyle {
    /// Padding around the content.
    pub padding: EdgeInsets,
    /// Gap between rows (vertical lists) / columns (horizontal lists).
    pub row_gap: f32,
    /// Gap between columns (`numColumns`) of vertical lists.
    pub column_gap: f32,
}

/// When list items count as viewable (React Native's `viewabilityConfig`).
/// Without thresholds, any visible pixel does.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Viewability {
    /// Percent of the viewport an item must cover.
    pub view_area_percent: Option<f32>,
    /// Percent of an item that must be visible.
    pub item_visible_percent: Option<f32>,
}

/// Where a header item goes (`<headeritem slot>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeaderSlot {
    /// Leading, after the back button.
    #[default]
    Left,
    /// Trailing.
    Right,
    /// Replaces the title.
    Title,
}

/// How a screen enters and leaves (`animation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenAnimation {
    /// The platform's push / pop transition.
    #[default]
    Default,
    /// Cross-fade.
    Fade,
    /// No animation.
    None,
}

/// A screen option (`<screen title headerShown ...>`).
#[derive(Debug, Clone, PartialEq)]
pub enum ScreenProp {
    /// `title`.
    Title(Option<Arc<str>>),
    /// `headerShown`.
    HeaderShown(bool),
    /// `headerLargeTitle` (iOS).
    HeaderLargeTitle(bool),
    /// `headerTransparent`: content extends under the header.
    HeaderTransparent(bool),
    /// `headerTintColor`: back button and bar buttons.
    HeaderTintColor(Option<Color>),
    /// `headerBackgroundColor`.
    HeaderBackgroundColor(Option<Color>),
    /// `headerTitleColor`.
    HeaderTitleColor(Option<Color>),
    /// `headerBackTitle`: label of the back button leading back here.
    HeaderBackTitle(Option<Arc<str>>),
    /// `headerBackVisible`.
    HeaderBackVisible(bool),
    /// `gestureEnabled`: swipe back (iOS) / back button (Android) pops it.
    GestureEnabled(bool),
    /// `animation`.
    Animation(ScreenAnimation),
}

/// How a modal appears (`animationType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModalAnimation {
    /// Slides up from the bottom (default).
    #[default]
    Slide,
    /// Fades in.
    Fade,
    /// Appears at once.
    None,
}

/// How a modal covers the screen (`presentationStyle`, iOS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModalPresentation {
    /// Covers the whole screen (default).
    #[default]
    FullScreen,
    /// A sheet that leaves the presenting content visible behind it.
    PageSheet,
    /// A smaller centered sheet on large screens.
    FormSheet,
}

/// A modal option.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModalProp {
    /// `animationType`.
    Animation(ModalAnimation),
    /// `presentationStyle`.
    Presentation(ModalPresentation),
    /// `transparent`: what is behind shows through the modal's background.
    Transparent(bool),
    /// `statusBarTranslucent` (Android).
    StatusBarTranslucent(bool),
    /// `supportedOrientations` (iOS): [`crate::ModalConfig::PORTRAIT`] bits.
    Orientations(u8),
}

/// An imperative call on a native view (`ref.focus()` and the like).
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Focus an input (show the keyboard).
    Focus,
    /// Unfocus an input; on the root view, unfocus whatever is focused.
    Blur,
    /// Scroll a scroll view to an offset.
    ScrollTo {
        /// Horizontal offset.
        x: f32,
        /// Vertical offset.
        y: f32,
        /// Animate the change.
        animated: bool,
    },
    /// Animate the layout changes of the next commit (on the root).
    LayoutAnimation(LayoutAnimation),
    /// Leave the app (Android back handling; on the root).
    ExitApp,
    /// Reload a web view.
    Reload,
    /// Go back in a web view's history.
    GoBack,
    /// Go forward in a web view's history.
    GoForward,
    /// Stop loading a web view.
    StopLoading,
    /// Run JavaScript in a web view's page.
    InjectJavaScript(String),
    /// Send a message to a web view's page (`window.onmessage` data).
    PostMessage(String),
    /// Seek a video to a time in seconds.
    Seek(f64),
    /// Scroll a list to an item (resolved by the core into `ScrollTo`).
    ScrollToIndex {
        /// The item.
        index: u32,
        /// Animate the change.
        animated: bool,
        /// Where the item ends up: 0 at the start of the viewport, 1 at the
        /// end, 0.5 in the middle.
        view_position: f32,
        /// Extra distance from that position.
        view_offset: f32,
    },
    /// Zoom a zoomable scroll view to `scale`, with the content point
    /// (`x`, `y`) (points, unzoomed) in the middle of the viewport, or
    /// around the viewport's middle.
    ZoomTo {
        /// The zoom scale (clamped to the view's limits).
        scale: f32,
        /// The content point to center.
        center: Option<(f32, f32)>,
        /// Animate the change.
        animated: bool,
    },
    /// Scroll a list to the end of its content.
    ScrollToEnd {
        /// Animate the change.
        animated: bool,
    },
    /// Set the status bar (on the root).
    StatusBar(StatusBarConfig),
    /// Show an alert or action sheet (on the root); the answer comes back
    /// as [`EventKind::Dialog`].
    Dialog(Box<Dialog>),
    /// A JS responder took the touches on this view: `block` keeps native
    /// scroll views around it from taking them over.
    SetJsResponder {
        /// Block native scrolling.
        block: bool,
    },
    /// The JS responder let go (on the root).
    ClearJsResponder,
    /// Select a range of an input (UTF-16 units; equal for a caret).
    SetSelection {
        /// Start.
        start: u32,
        /// End.
        end: u32,
    },
    /// Show a short message (Android toast; on the root).
    Toast {
        /// The message.
        text: String,
        /// Show it longer.
        long: bool,
    },
    /// Have the screen reader read a message (on the root).
    Announce(String),
    /// Load the image at `uri` (keeping its bytes for views that show it
    /// next) and report its pixel size as [`EventKind::ImageInfo`] `id`
    /// (on the root).
    ImageInfo {
        /// The request.
        id: u32,
        /// URL or `asset:/` URI.
        uri: String,
    },
    /// Move the screen reader's focus to this view.
    AccessibilityFocus,
}

/// The children's opacity while an underlay shows, by default.
pub const UNDERLAY_OPACITY: f32 = 0.85;

/// The status bar's look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusBarStyle {
    /// The platform's default for the appearance.
    #[default]
    Default,
    /// Light content (for dark backgrounds).
    Light,
    /// Dark content (for light backgrounds).
    Dark,
}

/// The status bar (React Native's `StatusBar`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StatusBarConfig {
    /// Content color.
    pub style: StatusBarStyle,
    /// Hidden.
    pub hidden: bool,
    /// Animate the change.
    pub animated: bool,
    /// Background (Android before edge-to-edge only).
    pub background: Option<Color>,
}

/// How a dialog button looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogButtonStyle {
    /// A regular button.
    #[default]
    Default,
    /// The cancel button.
    Cancel,
    /// A destructive action.
    Destructive,
}

/// A dialog button.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogButton {
    /// Label.
    pub text: String,
    /// Look.
    pub style: DialogButtonStyle,
}

/// An alert, or with `sheet` an action sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct Dialog {
    /// Echoed in the answer.
    pub id: u32,
    /// Title.
    pub title: Option<String>,
    /// Message.
    pub message: Option<String>,
    /// Buttons, or the sheet's options.
    pub buttons: Vec<DialogButton>,
    /// Tapping outside (or the back button) dismisses it.
    pub cancelable: bool,
    /// An action sheet rather than an alert.
    pub sheet: bool,
}

/// A non-style property of a host node.
#[derive(Debug, Clone, PartialEq)]
pub enum Prop {
    /// `<image src>`.
    Source(Option<ImageSource>),
    /// `<image resizeMode>`.
    ResizeMode(ResizeMode),
    /// `<scroll horizontal>` / `<list horizontal>`.
    Horizontal(bool),
    /// Scrolling behavior of a scroll view or list.
    Scroll(ScrollProp),
    /// Whether the scroll indicator is shown.
    ShowsIndicator(bool),
    /// Pull-to-refresh spinner state.
    Refreshing(bool),
    /// Number of items of a list.
    Count(u32),
    /// Size used for list items not measured yet.
    EstimatedItemSize(f32),
    /// The recycling slot (or the role) of a list cell, set once by the
    /// runtime.
    Slot(CellSlot),
    /// Items per row of a list (`numColumns`).
    Columns(u32),
    /// The items of a list changed: `removed` at `at` became `inserted`
    /// (precedes the new `Count`).
    Splice(u32, u32, u32),
    /// A list laid out from the end (chat style).
    Inverted(bool),
    /// Items of a list that stick to its start while scrolled past.
    StickyIndices(Vec<u32>),
    /// A list's `contentContainerStyle`.
    ListContent(ListContentStyle),
    /// A list's `viewabilityConfig`.
    Viewability(Viewability),
    /// How close to the end `onEndReached` fires, in viewports.
    EndReachedThreshold(f32),
    /// The item index a list cell now renders (set by the runtime after
    /// each bind, in the same batch as the cell's new content).
    CellIndex(u32),
    /// Native animation value driven by the scroll offset (main axis).
    ScrollValue(Option<yf_anim::NodeId>),
    /// Native animation values driven by a drag (x, y).
    PanValues(Option<yf_anim::NodeId>, Option<yf_anim::NodeId>),
    /// `<text numberOfLines>`: 0 means no limit.
    NumberOfLines(u32),
    /// `<text ellipsizeMode>`.
    EllipsizeMode(EllipsizeMode),
    /// `<input value>`: the text, controlled by JS.
    Value(String),
    /// `<input defaultValue>`: the initial text.
    DefaultValue(String),
    /// `<input placeholder>`.
    Placeholder(String),
    /// `<input placeholderTextColor>`.
    PlaceholderColor(Option<Color>),
    /// `<input secureTextEntry>`.
    Secure(bool),
    /// `<input keyboardType>`.
    KeyboardType(KeyboardType),
    /// `<input returnKeyType>`.
    ReturnKeyType(ReturnKeyType),
    /// `<input autoCapitalize>`.
    AutoCapitalize(AutoCapitalize),
    /// `<input autoCorrect>`.
    AutoCorrect(bool),
    /// `<input multiline>`.
    Multiline(bool),
    /// `<input maxLength>`.
    MaxLength(Option<u32>),
    /// `<input editable>`.
    Editable(bool),
    /// `<input blurOnSubmit>`.
    BlurOnSubmit(bool),
    /// `<input autoFocus>`.
    AutoFocus(bool),
    /// `<input caretHidden>`.
    CaretHidden(bool),
    /// `<input inputAccessoryViewID>`: the `nativeID` of an input
    /// accessory.
    InputAccessoryViewId(Option<Box<str>>),
    /// `nativeID` of an input accessory.
    NativeId(Option<Box<str>>),
    /// `<input selectionColor>` (caret and selection).
    SelectionColor(Option<Color>),
    /// `<input autoComplete>` / `textContentType`.
    AutoFill(AutoFill),
    /// `<input selection>`: start and end (UTF-16 units).
    Selection(u32, u32),
    /// `<scroll keyboardDismissMode>` / `<List keyboardDismissMode>`.
    KeyboardDismissMode(KeyboardDismissMode),
    /// `pointerEvents`.
    PointerEvents(PointerEvents),
    /// `hitSlop`.
    HitSlop(EdgeInsets),
    /// `activeOpacity`: native press feedback.
    ActiveOpacity(Option<f32>),
    /// `underlayColor`: the background while pressed.
    UnderlayColor(Option<Color>),
    /// `underlayOpacity`: the children's opacity while the underlay shows.
    UnderlayOpacity(f32),
    /// The surface lays out right to left (on the root, from
    /// `I18nManager`).
    LayoutDirection(bool),
    /// Padding added to the style's own (safe areas, the keyboard), in
    /// points on the physical edges.
    InsetPadding(EdgeInsets),
    /// Margin added to the style's own (`KeyboardAvoidingView`'s
    /// `height` behavior), in points on the physical edges.
    InsetMargin(EdgeInsets),
    /// `class`: Tailwind classes.
    Class(Box<str>),
    /// `classList`: the names whose value is truthy.
    ClassList(Vec<Box<str>>),
    /// The app's Tailwind theme (JSON, on the root).
    TailwindTheme(Box<str>),
    /// An image option.
    Image(ImageProp),
    /// `delayLongPress`: milliseconds before a press becomes a long press.
    LongPressDelay(Option<f32>),
    /// `android_ripple`: Android's press ripple.
    Ripple(Option<crate::props::Ripple>),
    /// An accessibility property.
    Accessibility(AccessibilityProp),
    /// `testID`.
    TestId(Option<Arc<str>>),
    /// `<text allowFontScaling>`: follow the user's text size setting.
    AllowFontScaling(bool),
    /// `<text maxFontSizeMultiplier>`: 0 means no limit.
    MaxFontSizeMultiplier(f32),
    /// `<text selectable>`: a long press offers to copy the text.
    Selectable(bool),
    /// `adjustsFontSizeToFit`: text shrinks to fit its lines and box.
    AdjustsFontSizeToFit(bool),
    /// `minimumFontScale`: how far it shrinks at most (0..1).
    MinimumFontScale(f32),
    /// Native animation value driven by pinching the view (`pinch={{ scale }}`).
    PinchValue(Option<yf_anim::NodeId>),
    /// Native animation value driven by rotating the view with two fingers
    /// (`rotation={{ angle }}`).
    RotationValue(Option<yf_anim::NodeId>),
    /// A screen option.
    Screen(ScreenProp),
    /// A modal option.
    Modal(ModalProp),
    /// `<headeritem slot>`.
    HeaderSlot(HeaderSlot),
    /// A property of a native control (web view, video), or `disabled`.
    Native(NativeProp),
}

/// Where a web view loads from.
#[derive(Debug, Clone, PartialEq)]
pub enum WebSource {
    /// Nothing.
    None,
    /// A URL.
    Uri(Arc<str>),
    /// HTML, with the URL relative links resolve against.
    Html {
        /// The page.
        html: Arc<str>,
        /// Base URL.
        base_url: Option<Arc<str>>,
    },
}

/// A property of a native control.
#[derive(Debug, Clone, PartialEq)]
pub enum NativeProp {
    /// `disabled` (any element): no presses, `disabled:` classes.
    Disabled(bool),
    /// Web view / video `source`.
    Source(WebSource),
    /// Web view `injectedJavaScript` (after the page loads).
    InjectedJavaScript(Option<Arc<str>>),
    /// Web view `injectedJavaScriptBeforeContentLoaded`.
    InjectedJavaScriptBefore(Option<Arc<str>>),
    /// Web view `userAgent`.
    UserAgent(Option<Arc<str>>),
    /// Web view `scrollEnabled`.
    ScrollEnabled(bool),
    /// Web view `javaScriptEnabled`.
    JavaScriptEnabled(bool),
    /// Web view `allowsBackForwardNavigationGestures` (iOS).
    BackForwardGestures(bool),
    /// Video `paused`.
    Paused(bool),
    /// Video `muted`.
    Muted(bool),
    /// Video `volume` (0..1).
    Volume(f32),
    /// Video `rate`.
    Rate(f32),
    /// Video `repeat`.
    Repeat(bool),
    /// Video `resizeMode`.
    VideoResizeMode(ResizeMode),
    /// Video `controls`: the platform's playback controls.
    Controls(bool),
    /// Video `progressUpdateInterval` (ms).
    ProgressInterval(f32),
}

/// A mutation produced by the JS renderer.
#[derive(Debug)]
pub enum Op {
    /// What a canvas draws (replaces the previous drawing).
    Draw {
        /// The canvas.
        id: NodeId,
        /// The recorded operations.
        drawing: Box<crate::canvas::Drawing>,
    },
    /// Creates a detached element node.
    Create {
        /// New node id.
        id: NodeId,
        /// Element tag.
        tag: Tag,
    },
    /// Creates a detached raw text node.
    CreateText {
        /// New node id.
        id: NodeId,
        /// Text content.
        text: String,
    },
    /// Replaces the content of a raw text node.
    SetText {
        /// Text node id.
        id: NodeId,
        /// New content.
        text: String,
    },
    /// Replaces the whole style of a node.
    SetStyle {
        /// Node id.
        id: NodeId,
        /// The new style; boxed because layout styles are large.
        style: Box<Style>,
        /// The declarations `style` was built from, applied again over the
        /// node's class styles.
        patch: StylePatch,
    },
    /// Sets a non-style property.
    SetProp {
        /// Node id.
        id: NodeId,
        /// The property.
        prop: Prop,
    },
    /// Enables or disables an event listener on a node.
    Listen {
        /// Node id.
        id: NodeId,
        /// Event kind.
        event: EventKind,
        /// Whether a handler is attached.
        enabled: bool,
    },
    /// Inserts (or moves) `child` into `parent` before `before`, or appends.
    Insert {
        /// Parent node id.
        parent: NodeId,
        /// Child node id.
        child: NodeId,
        /// Sibling to insert before; `None` appends.
        before: Option<NodeId>,
    },
    /// Detaches `child` from `parent`. The child stays alive until released.
    Remove {
        /// Parent node id.
        parent: NodeId,
        /// Child node id.
        child: NodeId,
    },
    /// The JS node was garbage collected: free the node and its native view.
    Release {
        /// Node id.
        id: NodeId,
    },
    /// An animation graph change, forwarded to the platform in order.
    Anim(AnimOp),
    /// An imperative call on a node's native view.
    Command {
        /// Node id.
        id: NodeId,
        /// The call.
        command: Command,
    },
}
