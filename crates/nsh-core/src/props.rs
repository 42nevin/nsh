//! Non-style view properties: touch handling, accessibility, gesture values
//! and layout animations.

use std::sync::Arc;

/// Edge insets in points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EdgeInsets {
    /// Top.
    pub top: f32,
    /// Right.
    pub right: f32,
    /// Bottom.
    pub bottom: f32,
    /// Left.
    pub left: f32,
}

impl EdgeInsets {
    /// The same inset on every edge.
    pub const fn all(v: f32) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    /// True if every inset is zero.
    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }
}

/// Which touches a view and its children receive (`pointerEvents`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PointerEvents {
    /// The view and its children receive touches (default).
    #[default]
    Auto,
    /// Neither the view nor its children receive touches.
    None,
    /// Only the children receive touches; the view itself is transparent to them.
    BoxNone,
    /// Only the view receives touches, never its children.
    BoxOnly,
}

/// What an element is, for assistive technologies (`accessibilityRole`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccessibilityRole {
    /// No particular role.
    #[default]
    None,
    /// A button.
    Button,
    /// A link.
    Link,
    /// A search field.
    Search,
    /// An image.
    Image,
    /// An image that acts as a button.
    ImageButton,
    /// A key of a keyboard.
    KeyboardKey,
    /// Static text.
    Text,
    /// A value that can be adjusted (slider).
    Adjustable,
    /// A section header.
    Header,
    /// A summary of the app's state.
    Summary,
    /// An alert.
    Alert,
    /// A checkbox.
    Checkbox,
    /// A combo box.
    Combobox,
    /// A menu.
    Menu,
    /// A menu bar.
    Menubar,
    /// An item of a menu.
    Menuitem,
    /// A progress bar.
    Progressbar,
    /// A radio button.
    Radio,
    /// A group of radio buttons.
    Radiogroup,
    /// A scroll bar.
    Scrollbar,
    /// A spin button.
    Spinbutton,
    /// A switch.
    Switch,
    /// A tab.
    Tab,
    /// A list of tabs.
    Tablist,
    /// A timer.
    Timer,
    /// A toolbar.
    Toolbar,
    /// A list.
    List,
    /// A grid.
    Grid,
}

impl AccessibilityRole {
    /// Parses a React Native role name.
    pub fn parse(name: &str) -> Option<Self> {
        use AccessibilityRole::*;
        Some(match name {
            "none" => None,
            "button" => Button,
            "link" => Link,
            "search" => Search,
            "image" => Image,
            "imagebutton" => ImageButton,
            "keyboardkey" => KeyboardKey,
            "text" => Text,
            "adjustable" => Adjustable,
            "header" => Header,
            "summary" => Summary,
            "alert" => Alert,
            "checkbox" => Checkbox,
            "combobox" => Combobox,
            "menu" => Menu,
            "menubar" => Menubar,
            "menuitem" => Menuitem,
            "progressbar" => Progressbar,
            "radio" => Radio,
            "radiogroup" => Radiogroup,
            "scrollbar" => Scrollbar,
            "spinbutton" => Spinbutton,
            "switch" => Switch,
            "tab" => Tab,
            "tablist" => Tablist,
            "timer" => Timer,
            "toolbar" => Toolbar,
            "list" => List,
            "grid" => Grid,
            _ => return Option::None,
        })
    }
}

/// Whether a checkable element is checked (`accessibilityState.checked`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checked {
    /// Not checked.
    False,
    /// Checked.
    True,
    /// Partially checked.
    Mixed,
}

/// Dynamic state of an element (`accessibilityState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AccessibilityState {
    /// Disabled.
    pub disabled: bool,
    /// Selected.
    pub selected: bool,
    /// Checked state of checkable elements.
    pub checked: Option<Checked>,
    /// Busy (loading).
    pub busy: bool,
    /// Expanded or collapsed, for expandable elements.
    pub expanded: Option<bool>,
}

/// The value of a range or textual element (`accessibilityValue`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AccessibilityValue {
    /// Minimum of the range.
    pub min: Option<f32>,
    /// Maximum of the range.
    pub max: Option<f32>,
    /// Current value in the range.
    pub now: Option<f32>,
    /// A textual description of the value; wins over the range.
    pub text: Option<Arc<str>>,
}

/// How changes of an element are announced (Android `accessibilityLiveRegion`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LiveRegion {
    /// Not announced.
    #[default]
    None,
    /// Announced when the user is idle.
    Polite,
    /// Announced right away.
    Assertive,
}

/// A custom action offered to assistive technologies.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessibilityActionDef {
    /// Name reported back in `onAccessibilityAction` (`activate`,
    /// `increment`, `decrement` and `longpress` are standard actions).
    pub name: Arc<str>,
    /// Label shown to the user; required for custom actions.
    pub label: Option<Arc<str>>,
}

/// Accessibility of one view.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Accessibility {
    /// `accessible`: the view is one element for assistive technologies
    /// (its children are not exposed separately). `None` keeps the
    /// platform default: views with text or touch handlers are elements.
    pub accessible: Option<bool>,
    /// `accessibilityLabel`.
    pub label: Option<Arc<str>>,
    /// `accessibilityHint`.
    pub hint: Option<Arc<str>>,
    /// `accessibilityRole`.
    pub role: AccessibilityRole,
    /// `accessibilityState`.
    pub state: AccessibilityState,
    /// `accessibilityValue`.
    pub value: Option<AccessibilityValue>,
    /// `accessibilityElementsHidden` / `importantForAccessibility:
    /// "no-hide-descendants"`: hide the view and its subtree.
    pub hidden: bool,
    /// `accessibilityLiveRegion`.
    pub live_region: LiveRegion,
    /// `accessibilityActions`.
    pub actions: Vec<AccessibilityActionDef>,
}

impl Accessibility {
    /// True if nothing is set.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// One accessibility property, as set from JS.
#[derive(Debug, Clone, PartialEq)]
pub enum AccessibilityProp {
    /// `accessible`.
    Accessible(Option<bool>),
    /// `accessibilityLabel`.
    Label(Option<Arc<str>>),
    /// `accessibilityHint`.
    Hint(Option<Arc<str>>),
    /// `accessibilityRole`.
    Role(AccessibilityRole),
    /// `accessibilityState`.
    State(AccessibilityState),
    /// `accessibilityValue`.
    Value(Option<AccessibilityValue>),
    /// `accessibilityElementsHidden`.
    Hidden(bool),
    /// `importantForAccessibility`: `"auto"`, `"yes"`, `"no"` or
    /// `"no-hide-descendants"`.
    Important(Option<bool>, bool),
    /// `accessibilityLiveRegion`.
    LiveRegion(LiveRegion),
    /// `accessibilityActions`.
    Actions(Vec<AccessibilityActionDef>),
}

/// Android's press ripple (`android_ripple`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Ripple {
    /// Ripple color; the theme's when `None`.
    pub color: Option<crate::style::Color>,
    /// The ripple is not clipped to the view.
    pub borderless: bool,
    /// Radius in points; the platform's when `None`.
    pub radius: Option<f32>,
    /// Drawn over the view's content (else behind it).
    pub foreground: bool,
}

/// Non-style properties of a view that only the platform needs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewProps {
    /// Which touches the view and its subtree receive.
    pub pointer_events: PointerEvents,
    /// Extends the touch area beyond the view's bounds.
    pub hit_slop: EdgeInsets,
    /// Opacity while pressed (native press feedback, no JS round trip).
    pub active_opacity: Option<f32>,
    /// Milliseconds before a press becomes a long press (platform default
    /// when `None`).
    pub long_press_delay: Option<f32>,
    /// Ripple while pressed (Android).
    pub ripple: Option<Ripple>,
    /// Accessibility.
    pub accessibility: Accessibility,
    /// `testID`: an identifier for UI tests.
    pub test_id: Option<Arc<str>>,
}

/// Native animation values driven by gestures on a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GestureValues {
    /// Horizontal drag translation (`pan={{ x }}`).
    pub pan_x: Option<nsh_anim::NodeId>,
    /// Vertical drag translation (`pan={{ y }}`).
    pub pan_y: Option<nsh_anim::NodeId>,
    /// Pinch scale (`pinch={{ scale }}`), multiplied from its value when the
    /// pinch starts.
    pub pinch: Option<nsh_anim::NodeId>,
    /// Two-finger rotation in degrees (`rotation={{ angle }}`), added to its
    /// value when the rotation starts.
    pub rotation: Option<nsh_anim::NodeId>,
}

impl GestureValues {
    /// True if no value is driven.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Timing curve of a layout animation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutCurve {
    /// Constant speed.
    Linear,
    /// Accelerates.
    EaseIn,
    /// Decelerates.
    EaseOut,
    /// Accelerates, then decelerates.
    EaseInEaseOut,
    /// A spring with the given damping ratio (0..1).
    Spring {
        /// Damping ratio: 1 does not overshoot.
        damping: f32,
    },
}

/// The property animated when views appear or disappear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutProperty {
    /// Fade in / out.
    Opacity,
    /// Grow / shrink from the center.
    Scale,
}

/// How one kind of change animates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutTransition {
    /// Curve.
    pub curve: LayoutCurve,
    /// Animated property of appearing / disappearing views.
    pub property: LayoutProperty,
    /// Duration in milliseconds.
    pub duration_ms: f32,
    /// Delay in milliseconds.
    pub delay_ms: f32,
}

/// Animates the layout changes of the next commit
/// (`LayoutAnimation.configureNext`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutAnimation {
    /// Views inserted by the commit.
    pub create: Option<LayoutTransition>,
    /// Views whose frame changes.
    pub update: Option<LayoutTransition>,
    /// Views removed by the commit.
    pub delete: Option<LayoutTransition>,
}
