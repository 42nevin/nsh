// Types of yf apps: styles, the props of host elements and the JSX
// namespace. Types only: nothing here reaches a bundle.
//
// Keep in sync with what the core accepts: `crates/yf-core/src/style.rs`
// (styles) and `crates/yf-js/src/protocol.rs` (props and events).

import type { NativeNode, NativeValue, YNode } from "./index";

/** A CSS-like color: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`,
 * `rgba()` or a named color (`black`, `white`, `transparent`, …). */
export type ColorValue = string;

/** Points, a percentage of the parent, or `"auto"`. */
export type DimensionValue = number | `${number}%` | "auto";

/** Points or a percentage of the parent. */
export type LengthValue = number | `${number}%`;

/** One transform step; angles are degrees (`"45deg"`, `"0.5rad"` or a
 * number). Native values (and nodes derived from them) animate a step
 * natively. */
export type TransformStep =
  | { translateX: number | NativeNode }
  | { translateY: number | NativeNode }
  | { scale: number | NativeNode }
  | { scaleX: number | NativeNode }
  | { scaleY: number | NativeNode }
  | { rotate: number | string | NativeNode }
  | { rotateZ: number | string | NativeNode }
  | { rotateX: number | string | NativeNode }
  | { rotateY: number | string | NativeNode }
  | { skewX: number | string }
  | { skewY: number | string }
  | { perspective: number };

type Align = "flex-start" | "flex-end" | "center" | "stretch" | "baseline";
type Distribute = "flex-start" | "flex-end" | "center" | "stretch" | "space-between" | "space-around" | "space-evenly";

/** Layout (flexbox with React Native's defaults: `column`, no shrinking). */
export interface LayoutStyle {
  display?: "flex" | "none";
  position?: "relative" | "absolute";
  flexDirection?: "row" | "column" | "row-reverse" | "column-reverse";
  flexWrap?: "wrap" | "nowrap" | "wrap-reverse";
  flex?: number;
  flexGrow?: number;
  flexShrink?: number;
  flexBasis?: DimensionValue;
  justifyContent?: Exclude<Distribute, "stretch"> | "stretch";
  alignItems?: Align;
  alignSelf?: Align | "auto";
  alignContent?: Distribute;
  width?: DimensionValue;
  height?: DimensionValue;
  minWidth?: LengthValue;
  minHeight?: LengthValue;
  maxWidth?: LengthValue;
  maxHeight?: LengthValue;
  aspectRatio?: number;
  top?: DimensionValue;
  right?: DimensionValue;
  bottom?: DimensionValue;
  left?: DimensionValue;
  margin?: DimensionValue;
  marginHorizontal?: DimensionValue;
  marginVertical?: DimensionValue;
  marginTop?: DimensionValue;
  marginRight?: DimensionValue;
  marginBottom?: DimensionValue;
  marginLeft?: DimensionValue;
  padding?: LengthValue;
  paddingHorizontal?: LengthValue;
  paddingVertical?: LengthValue;
  paddingTop?: LengthValue;
  paddingRight?: LengthValue;
  paddingBottom?: LengthValue;
  paddingLeft?: LengthValue;
  gap?: LengthValue;
  rowGap?: LengthValue;
  columnGap?: LengthValue;
  /** Order among siblings (stable); also the native stacking order. */
  zIndex?: number;
  /** Layout direction; inherited by default. Right to left, `left` and `right` properties mean start and end (as in React Native). */
  direction?: "inherit" | "ltr" | "rtl";
  /** Left in left-to-right layouts, right in right-to-left ones. */
  start?: DimensionValue;
  end?: DimensionValue;
  marginStart?: DimensionValue;
  marginEnd?: DimensionValue;
  paddingStart?: LengthValue;
  paddingEnd?: LengthValue;
}

/** Styles of views (and of text, images and inputs, which are views). */
export interface ViewStyle extends LayoutStyle {
  backgroundColor?: ColorValue | NativeNode;
  opacity?: number | NativeNode;
  transform?: TransformStep[];
  overflow?: "visible" | "hidden";
  borderWidth?: number;
  borderTopWidth?: number;
  borderRightWidth?: number;
  borderBottomWidth?: number;
  borderLeftWidth?: number;
  borderColor?: ColorValue;
  borderTopColor?: ColorValue;
  borderRightColor?: ColorValue;
  borderBottomColor?: ColorValue;
  borderLeftColor?: ColorValue;
  borderStartWidth?: number;
  borderEndWidth?: number;
  borderStartColor?: ColorValue;
  borderEndColor?: ColorValue;
  borderStyle?: "solid" | "dashed" | "dotted";
  borderRadius?: number;
  borderTopLeftRadius?: number;
  borderTopRightRadius?: number;
  borderBottomRightRadius?: number;
  borderBottomLeftRadius?: number;
  borderTopStartRadius?: number;
  borderTopEndRadius?: number;
  borderBottomStartRadius?: number;
  borderBottomEndRadius?: number;
  /** The point `transform` acts around: keywords, percentages and lengths ("top left", "50% 0", `[10, 20]`). Default the center. */
  transformOrigin?: string | (number | string)[];
  /** `"hidden"`: invisible while turned away (3D rotations). */
  backfaceVisibility?: "visible" | "hidden";
  /** Android's material shadow (ignored on iOS, as in React Native). */
  elevation?: number;
  shadowColor?: ColorValue;
  /** Default `{ width: 0, height: -3 }`, as in React Native. */
  shadowOffset?: { width: number; height: number };
  shadowOpacity?: number;
  shadowRadius?: number;
}

/** Styles of text (inherited by nested `<text>` spans) and inputs. */
export interface TextStyle extends ViewStyle {
  color?: ColorValue;
  fontSize?: number;
  /** `"normal"` (400), `"bold"` (700) or 100–900. */
  fontWeight?: "normal" | "bold" | number | `${number}`;
  fontStyle?: "normal" | "italic";
  /** A system family or the name an imported font (`import Inter from "./Inter.ttf"`) evaluates to. */
  fontFamily?: string;
  lineHeight?: number;
  letterSpacing?: number;
  textAlign?: "auto" | "left" | "center" | "right" | "justify";
  textDecorationLine?: "none" | "underline" | "line-through" | "underline line-through";
  textDecorationColor?: ColorValue;
  textTransform?: "none" | "uppercase" | "lowercase" | "capitalize";
  textShadowColor?: ColorValue;
  textShadowOffset?: { width: number; height: number };
  textShadowRadius?: number;
  /** OpenType number and letter forms. */
  fontVariant?: ("small-caps" | "oldstyle-nums" | "lining-nums" | "tabular-nums" | "proportional-nums")[];
  /** Base direction of the text; `auto` takes it from the content. */
  writingDirection?: "auto" | "ltr" | "rtl";
}

/** Styles of images. */
export interface ImageStyle extends ViewStyle {
  /** Recolors a template image. */
  tintColor?: ColorValue;
}

/** A style, or a list of styles merged left to right (falsy entries are
 * skipped). */
export type StyleProp<T> = T | readonly (T | false | null | undefined)[] | null | undefined;

export interface LayoutEvent {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface PanEvent {
  translationX: number;
  translationY: number;
  velocityX: number;
  velocityY: number;
  /** The `pan.x` value now (with one). */
  valueX?: number;
  /** The `pan.y` value now (with one). */
  valueY?: number;
}

export type AccessibilityRole =
  | "none" | "button" | "link" | "search" | "image" | "imagebutton" | "keyboardkey" | "text" | "adjustable"
  | "header" | "summary" | "alert" | "checkbox" | "combobox" | "menu" | "menubar" | "menuitem" | "progressbar"
  | "radio" | "radiogroup" | "scrollbar" | "spinbutton" | "switch" | "tab" | "tablist" | "timer" | "toolbar"
  | "list" | "grid";

export interface AccessibilityState {
  disabled?: boolean;
  selected?: boolean;
  checked?: boolean | "mixed";
  busy?: boolean;
  expanded?: boolean;
}

/** A finger of a touch event (points). */
export interface TouchPoint {
  /** Identifies the finger while it is down. */
  identifier: number;
  /** Position in the view the finger went down on (`target`). */
  locationX: number;
  locationY: number;
  /** Position in the surface. */
  pageX: number;
  pageY: number;
  /** Id of the view the finger went down on. */
  target: number;
  /** Milliseconds. */
  timestamp: number;
}

/** A touch or responder event: the finger it is about, the fingers down
 * and those that changed (React Native's `nativeEvent`, which it also is). */
export interface GestureResponderEvent extends TouchPoint {
  touches: TouchPoint[];
  changedTouches: TouchPoint[];
  nativeEvent: GestureResponderEvent;
  /** Touch handlers (`onTouchStart`…): views further out do not see it. */
  stopPropagation(): void;
}

/** Raw touches and the gesture responder system (React Native semantics). */
export interface TouchProps {
  onTouchStart?: (e: GestureResponderEvent) => void;
  onTouchMove?: (e: GestureResponderEvent) => void;
  onTouchEnd?: (e: GestureResponderEvent) => void;
  onTouchCancel?: (e: GestureResponderEvent) => void;
  onTouchStartCapture?: (e: GestureResponderEvent) => void;
  onTouchMoveCapture?: (e: GestureResponderEvent) => void;
  onTouchEndCapture?: (e: GestureResponderEvent) => void;
  onTouchCancelCapture?: (e: GestureResponderEvent) => void;
  /** Become the responder when a finger goes down (asked from the target outwards). */
  onStartShouldSetResponder?: (e: GestureResponderEvent) => boolean;
  /** Become the responder when a finger moves. */
  onMoveShouldSetResponder?: (e: GestureResponderEvent) => boolean;
  /** As above, asked from the outside in, before the bubbling ones. */
  onStartShouldSetResponderCapture?: (e: GestureResponderEvent) => boolean;
  onMoveShouldSetResponderCapture?: (e: GestureResponderEvent) => boolean;
  /** This view is the responder now. Return true to keep native scroll views from taking the touches. */
  onResponderGrant?: (e: GestureResponderEvent) => boolean | void;
  /** Another view is the responder and would not let go. */
  onResponderReject?: (e: GestureResponderEvent) => void;
  onResponderStart?: (e: GestureResponderEvent) => void;
  onResponderMove?: (e: GestureResponderEvent) => void;
  onResponderEnd?: (e: GestureResponderEvent) => void;
  /** The last finger lifted. */
  onResponderRelease?: (e: GestureResponderEvent) => void;
  /** Another view (or a native gesture) took the touches. */
  onResponderTerminate?: (e: GestureResponderEvent) => void;
  /** Another view wants the touches: return false to keep them (default true). */
  onResponderTerminationRequest?: (e: GestureResponderEvent) => boolean;
}

export type AutoComplete =
  | "off"
  | "username"
  | "password"
  | "current-password"
  | "new-password"
  | "one-time-code"
  | "sms-otp"
  | "email"
  | "tel"
  | "name"
  | "given-name"
  | "family-name"
  | "additional-name"
  | "nickname"
  | "organization"
  | "street-address"
  | "address-line1"
  | "address-line2"
  | "address-level1"
  | "address-level2"
  | "postal-code"
  | "country"
  | "cc-number"
  | "cc-exp"
  | "cc-csc"
  | "cc-name"
  | "url"
  | "birthdate-full";

export type TextContentType =
  | "none"
  | "username"
  | "password"
  | "newPassword"
  | "oneTimeCode"
  | "emailAddress"
  | "telephoneNumber"
  | "name"
  | "givenName"
  | "middleName"
  | "familyName"
  | "nickname"
  | "organizationName"
  | "fullStreetAddress"
  | "streetAddressLine1"
  | "streetAddressLine2"
  | "addressCity"
  | "addressState"
  | "postalCode"
  | "countryName"
  | "creditCardNumber"
  | "creditCardExpiration"
  | "creditCardSecurityCode"
  | "creditCardName"
  | "URL"
  | "birthdate";

/** Props every host element accepts. */
export interface CommonProps extends TouchProps {
  ref?: YNode | ((node: YNode) => void);
  children?: JSX.Element;
  /**
   * Tailwind CSS classes (`"flex-row gap-2 p-4 bg-white dark:bg-zinc-900"`),
   * resolved natively. Later classes win over earlier ones; `style` wins
   * over both.
   */
  class?: string;
  /** Classes applied while their value is truthy (`{ "opacity-50": busy() }`). */
  classList?: Record<string, boolean | null | undefined>;
  /** Ignores presses (no press feedback either), applies `disabled:` classes and disables controls. */
  disabled?: boolean;
  /** Touch handling of the view and its children (React Native semantics). */
  pointerEvents?: "auto" | "none" | "box-none" | "box-only";
  /** Extends the touch area beyond the bounds. */
  hitSlop?: number | { top?: number; right?: number; bottom?: number; left?: number };
  /** Opacity while pressed (native, no JS round trip). */
  activeOpacity?: number;
  /** The background while pressed (`TouchableHighlight`; native, no JS round trip). */
  underlayColor?: ColorValue;
  /** The children's opacity while `underlayColor` shows (default 0.85). */
  underlayOpacity?: number;
  /** Milliseconds before a press becomes a long press (default 500). */
  delayLongPress?: number;
  /** Android's ripple while pressed (ignored elsewhere). */
  android_ripple?: { color?: ColorValue; borderless?: boolean; radius?: number; foreground?: boolean };
  testID?: string;
  onPress?: () => void;
  onLongPress?: () => void;
  onPressIn?: () => void;
  onPressOut?: () => void;
  onLayout?: (e: LayoutEvent) => void;
  /** Drags drive these native values (points) on the UI thread. */
  pan?: { x?: NativeValue; y?: NativeValue };
  onPanStart?: (e: PanEvent) => void;
  /** With the release velocity, e.g. to start a `decay`. */
  onPanEnd?: (e: PanEvent) => void;
  /** Two-finger pinches drive `scale` natively. */
  pinch?: { scale?: NativeValue };
  onPinchStart?: (e: { scale: number; velocity: number }) => void;
  onPinchEnd?: (e: { scale: number; velocity: number }) => void;
  /** Two-finger rotations drive `angle` (degrees) natively. */
  rotation?: { angle?: NativeValue };
  onRotationStart?: (e: { rotation: number; velocity: number }) => void;
  onRotationEnd?: (e: { rotation: number; velocity: number }) => void;
  accessible?: boolean;
  accessibilityLabel?: string;
  accessibilityHint?: string;
  accessibilityRole?: AccessibilityRole;
  role?: AccessibilityRole;
  accessibilityState?: AccessibilityState;
  accessibilityValue?: { min?: number; max?: number; now?: number; text?: string };
  accessibilityElementsHidden?: boolean;
  importantForAccessibility?: "auto" | "yes" | "no" | "no-hide-descendants";
  accessibilityLiveRegion?: "none" | "polite" | "assertive";
  accessibilityActions?: { name: string; label?: string }[];
  onAccessibilityAction?: (e: { actionName: string }) => void;
}

/** `<view>` */
export interface ViewProps extends CommonProps {
  style?: StyleProp<ViewStyle>;
}

/** `<text>`: nested `<text>` elements are spans of the same paragraph. */
export interface TextProps extends CommonProps {
  style?: StyleProp<TextStyle>;
  /** 0 (default) is unlimited. */
  numberOfLines?: number;
  ellipsizeMode?: "tail" | "head" | "middle" | "clip";
  /** Follow the system text size (default true). */
  allowFontScaling?: boolean;
  /** Upper bound of the system scale (0 is none). */
  maxFontSizeMultiplier?: number;
}

/** An image asset (`import logo from "./logo.png"`), sized in points. */
export interface ImageAsset {
  uri: string;
  width: number;
  height: number;
}

/** `<image>` */
export interface ImageProps extends CommonProps {
  style?: StyleProp<ImageStyle>;
  /** A URL, `asset:/path`, or an imported asset. */
  src?: string | ImageAsset | { uri: string };
  resizeMode?: "cover" | "contain" | "stretch" | "center";
  /** Shown until `src` has loaded (best a local asset). */
  defaultSource?: string | ImageAsset | { uri: string };
  /** Blurs the image by this radius in points (decoded once, off the main thread). */
  blurRadius?: number;
  /** How long a loaded image fades in, in milliseconds (default 0 on iOS, 300 on Android, like React Native). */
  fadeDuration?: number;
  onLoadStart?: () => void;
  /** The source image's size in pixels (not the decoded, downsampled one). */
  onLoad?: (e: { width: number; height: number }) => void;
  onError?: (e: { message: string }) => void;
  /** After `onLoad` or `onError`. */
  onLoadEnd?: () => void;
}

/** Where a scroll view is: the payload of `onScroll`. */
export interface ScrollEvent {
  x: number;
  y: number;
  contentWidth: number;
  contentHeight: number;
  width: number;
  height: number;
}

/** How scroll views and lists scroll (React Native's ScrollView props). */
export interface ScrollBehaviorProps {
  /** Stops at multiples of the viewport. */
  pagingEnabled?: boolean;
  /** Stops at multiples of this length. */
  snapToInterval?: number;
  /** Stops at these offsets. */
  snapToOffsets?: number[];
  /** Which point of an item aligns with the viewport with `snapToInterval`. */
  snapToAlignment?: "start" | "center" | "end";
  /** A fling moves one snap point at most. */
  disableIntervalMomentum?: boolean;
  /** How fast a fling slows down: "normal", "fast", or the velocity kept per millisecond (0.998 / 0.99). */
  decelerationRate?: "normal" | "fast" | number;
  /** The user can scroll it (default true). */
  scrollEnabled?: boolean;
  /** Overscroll past the edges (default true). */
  bounces?: boolean;
  /** Room around the content. */
  contentInset?: { top?: number; right?: number; bottom?: number; left?: number };
  /** Where it starts. */
  contentOffset?: { x?: number; y?: number };
  /** Whether a tap dismisses the keyboard: "never" (default: the tap only dismisses it), "always" (never dismisses), "handled" (unless something handles the tap). */
  keyboardShouldPersistTaps?: "never" | "always" | "handled" | boolean;
  /** Minimum milliseconds between `onScroll` events (at most one per frame anyway). */
  scrollEventThrottle?: number;
  onScroll?: (e: ScrollEvent) => void;
  onScrollBeginDrag?: (e: { x: number; y: number }) => void;
  onScrollEndDrag?: (e: { x: number; y: number }) => void;
  onMomentumScrollBegin?: (e: { x: number; y: number }) => void;
  onMomentumScrollEnd?: (e: { x: number; y: number }) => void;
}

/** `<scroll>` */
export interface ScrollProps extends CommonProps, ScrollBehaviorProps {
  style?: StyleProp<ViewStyle>;
  horizontal?: boolean;
  showsIndicator?: boolean;
  refreshing?: boolean;
  onRefresh?: () => void;
  onScrollEnd?: (e: { x: number; y: number }) => void;
  /** Fed with the scroll offset natively, for scroll-linked effects. */
  scrollValue?: NativeValue;
  keyboardDismissMode?: "none" | "on-drag" | "interactive";
}

/** `<input>` */
export interface InputProps extends CommonProps {
  style?: StyleProp<TextStyle>;
  /** Controlled text. */
  value?: string;
  defaultValue?: string;
  onChangeText?: (text: string) => void;
  onSubmitEditing?: (e: { text: string }) => void;
  /** Return unfocuses a single-line input (default true); false keeps the keyboard up, e.g. to send several messages. */
  blurOnSubmit?: boolean;
  onFocus?: () => void;
  onBlur?: () => void;
  placeholder?: string;
  placeholderTextColor?: ColorValue;
  selectionColor?: ColorValue;
  /** Controlled selection (UTF-16 units; `end` defaults to `start`, a caret). */
  selection?: { start: number; end?: number };
  onSelectionChange?: (e: { start: number; end: number }) => void;
  /** Before the text changes: `key` is the typed text, `Backspace` or `Enter`. */
  onKeyPress?: (e: { key: string }) => void;
  /** iOS: the `nativeID` of an `InputAccessoryView` shown over the keyboard while this input has focus. */
  inputAccessoryViewID?: string;
  /** What the input holds, for autofill (React Native's values, e.g. "username", "current-password", "new-password", "one-time-code", "email", "tel", "postal-code", "cc-number", "off"). */
  autoComplete?: AutoComplete;
  /** iOS names of the same (`autoComplete` covers both platforms). */
  textContentType?: TextContentType;
  secureTextEntry?: boolean;
  keyboardType?: "default" | "number-pad" | "numeric" | "decimal-pad" | "email-address" | "phone-pad" | "url";
  returnKeyType?: "default" | "done" | "go" | "next" | "search" | "send";
  autoCapitalize?: "none" | "sentences" | "words" | "characters";
  autoCorrect?: boolean;
  multiline?: boolean;
  maxLength?: number;
  editable?: boolean;
  autoFocus?: boolean;
  allowFontScaling?: boolean;
  maxFontSizeMultiplier?: number;
}

/** Host elements without components of their own take any prop the core
 * knows; their typed forms are the runtime's components (`WebView`, …). */
interface ControlProps extends CommonProps {
  style?: StyleProp<ViewStyle>;
  [prop: string]: any;
}

export declare namespace JSX {
  /** What components return and children accept. */
  type Element = YNode | ArrayElement | (() => Element) | string | number | boolean | null | undefined;
  interface ArrayElement extends Array<Element> {}
  interface ElementChildrenAttribute {
    children: {};
  }
  interface IntrinsicElements {
    view: ViewProps;
    text: TextProps;
    image: ImageProps;
    scroll: ScrollProps;
    input: InputProps;
    inputaccessory: ControlProps;
    webview: ControlProps;
    video: ControlProps;
    stack: ControlProps;
    screen: ControlProps;
    modal: ControlProps;
    headeritem: ControlProps;
  }
  // Used by solid-js's own types.
  interface HTMLAttributes<T> {}
}

/** The state of a pan gesture (React Native's `PanResponderGestureState`). */
export interface PanResponderGestureState {
  stateID: number;
  /** Latest position of the touches' centroid (surface points). */
  moveX: number;
  moveY: number;
  /** Where the centroid was when the responder was granted. */
  x0: number;
  y0: number;
  /** Distance moved since the grant. */
  dx: number;
  dy: number;
  /** Velocity (points per millisecond). */
  vx: number;
  vy: number;
  numberActiveTouches: number;
}

type PanHandler<R = void> = (e: GestureResponderEvent, gestureState: PanResponderGestureState) => R;

/** `PanResponder.create` callbacks. */
export interface PanResponderCallbacks {
  onStartShouldSetPanResponder?: PanHandler<boolean>;
  onStartShouldSetPanResponderCapture?: PanHandler<boolean>;
  onMoveShouldSetPanResponder?: PanHandler<boolean>;
  onMoveShouldSetPanResponderCapture?: PanHandler<boolean>;
  onPanResponderGrant?: PanHandler;
  onPanResponderReject?: PanHandler;
  onPanResponderStart?: PanHandler;
  onPanResponderMove?: PanHandler;
  onPanResponderEnd?: PanHandler;
  onPanResponderRelease?: PanHandler;
  onPanResponderTerminate?: PanHandler;
  onPanResponderTerminationRequest?: PanHandler<boolean>;
  /** Keep native scroll views from taking the touches (default true). */
  onShouldBlockNativeResponder?: PanHandler<boolean>;
}

/** What `PanResponder.create` returns: spread `panHandlers` on a view. */
export interface PanResponderInstance {
  panHandlers: TouchProps;
}
