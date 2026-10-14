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
  borderStyle?: "solid" | "dashed" | "dotted";
  borderRadius?: number;
  borderTopLeftRadius?: number;
  borderTopRightRadius?: number;
  borderBottomRightRadius?: number;
  borderBottomLeftRadius?: number;
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

/** Props every host element accepts. */
export interface CommonProps {
  ref?: YNode | ((node: YNode) => void);
  children?: JSX.Element;
  /** Touch handling of the view and its children (React Native semantics). */
  pointerEvents?: "auto" | "none" | "box-none" | "box-only";
  /** Extends the touch area beyond the bounds. */
  hitSlop?: number | { top?: number; right?: number; bottom?: number; left?: number };
  /** Opacity while pressed (native, no JS round trip). */
  activeOpacity?: number;
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
  onLoad?: (e: { width: number; height: number }) => void;
  onError?: (e: { message: string }) => void;
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
 * knows; their typed forms are the runtime's components (`Switch`, …). */
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
    switch: ControlProps;
    slider: ControlProps;
    activityindicator: ControlProps;
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
