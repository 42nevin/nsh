// yf runtime: the Solid universal renderer that turns JSX into a flat op
// buffer for the Rust core.
//
// Performance rules (QuickJS is a plain interpreter, the slowest part of the
// pipeline): do the minimum here. No parsing, no diffing, no string building.
// Push raw values into one reused array and let Rust decode them.
//
// The opcode values must match `crates/yf-js/src/protocol.rs`.

import {
  type Accessor,
  batch,
  children as resolveChildren,
  createContext,
  createMemo,
  createRenderEffect,
  createRoot,
  createSignal,
  getOwner,
  mapArray,
  mergeProps as mergeSolidProps,
  onCleanup,
  splitProps,
  untrack,
  useContext,
} from "solid-js";
import { createRenderer } from "solid-js/universal";
// The app's Tailwind theme (`yf.json`), JSON or null.
import tailwindTheme from "yf:tailwind-theme";
import type {
  GestureResponderEvent,
  JSX,
  PanResponderCallbacks,
  PanResponderGestureState,
  PanResponderInstance,
  ScrollBehaviorProps,
  StyleProp,
  TouchPoint,
  ColorValue,
  ImageAsset,
  LayoutEvent,
  ViewProps,
  ViewStyle,
} from "./types";

export type * from "./types";

const OP_CREATE = 0; // id, tag
const OP_CREATE_TEXT = 1; // id, text
const OP_SET_TEXT = 2; // id, text
const OP_SET_PROP = 3; // id, name, value
const OP_INSERT = 4; // parent, child, before (0 = append)
const OP_REMOVE = 5; // parent, child
const OP_RELEASE = 6; // id
const OP_ANIM = 7; // sub-op, node, args...
const OP_COMMAND = 8; // id, name, argument
const OP_DRAW = 9; // id, [canvas operations (CV_*)]

// Sub-ops of OP_ANIM.
const ANIM_VALUE = 0; // node, value
const ANIM_INTERPOLATE = 1; // node, input, inputRange, outputRange, extrapolateLeft, extrapolateRight, easing
const ANIM_MATH = 2; // node, op, a, b
const ANIM_SET = 3; // node, value
const ANIM_START = 4; // node, animation id, config
const ANIM_STOP = 5; // node
const ANIM_RELEASE = 6; // node
const ANIM_DIFF_CLAMP = 7; // node, input, min, max
const ANIM_START_COMPOSITE = 8; // animation id, composite key, spec
const ANIM_STOP_ANIMATION = 9; // animation id
const ANIM_RESET_COMPOSITE = 10; // composite key
const ANIM_RELEASE_COMPOSITE = 11; // composite key

declare function __yf_commit(ops: unknown[]): void;

type Handler = (payload: unknown) => void;

/** A host node. Only the tree links needed by Solid are kept. */
export class YNode {
  readonly id: number;
  parent: YNode | null = null;
  first: YNode | null = null;
  last: YNode | null = null;
  prev: YNode | null = null;
  next: YNode | null = null;
  handlers: Record<string, Handler> | null = null;
  /** Native animation nodes used by this view's props, by prop name: kept
   * alive with the view. */
  anims: Record<string, NativeNode[]> | null = null;
  /** Screens: called when the user popped the screen natively. */
  popped: (() => void) | null = null;
  readonly isText: boolean;
  /** Element tag, for nodes the runtime creates itself (`"screen"`). */
  tag: string | null = null;

  constructor(id: number, isText: boolean) {
    this.id = id;
    this.isText = isText;
  }

  /** Focuses an `<input>` (shows the keyboard). */
  focus(): void {
    command(this.id, "focus");
  }

  /** Inputs: selects `start..end` (UTF-16 units; a caret without `end`). */
  setSelection(start: number, end: number = start): void {
    command(this.id, "setSelection", { start, end });
  }

  /** Unfocuses an `<input>`. */
  blur(): void {
    command(this.id, "blur");
  }

  /** Scrolls a `<scroll>` or `List` to an offset. */
  scrollTo(options: { x?: number; y?: number; animated?: boolean }): void {
    command(this.id, "scrollTo", options);
  }

  /**
   * A zoomable `<scroll>` (`maximumZoomScale` above `minimumZoomScale`):
   * zooms to `scale` (clamped to the limits), with the content point
   * (`x`, `y`) (unzoomed points) in the middle of the viewport, or around
   * the middle. E.g. double tap to zoom in on the tapped point.
   */
  zoomTo(options: { scale: number; x?: number; y?: number; animated?: boolean }): void {
    command(this.id, "zoomTo", options);
  }

  /** `List`: scrolls to a content offset along the list's axis. */
  scrollToOffset(options: { offset: number; animated?: boolean }): void {
    command(this.id, "scrollTo", { x: options.offset, y: options.offset, animated: options.animated });
  }

  /**
   * `List`: scrolls item `index` into view. `viewPosition` places it: 0 at
   * the start of the viewport (default), 0.5 in the middle, 1 at the end;
   * `viewOffset` shifts it further. Resolved natively with the current
   * (measured or estimated) item sizes.
   */
  scrollToIndex(options: { index: number; animated?: boolean; viewPosition?: number; viewOffset?: number }): void {
    command(this.id, "scrollToIndex", options);
  }

  /** `List`: scrolls to the end of the content (the start of an inverted list). */
  scrollToEnd(options?: { animated?: boolean }): void {
    command(this.id, "scrollToEnd", options ?? {});
  }

  /** `WebView`: reloads the page. */
  reload(): void {
    command(this.id, "reload");
  }

  /** `WebView`: goes back in its history. */
  goBack(): void {
    command(this.id, "goBack");
  }

  /** `WebView`: goes forward in its history. */
  goForward(): void {
    command(this.id, "goForward");
  }

  /** `WebView`: stops loading. */
  stopLoading(): void {
    command(this.id, "stopLoading");
  }

  /** `WebView`: runs JavaScript in the page. */
  injectJavaScript(script: string): void {
    command(this.id, "injectJavaScript", script);
  }

  /** `WebView`: sends `data` to the page (a `message` event on `window`). */
  postMessage(data: string): void {
    command(this.id, "postMessage", data);
  }

  /** `Video`: jumps to `seconds`. */
  seek(seconds: number): void {
    command(this.id, "seek", seconds);
  }
}

function detachNode(parent: YNode, node: YNode) {
  unlink(node);
  ops.push(OP_REMOVE, parent.id, node.id);
  schedule();
}

function command(id: number, name: string, argument?: unknown) {
  ops.push(OP_COMMAND, id, name, argument);
  schedule();
}

// --- Layout animations ----------------------------------------------------

export type LayoutAnimationType = "spring" | "linear" | "easeInEaseOut" | "easeIn" | "easeOut";
export type LayoutAnimationProperty = "opacity" | "scaleXY";

export interface LayoutAnimationAnim {
  type?: LayoutAnimationType;
  property?: LayoutAnimationProperty;
  duration?: number;
  delay?: number;
  springDamping?: number;
}

export interface LayoutAnimationConfig {
  duration: number;
  create?: LayoutAnimationAnim;
  update?: LayoutAnimationAnim;
  delete?: LayoutAnimationAnim;
}

function layoutPreset(
  duration: number,
  type: LayoutAnimationType = "easeInEaseOut",
  property: LayoutAnimationProperty = "opacity",
): LayoutAnimationConfig {
  return { duration, create: { type, property }, update: { type }, delete: { type, property } };
}

/** Animates the layout changes of the next render natively: frames move,
 * new views fade in, removed views fade out. */
export const LayoutAnimation = {
  /** Animates the changes of the next commit. `onEnd` runs when done. */
  configureNext(config: LayoutAnimationConfig, onEnd?: () => void): void {
    command(1, "layoutAnimation", config);
    if (onEnd) setTimeout(onEnd, config.duration);
  },
  /** A config with `duration`, the same curve for all three phases. */
  create: layoutPreset as (
    duration: number,
    type?: LayoutAnimationType,
    property?: LayoutAnimationProperty,
  ) => LayoutAnimationConfig,
  Types: {
    spring: "spring",
    linear: "linear",
    easeInEaseOut: "easeInEaseOut",
    easeIn: "easeIn",
    easeOut: "easeOut",
  } as const,
  Properties: { opacity: "opacity", scaleXY: "scaleXY" } as const,
  Presets: {
    easeInEaseOut: layoutPreset(300) as LayoutAnimationConfig,
    linear: layoutPreset(500, "linear") as LayoutAnimationConfig,
    spring: {
      duration: 700,
      create: { type: "linear", property: "opacity" },
      update: { type: "spring", springDamping: 0.4 },
      delete: { type: "linear", property: "opacity" },
    } as LayoutAnimationConfig,
  },
  easeInEaseOut(onEnd?: () => void): void {
    LayoutAnimation.configureNext(LayoutAnimation.Presets.easeInEaseOut, onEnd);
  },
  linear(onEnd?: () => void): void {
    LayoutAnimation.configureNext(LayoutAnimation.Presets.linear, onEnd);
  },
  spring(onEnd?: () => void): void {
    LayoutAnimation.configureNext(LayoutAnimation.Presets.spring, onEnd);
  },
};

// --- Status bar, dialogs, toasts ------------------------------------------

export type StatusBarStyle = "default" | "light-content" | "dark-content";

export interface StatusBarProps {
  barStyle?: StatusBarStyle;
  hidden?: boolean;
  /** Animate changes of `barStyle` and `hidden`. */
  animated?: boolean;
  /** Android before edge-to-edge (API 35) only. */
  backgroundColor?: string;
}

/** Status bar settings by owner: rendered `<StatusBar>`s (the last mounted
 * wins, prop by prop) over the imperative ones. */
const statusBarStack: (() => StatusBarProps)[] = [];
const [statusBarBase, setStatusBarBase] = createSignal<StatusBarProps>({});
const [statusBarVersion, setStatusBarVersion] = createSignal(0);
let statusBarStarted = false;

function startStatusBar() {
  if (statusBarStarted) return;
  statusBarStarted = true;
  createRoot(() => {
    let last = "";
    effect(() => {
      statusBarVersion();
      const merged: StatusBarProps = { ...statusBarBase() };
      for (const entry of statusBarStack) {
        const p = entry();
        for (const key of Object.keys(p) as (keyof StatusBarProps)[]) {
          if (p[key] !== undefined) (merged as any)[key] = p[key];
        }
      }
      const key = JSON.stringify(merged);
      if (key !== last) {
        last = key;
        command(1, "statusBar", merged);
      }
    });
  });
}

/** `<StatusBar>` and its imperative methods. */
export interface StatusBarComponent {
  (props: StatusBarProps): null;
  setBarStyle(barStyle: StatusBarStyle, animated?: boolean): void;
  setHidden(hidden: boolean, animation?: "none" | "fade" | "slide"): void;
  setBackgroundColor(backgroundColor: string, animated?: boolean): void;
}

const setStatusBar = (patch: StatusBarProps) => {
  startStatusBar();
  setStatusBarBase((b) => ({ ...b, ...patch }));
};

/**
 * The status bar: render `<StatusBar barStyle="light-content" />` in a
 * screen (the most recently mounted one wins while it is mounted), or call
 * `StatusBar.setBarStyle(...)`. In an app embedded in a native one, the
 * host app's view controller decides on iOS.
 */
export const StatusBar: StatusBarComponent = Object.assign(
  (props: StatusBarProps): null => {
    startStatusBar();
    const entry = () => ({
      barStyle: props.barStyle,
      hidden: props.hidden,
      animated: props.animated,
      backgroundColor: props.backgroundColor,
    });
    statusBarStack.push(entry);
    effect(() => {
      entry();
      setStatusBarVersion((v) => v + 1);
    });
    onCleanup(() => {
      const i = statusBarStack.indexOf(entry);
      if (i >= 0) statusBarStack.splice(i, 1);
      setStatusBarVersion((v) => v + 1);
    });
    return null;
  },
  {
    setBarStyle: (barStyle: StatusBarStyle, animated?: boolean): void => setStatusBar({ barStyle, animated }),
    setHidden: (hidden: boolean, animation?: "none" | "fade" | "slide"): void =>
      setStatusBar({ hidden, animated: animation !== undefined && animation !== "none" }),
    setBackgroundColor: (backgroundColor: string, animated?: boolean): void =>
      setStatusBar({ backgroundColor, animated }),
  },
);

export interface AlertButton {
  text?: string;
  onPress?: () => void;
  style?: "default" | "cancel" | "destructive";
}

export interface AlertOptions {
  /** Tapping outside or the back button closes it (Android; default false). */
  cancelable?: boolean;
  onDismiss?: () => void;
}

let nextDialog = 1;
const dialogs = new Map<number, (index: number) => void>();

/** Shows a native dialog; `answer` gets the button index, or -1. */
function showDialog(spec: Record<string, unknown>, answer: (index: number) => void) {
  const id = nextDialog++;
  if (dialogs.size === 0) setProp(root, "onDialog", onDialog);
  dialogs.set(id, answer);
  command(1, "dialog", { ...spec, id });
}

function onDialog(e: { id: number; index: number }) {
  const answer = dialogs.get(e.id);
  dialogs.delete(e.id);
  if (dialogs.size === 0) setProp(root, "onDialog", undefined);
  answer?.(e.index);
}

// --- Image information ----------------------------------------------------------

let nextImageInfo = 1;
const imageInfos = new Map<number, (e: { width?: number; height?: number; error?: string }) => void>();

function onImageInfo(e: { id: number; width?: number; height?: number; error?: string }) {
  const done = imageInfos.get(e.id);
  imageInfos.delete(e.id);
  if (imageInfos.size === 0) setProp(root, "onImageInfo", undefined);
  done?.(e);
}

function imageInfo(uri: string): Promise<{ width: number; height: number }> {
  return new Promise((resolve, reject) => {
    const id = nextImageInfo++;
    if (imageInfos.size === 0) setProp(root, "onImageInfo", onImageInfo);
    imageInfos.set(id, (e) => {
      if (e.error !== undefined || e.width === undefined || e.height === undefined) reject(new Error(e.error ?? "no size"));
      else resolve({ width: e.width, height: e.height });
    });
    command(1, "imageInfo", { id, uri });
  });
}

export interface ImageModule {
  /** The image's size in pixels; its bytes stay loaded for the views that show it next. */
  getSize(source: string | { uri: string }): Promise<{ width: number; height: number }>;
  /** Loads the image ahead of showing it: true once its bytes are in memory. */
  prefetch(source: string | { uri: string }): Promise<boolean>;
}

/** Image information and loading ahead. */
export const Image: ImageModule = {
  getSize: (source) => imageInfo(typeof source === "string" ? source : source.uri),
  prefetch: (source) =>
    imageInfo(typeof source === "string" ? source : source.uri).then(
      () => true,
      () => false,
    ),
};

/** Native alerts (UIAlertController / AlertDialog). */
export const Alert = {
  /** At most three buttons on Android; "OK" when none is given. */
  alert(title: string, message?: string, buttons?: AlertButton[], options?: AlertOptions): void {
    const list = buttons && buttons.length > 0 ? buttons : [{ text: "OK" }];
    showDialog(
      {
        title,
        message,
        buttons: list.map((b) => ({ text: b.text ?? "OK", style: b.style ?? "default" })),
        cancelable: options?.cancelable ?? false,
      },
      (index) => {
        if (index >= 0) list[index]?.onPress?.();
        else options?.onDismiss?.();
      },
    );
  },
};

export interface ActionSheetOptions {
  options: string[];
  cancelButtonIndex?: number;
  destructiveButtonIndex?: number | number[];
  title?: string;
  message?: string;
}

/** A sheet of actions (UIAlertController action sheet; a list dialog on
 * Android). `callback` gets the chosen index (the cancel index when
 * dismissed). */
export const ActionSheetIOS = {
  showActionSheetWithOptions(options: ActionSheetOptions, callback: (index: number) => void): void {
    const destructive = ([] as number[]).concat(options.destructiveButtonIndex ?? []);
    showDialog(
      {
        title: options.title,
        message: options.message,
        sheet: true,
        cancelable: true,
        buttons: options.options.map((text, i) => ({
          text,
          style: i === options.cancelButtonIndex ? "cancel" : destructive.includes(i) ? "destructive" : "default",
        })),
      },
      (index) => callback(index >= 0 ? index : (options.cancelButtonIndex ?? -1)),
    );
  },
};

/** Short messages at the bottom of the screen (Android; ignored on iOS). */
export const ToastAndroid = {
  SHORT: 0 as const,
  LONG: 1 as const,
  show(message: string, duration: 0 | 1 = 0): void {
    command(1, "toast", { text: message, long: duration === 1 });
  },
};

/** The software keyboard. */
export const Keyboard = {
  /** Unfocuses the focused input, hiding the keyboard. */
  dismiss(): void {
    command(1, "blur");
  },
};

const ops: unknown[] = [];
let scheduled = false;
let nextId = 2; // 1 is the root view, owned by the platform.

function schedule() {
  if (!scheduled) {
    scheduled = true;
    queueMicrotask(flush);
  }
}

function flush() {
  scheduled = false;
  if (ops.length > 0) {
    __yf_commit(ops);
    ops.length = 0;
  }
}

// Nodes with listeners, for event dispatch. Weak so handlers never keep a
// collected node alive.
const listening = new Map<number, WeakRef<YNode>>();

// Tell the core when a node is garbage collected so it can free the native
// view. Nodes removed from the tree may be reinserted later (Solid keeps
// them in memos), so removal alone is not a release.
const registry = new FinalizationRegistry<number>((id) => {
  listening.delete(id);
  ops.push(OP_RELEASE, id);
  schedule();
});

/** The surface root view. */
export const root: YNode = new YNode(1, false);

// Before anything renders: classes resolve against it.
if (tailwindTheme) {
  ops.push(OP_SET_PROP, 1, "tailwindTheme", tailwindTheme);
  schedule();
}

// Nothing else references the tree once rendering is done (there is no
// `document` as in a browser): without this global anchor the whole app —
// nodes, handlers, signals and the registry itself — is one unreachable
// cycle that the next GC collects.
Object.defineProperty(globalThis, "__yf", { value: { root, registry, listening } });

function unlink(node: YNode) {
  const p = node.parent;
  if (p === null) return;
  if (node.prev !== null) node.prev.next = node.next;
  else p.first = node.next;
  if (node.next !== null) node.next.prev = node.prev;
  else p.last = node.prev;
  node.parent = node.prev = node.next = null;
}

function newNode(isText: boolean): YNode {
  const node = new YNode(nextId++, isText);
  registry.register(node, node.id);
  return node;
}

const renderer = createRenderer<YNode>({
  createElement(tag: string) {
    const node = newNode(false);
    ops.push(OP_CREATE, node.id, tag);
    schedule();
    return node;
  },
  createTextNode(value: string) {
    const node = newNode(true);
    ops.push(OP_CREATE_TEXT, node.id, value);
    schedule();
    return node;
  },
  replaceText(node: YNode, value: string) {
    ops.push(OP_SET_TEXT, node.id, value);
    schedule();
  },
  isTextNode(node: YNode) {
    return node.isText;
  },
  setProperty(node: YNode, name: string, value: any) {
    if (name.charCodeAt(0) === 111 /* o */ && name.charCodeAt(1) === 110 /* n */ && name.length > 2) {
      const handlers = node.handlers ?? (node.handlers = {});
      if (touchProps.has(name)) {
        // Touch and responder handlers live in JS; the native view only
        // needs to know that touches on it matter (`onTouch`).
        const before = listensToTouches(handlers);
        if (typeof value === "function") {
          handlers[name] = value;
          listening.set(node.id, new WeakRef(node));
        } else {
          delete handlers[name];
        }
        const after = listensToTouches(handlers);
        if (before !== after) {
          ops.push(OP_SET_PROP, node.id, "onTouch", after);
          schedule();
        }
        return;
      }
      if (typeof value === "function") {
        handlers[name] = value;
        listening.set(node.id, new WeakRef(node));
        ops.push(OP_SET_PROP, node.id, name, true);
      } else {
        delete handlers[name];
        ops.push(OP_SET_PROP, node.id, name, false);
      }
    } else {
      if (name === "style" || name === "pan" || name === "pinch" || name === "rotation" || name === "scrollValue") {
        retainAnims(node, name, value);
      }
      ops.push(OP_SET_PROP, node.id, name, value);
    }
    schedule();
  },
  insertNode(parent: YNode, node: YNode, anchor?: YNode) {
    unlink(node);
    node.parent = parent;
    if (anchor) {
      node.next = anchor;
      node.prev = anchor.prev;
      if (anchor.prev !== null) anchor.prev.next = node;
      else parent.first = node;
      anchor.prev = node;
    } else {
      node.prev = parent.last;
      if (parent.last !== null) parent.last.next = node;
      else parent.first = node;
      parent.last = node;
    }
    ops.push(OP_INSERT, parent.id, node.id, anchor ? anchor.id : 0);
    schedule();
  },
  removeNode: detachNode,
  getParentNode(node: YNode) {
    return node.parent ?? undefined;
  },
  getFirstChild(node: YNode) {
    return node.first ?? undefined;
  },
  getNextSibling(node: YNode) {
    return node.next ?? undefined;
  },
});

type Renderer = ReturnType<typeof createRenderer<YNode>>;
export const effect: Renderer["effect"] = renderer.effect;
export const memo: Renderer["memo"] = renderer.memo;
export const createComponent: Renderer["createComponent"] = renderer.createComponent;
export const createElement: Renderer["createElement"] = renderer.createElement;
export const createTextNode: Renderer["createTextNode"] = renderer.createTextNode;
export const insertNode: Renderer["insertNode"] = renderer.insertNode;
export const insert: Renderer["insert"] = renderer.insert;
export const spread: Renderer["spread"] = renderer.spread;
export const setProp: Renderer["setProp"] = renderer.setProp;
export const mergeProps: Renderer["mergeProps"] = renderer.mergeProps;
export const use: Renderer["use"] = renderer.use;

/**
 * Renders `code` into `element` (usually `root`); returns a function that
 * removes it. During development, a module that runs again after an edit
 * replaces what it rendered before.
 */
export function render(code: () => any, element: YNode): () => void {
  const before = new Set<YNode>();
  for (let c = element.first; c !== null; c = c.next) before.add(c);
  const disposeRoot = renderer.render(code, element);
  // Solid's universal `render` only disposes the reactive root: the views it
  // inserted are removed here.
  const dispose = () => {
    disposeRoot();
    for (let c = element.first; c !== null; ) {
      const next = c.next;
      if (!before.has(c)) detachNode(element, c);
      c = next;
    }
  };
  (globalThis as any).__yf_hot?.rendered(dispose);
  return dispose;
}

// --- Native animation values -------------------------------------------

let nextAnimNode = 1;
let nextAnimation = 1;
const animationEnds = new Map<number, (finished: boolean) => void>();
const animRegistry = new FinalizationRegistry<number>((node) => {
  ops.push(OP_ANIM, ANIM_RELEASE, node);
  schedule();
});

/** Keeps the native nodes referenced by a style / prop value alive with
 * the view. */
function retainAnims(node: YNode, name: string, value: any) {
  let found: NativeNode[] | null = null;
  const visit = (v: any) => {
    if (v instanceof NativeNode) (found ??= []).push(v);
    else if (Array.isArray(v)) v.forEach(visit);
    else if (v !== null && typeof v === "object") for (const k in v) visit(v[k]);
  };
  visit(value);
  if (found) (node.anims ??= {})[name] = found;
  else if (node.anims) delete node.anims[name];
}

type Operand = NativeNode | number;

/** A node of the native animation graph: evaluated on the UI thread. */
export class NativeNode {
  readonly __nv: number;
  /** Inputs, kept alive as long as this node is. */
  private readonly inputs: NativeNode[];

  constructor(inputs: NativeNode[] = []) {
    this.__nv = nextAnimNode++;
    this.inputs = inputs;
    animRegistry.register(this, this.__nv);
  }

  /** Maps this node through piecewise ranges: numbers, colors or angles
   * (`"45deg"`, `"1rad"`). Outside the input range it clamps by default;
   * `easing` shapes each segment. */
  interpolate(config: {
    inputRange: number[];
    outputRange: number[] | string[];
    extrapolate?: Extrapolation;
    extrapolateLeft?: Extrapolation;
    extrapolateRight?: Extrapolation;
    easing?: Easing;
  }): NativeNode {
    const node = new NativeNode([this]);
    const both = config.extrapolate ?? "clamp";
    ops.push(
      OP_ANIM,
      ANIM_INTERPOLATE,
      node.__nv,
      this.__nv,
      config.inputRange,
      config.outputRange,
      config.extrapolateLeft ?? both,
      config.extrapolateRight ?? both,
      config.easing ?? "linear",
    );
    schedule();
    return node;
  }

  /** Follows this node's changes, kept within `min..max` (a header that
   * hides while content scrolls up and comes back as it scrolls down). */
  diffClamp(min: number, max: number): NativeNode {
    const node = new NativeNode([this]);
    ops.push(OP_ANIM, ANIM_DIFF_CLAMP, node.__nv, this.__nv, min, max);
    schedule();
    return node;
  }

  add(b: Operand): NativeNode {
    return math("add", this, b);
  }
  sub(b: Operand): NativeNode {
    return math("sub", this, b);
  }
  mul(b: Operand): NativeNode {
    return math("mul", this, b);
  }
  div(b: Operand): NativeNode {
    return math("div", this, b);
  }
  /** Modulo, never negative for a positive `b` (e.g. angles in 0..360). */
  mod(b: Operand): NativeNode {
    return math("mod", this, b);
  }
}

export type Extrapolation = "extend" | "clamp" | "identity";

function math(op: string, a: Operand, b: Operand): NativeNode {
  const node = new NativeNode([a, b].filter((x): x is NativeNode => x instanceof NativeNode));
  ops.push(OP_ANIM, ANIM_MATH, node.__nv, op, a, b);
  schedule();
  return node;
}

/** A curve of the `Easing` module. */
export interface EasingCurve {
  curve: "quad" | "cubic" | "poly" | "sin" | "circle" | "exp" | "elastic" | "back" | "bounce";
  param?: number;
  mode: "in" | "out" | "inOut";
}

/** How a timing animation (or an interpolation segment) progresses: a name,
 * a cubic Bézier `[x1, y1, x2, y2]` or a curve from `Easing`. */
export type Easing =
  | "linear"
  | "ease"
  | "easeIn"
  | "easeOut"
  | "easeInOut"
  | [number, number, number, number]
  | EasingCurve;

const curve = (name: EasingCurve["curve"], param?: number): EasingCurve => ({ curve: name, param, mode: "in" });

/** Easing functions (React Native's `Easing`). */
export interface EasingModule {
  linear: Easing;
  ease: Easing;
  quad: EasingCurve;
  cubic: EasingCurve;
  poly(n: number): EasingCurve;
  sin: EasingCurve;
  circle: EasingCurve;
  exp: EasingCurve;
  elastic(bounciness?: number): EasingCurve;
  back(s?: number): EasingCurve;
  bounce: EasingCurve;
  bezier(x1: number, y1: number, x2: number, y2: number): Easing;
  in(e: Easing): Easing;
  out(e: Easing): Easing;
  inOut(e: Easing): Easing;
}

/** Easing functions (React Native's `Easing`): curves in their "in" form,
 * turned around with `out` / `inOut`. */
export const Easing: EasingModule = {
  linear: "linear" as Easing,
  ease: "ease" as Easing,
  quad: curve("quad"),
  cubic: curve("cubic"),
  poly: (n: number) => curve("poly", n),
  sin: curve("sin"),
  circle: curve("circle"),
  exp: curve("exp"),
  elastic: (bounciness = 1) => curve("elastic", bounciness),
  back: (s = 1.70158) => curve("back", s),
  bounce: curve("bounce"),
  bezier: (x1: number, y1: number, x2: number, y2: number): Easing => [x1, y1, x2, y2],
  in: (e: Easing): Easing => (typeof e === "object" && !Array.isArray(e) ? { ...e, mode: "in" } : e),
  out: (e: Easing): Easing => {
    if (Array.isArray(e)) return [1 - e[2], 1 - e[3], 1 - e[0], 1 - e[1]];
    if (typeof e === "object") return { ...e, mode: "out" };
    return e === "easeIn" ? "easeOut" : e === "easeOut" ? "easeIn" : e;
  },
  inOut: (e: Easing): Easing => (typeof e === "object" && !Array.isArray(e) ? { ...e, mode: "inOut" } : e),
};

/** A settable, animatable native value. */
export class NativeValue extends NativeNode {
  constructor(value: number) {
    super();
    ops.push(OP_ANIM, ANIM_VALUE, this.__nv, value);
    schedule();
  }

  /** Jumps to `value`, stopping any animation. */
  set(value: number): void {
    ops.push(OP_ANIM, ANIM_SET, this.__nv, value);
    schedule();
  }

  /** Stops the running animation where it is. */
  stop(): void {
    ops.push(OP_ANIM, ANIM_STOP, this.__nv);
    schedule();
  }

  /** Animates to `to` over `duration` ms. Resolves with `finished`. */
  timing(to: number, options: { duration?: number; easing?: Easing; delay?: number } = {}): Promise<boolean> {
    return this.start({ type: "timing", to, ...options });
  }

  /** Springs to `to`. Resolves with `finished`. */
  spring(
    to: number,
    options: { stiffness?: number; damping?: number; mass?: number; velocity?: number } = {},
  ): Promise<boolean> {
    return this.start({ type: "spring", to, ...options });
  }

  /** Coasts from `velocity` (units per second) and slows down. */
  decay(options: { velocity?: number; deceleration?: number } = {}): Promise<boolean> {
    return this.start({ type: "decay", ...options });
  }

  private start(config: object): Promise<boolean> {
    const id = nextAnimation++;
    ops.push(OP_ANIM, ANIM_START, this.__nv, id, config);
    schedule();
    return new Promise((resolve) => animationEnds.set(id, resolve));
  }
}

/** Creates a native value. Bind it (or nodes derived from it) to style
 * properties such as `opacity`, `transform` and `backgroundColor`, to a
 * scroll position (`scrollValue`) or to drags (`pan`). */
export function createNativeValue(value = 0): NativeValue {
  return new NativeValue(value);
}

// --- Composite animations -------------------------------------------------

/** An animation built with `Animated`: runs natively, steps and all. */
export interface CompositeAnimation {
  /** Runs it (again). Resolves (and calls `callback`) with whether it
   * finished rather than being stopped. */
  start(callback?: (result: { finished: boolean }) => void): Promise<boolean>;
  /** Stops it where it is. */
  stop(): void;
  /** Stops it and puts its values back where they were when it started. */
  reset(): void;
}

type Spec = Record<string, unknown>;

/** Composites that run, with what they hold alive. */
const runningComposites = new Set<Composite>();
let nextCompositeKey = 1;
/** The native side keeps each composite's start values (for `reset`). */
const compositeRegistry = new FinalizationRegistry<number>((key) => {
  ops.push(OP_ANIM, ANIM_RELEASE_COMPOSITE, key);
  schedule();
});

class Composite implements CompositeAnimation {
  private id = 0;
  private readonly key = nextCompositeKey++;

  constructor(
    readonly spec: () => Spec,
    readonly values: NativeValue[],
  ) {
    compositeRegistry.register(this, this.key);
  }

  start(callback?: (result: { finished: boolean }) => void): Promise<boolean> {
    if (this.id) this.stop();
    const id = (this.id = nextAnimation++);
    runningComposites.add(this);
    ops.push(OP_ANIM, ANIM_START_COMPOSITE, id, this.key, this.spec());
    schedule();
    return new Promise((resolve) =>
      animationEnds.set(id, (finished) => {
        if (this.id === id) {
          this.id = 0;
          runningComposites.delete(this);
        }
        callback?.({ finished });
        resolve(finished);
      }),
    );
  }

  stop(): void {
    if (!this.id) return;
    ops.push(OP_ANIM, ANIM_STOP_ANIMATION, this.id);
    schedule();
  }

  reset(): void {
    ops.push(OP_ANIM, ANIM_RESET_COMPOSITE, this.key);
    schedule();
  }
}

const run = (value: NativeValue, spec: Spec) => new Composite(() => ({ ...spec, node: value.__nv }), [value]);
const group = (type: string, list: CompositeAnimation[], extra: Spec = {}) => {
  const children = list as Composite[];
  return new Composite(
    () => ({ type, children: children.map((c) => c.spec()), ...extra }),
    children.flatMap((c) => c.values),
  );
};

/** Animations composed natively (React Native's `Animated`). */
export interface AnimatedModule {
  timing(value: NativeValue, config: { toValue: number; duration?: number; easing?: Easing; delay?: number }): CompositeAnimation;
  spring(
    value: NativeValue,
    config: { toValue: number; stiffness?: number; damping?: number; mass?: number; velocity?: number },
  ): CompositeAnimation;
  decay(value: NativeValue, config?: { velocity?: number; deceleration?: number }): CompositeAnimation;
  sequence(list: CompositeAnimation[]): CompositeAnimation;
  parallel(list: CompositeAnimation[], config?: { stopTogether?: boolean }): CompositeAnimation;
  stagger(ms: number, list: CompositeAnimation[]): CompositeAnimation;
  delay(ms: number): CompositeAnimation;
  loop(animation: CompositeAnimation, config?: { iterations?: number; resetBeforeIteration?: boolean }): CompositeAnimation;
  add(a: NativeNode, b: NativeNode | number): NativeNode;
  subtract(a: NativeNode, b: NativeNode | number): NativeNode;
  multiply(a: NativeNode, b: NativeNode | number): NativeNode;
  divide(a: NativeNode, b: NativeNode | number): NativeNode;
  modulo(a: NativeNode, b: NativeNode | number): NativeNode;
  diffClamp(a: NativeNode, min: number, max: number): NativeNode;
}

/** Animations composed natively (React Native's `Animated`): each step
 * starts on the UI thread the moment the previous one ends. */
export const Animated: AnimatedModule = {
  timing(value: NativeValue, config: { toValue: number; duration?: number; easing?: Easing; delay?: number }): CompositeAnimation {
    const { toValue, ...rest } = config;
    return run(value, { type: "timing", to: toValue, ...rest });
  },
  spring(
    value: NativeValue,
    config: { toValue: number; stiffness?: number; damping?: number; mass?: number; velocity?: number },
  ): CompositeAnimation {
    const { toValue, ...rest } = config;
    return run(value, { type: "spring", to: toValue, ...rest });
  },
  decay(value: NativeValue, config: { velocity?: number; deceleration?: number } = {}): CompositeAnimation {
    return run(value, { type: "decay", ...config });
  },
  /** One after the other; stops when one is stopped. */
  sequence(list: CompositeAnimation[]): CompositeAnimation {
    return group("sequence", list);
  },
  /** All at once; by default one stopping stops the others. */
  parallel(list: CompositeAnimation[], config: { stopTogether?: boolean } = {}): CompositeAnimation {
    return group("parallel", list, { stopTogether: config.stopTogether ?? true });
  },
  /** Starts each `ms` after the one before, in parallel. */
  stagger(ms: number, list: CompositeAnimation[]): CompositeAnimation {
    return Animated.parallel(list.map((a, i) => Animated.sequence([Animated.delay(ms * i), a])));
  },
  delay(ms: number): CompositeAnimation {
    return new Composite(() => ({ type: "delay", ms }), []);
  },
  /** Repeats (`iterations` -1 or unset: forever), putting the values back
   * before each run unless `resetBeforeIteration` is false. */
  loop(animation: CompositeAnimation, config: { iterations?: number; resetBeforeIteration?: boolean } = {}): CompositeAnimation {
    const child = animation as Composite;
    return new Composite(
      () => ({ type: "loop", child: child.spec(), iterations: config.iterations ?? -1, reset: config.resetBeforeIteration ?? true }),
      child.values,
    );
  },
  add: (a: NativeNode, b: Operand) => a.add(b),
  subtract: (a: NativeNode, b: Operand) => a.sub(b),
  multiply: (a: NativeNode, b: Operand) => a.mul(b),
  divide: (a: NativeNode, b: Operand) => a.div(b),
  modulo: (a: NativeNode, b: Operand) => a.mod(b),
  diffClamp: (a: NativeNode, min: number, max: number) => a.diffClamp(min, max),
};

(globalThis as any).__yf_anim_end = (id: number, finished: boolean) => {
  const resolve = animationEnds.get(id);
  if (resolve) {
    animationEnds.delete(id);
    resolve(finished);
  }
};

// --- Environment ----------------------------------------------------------

export interface Insets {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

/** What the platform reports about the surface; see the accessors below. */
interface Environment {
  window: { width: number; height: number };
  safeArea: Insets;
  keyboard: { height: number; duration: number };
  colorScheme: "light" | "dark";
  fontScale: number;
  /** The user's language is written right to left. */
  rtl: boolean;
  reduceMotion: boolean;
  pixelRatio: number;
  screenReader: boolean;
  os: "ios" | "android";
  osVersion: string | number;
  idiom: "phone" | "pad" | "mac" | "tv";
}

const [environment, setEnvironment] = createSignal<Environment>((globalThis as any).__yf_env_value, {
  equals: false,
});
(globalThis as any).__yf_env = (env: Environment) => setEnvironment(env);

// One memo per part, so a keyboard change does not rerun code that only
// reads the safe area.
const parts = createRoot(() => {
  const part = <K extends keyof Environment>(key: K, same: (a: Environment[K], b: Environment[K]) => boolean) =>
    createMemo(() => environment()[key], undefined, { equals: same });
  const sameInsets = (a: Insets, b: Insets) =>
    a.top === b.top && a.right === b.right && a.bottom === b.bottom && a.left === b.left;
  return [
    part("window", (a, b) => a.width === b.width && a.height === b.height),
    part("safeArea", sameInsets),
    part("keyboard", (a, b) => a.height === b.height && a.duration === b.duration),
    part("colorScheme", (a, b) => a === b),
    part("fontScale", (a, b) => a === b),
    part("screenReader", (a, b) => a === b),
    part("reduceMotion", (a, b) => a === b),
  ] as const;
});

/** Size of the surface in points (reactive). */
export const windowSize: Accessor<Environment["window"]> = parts[0];
/** Insets of the surface not covered by system UI (notch, home indicator,
 * title bar), in points (reactive). */
export const safeArea: Accessor<Insets> = parts[1];
/** Height of the software keyboard over the surface (0 when hidden) and the
 * duration of its animation in seconds (reactive). */
export const keyboard: Accessor<Environment["keyboard"]> = parts[2];
/** The system appearance (reactive). */
export const colorScheme: Accessor<Environment["colorScheme"]> = parts[3];
/** The user's preferred text size relative to the default (reactive). */
export const fontScale: Accessor<number> = parts[4];
/** Whether a screen reader (VoiceOver, TalkBack) runs (reactive). */
export const screenReaderEnabled: Accessor<boolean> = parts[5];
/** Whether the user asked for less motion (reactive; also `motion-reduce:`). */
export const reduceMotion: Accessor<boolean> = parts[6];

// --- Platform -----------------------------------------------------------------

const initialEnvironment = untrack(environment);

export interface PlatformSelectSpec<T> {
  ios?: T;
  android?: T;
  /** Both platforms (yf only runs natively). */
  native?: T;
  default?: T;
}

export interface PlatformStatic {
  readonly OS: "ios" | "android";
  /** iOS: the version string (`"17.5"`); Android: the API level. */
  readonly Version: string | number;
  readonly isPad: boolean;
  readonly isTV: boolean;
  /** Mac Catalyst in the Mac idiom. */
  readonly isMacCatalyst: boolean;
  readonly constants: {
    readonly osVersion: string | number;
    readonly interfaceIdiom: "phone" | "pad" | "mac" | "tv";
  };
  /** The value for this platform: `ios` / `android`, else `native`, else `default`. */
  select<T>(spec: PlatformSelectSpec<T>): T | undefined;
}

/** The platform the app runs on (React Native's `Platform`). */
export const Platform: PlatformStatic = {
  OS: initialEnvironment.os,
  Version: initialEnvironment.osVersion,
  isPad: initialEnvironment.idiom === "pad",
  isTV: initialEnvironment.idiom === "tv",
  isMacCatalyst: initialEnvironment.idiom === "mac",
  constants: { osVersion: initialEnvironment.osVersion, interfaceIdiom: initialEnvironment.idiom },
  select<T>(spec: PlatformSelectSpec<T>): T | undefined {
    const own = spec[initialEnvironment.os];
    if (own !== undefined) return own;
    return spec.native !== undefined ? spec.native : spec.default;
  },
};

/** Device pixels per point (React Native's `PixelRatio`). Not reactive:
 * read in components that also read `windowSize()` to follow a window
 * moving to another screen. */
export interface PixelRatioStatic {
  get(): number;
  getFontScale(): number;
  getPixelSizeForLayoutSize(size: number): number;
  roundToNearestPixel(size: number): number;
}

export const PixelRatio: PixelRatioStatic = {
  get(): number {
    return untrack(environment).pixelRatio;
  },
  /** The user's text size preference (also `fontScale()`). */
  getFontScale(): number {
    return untrack(environment).fontScale;
  },
  /** `size` points in whole device pixels. */
  getPixelSizeForLayoutSize(size: number): number {
    return Math.round(size * PixelRatio.get());
  },
  /** `size` rounded to the nearest device pixel, in points. */
  roundToNearestPixel(size: number): number {
    const ratio = PixelRatio.get();
    return Math.round(size * ratio) / ratio;
  },
};

// --- Accessibility --------------------------------------------------------------

export type AccessibilityChangeEvent = "screenReaderChanged" | "reduceMotionChanged";

const accessibilityListeners: Record<AccessibilityChangeEvent, Set<(enabled: boolean) => void>> = {
  screenReaderChanged: new Set(),
  reduceMotionChanged: new Set(),
};
createRoot(() => {
  const watch = (name: AccessibilityChangeEvent, value: Accessor<boolean>) => {
    let last = untrack(value);
    createRenderEffect(() => {
      const now = value();
      if (now === last) return;
      last = now;
      for (const listener of [...accessibilityListeners[name]]) listener(now);
    });
  };
  watch("screenReaderChanged", screenReaderEnabled);
  watch("reduceMotionChanged", reduceMotion);
});

/** Screen reader and motion settings (React Native's `AccessibilityInfo`).
 * `screenReaderEnabled()` and `reduceMotion()` are the reactive forms. */
export interface AccessibilityInfoStatic {
  isScreenReaderEnabled(): Promise<boolean>;
  isReduceMotionEnabled(): Promise<boolean>;
  addEventListener(event: AccessibilityChangeEvent, listener: (enabled: boolean) => void): { remove(): void };
  announceForAccessibility(message: string): void;
  setAccessibilityFocus(node: YNode): void;
}

export const AccessibilityInfo: AccessibilityInfoStatic = {
  isScreenReaderEnabled(): Promise<boolean> {
    return Promise.resolve(untrack(screenReaderEnabled));
  },
  isReduceMotionEnabled(): Promise<boolean> {
    return Promise.resolve(untrack(reduceMotion));
  },
  addEventListener(event: AccessibilityChangeEvent, listener: (enabled: boolean) => void): { remove(): void } {
    accessibilityListeners[event].add(listener);
    return { remove: () => accessibilityListeners[event].delete(listener) };
  },
  /** The screen reader reads `message` (VoiceOver announcement, TalkBack). */
  announceForAccessibility(message: string): void {
    command(1, "announce", message);
  },
  /** Moves the screen reader's focus to `node` (a `ref`). */
  setAccessibilityFocus(node: YNode): void {
    command(node.id, "accessibilityFocus");
  },
};

// --- Right to left ------------------------------------------------------------

const [allowRTL, setAllowRTL] = createSignal(true);
const [forceRTL, setForceRTL] = createSignal(false);
const isRTL = createRoot(() => {
  const rtl = createMemo(() => (allowRTL() && environment().rtl) || forceRTL());
  // The surface's direction; the core lays every view out by it.
  createRenderEffect(() => {
    ops.push(OP_SET_PROP, 1, "layoutDirectionRtl", rtl());
    schedule();
  });
  return rtl;
});

/** Right-to-left layout (React Native's `I18nManager`). Right to left,
 * rows run from the right and `left` / `right` style properties mean start
 * and end. Changes apply at once (no restart). */
export const I18nManager = {
  /** Whether the app lays out right to left (reactive). */
  get isRTL(): boolean {
    return isRTL();
  },
  /** `left` and `right` swap right to left (always, as React Native's default). */
  doLeftAndRightSwapInRTL: true as const,
  /** Follow right-to-left languages (default true). */
  allowRTL(allow: boolean): void {
    setAllowRTL(allow);
  },
  /** Lay out right to left whatever the language. */
  forceRTL(force: boolean): void {
    setForceRTL(force);
  },
  getConstants(): { isRTL: boolean; doLeftAndRightSwapInRTL: boolean } {
    return { isRTL: isRTL(), doLeftAndRightSwapInRTL: true };
  },
};

// --- Safe areas -------------------------------------------------------------

/** The insets of the nearest screen or modal (the surface's safe area outside
 * of them). */
const InsetsContext = createContext<Accessor<Insets>>(() => safeArea());

/** Insets not covered by system UI or headers where this component renders:
 * the surface's safe area, a modal's, or a screen's (below its header). */
export function useSafeAreaInsets(): Accessor<Insets> {
  return useContext(InsetsContext);
}

export interface SafeAreaViewProps extends ViewProps {
  /** Edges to pad (all by default). */
  edges?: ("top" | "right" | "bottom" | "left")[];
}

/** A view padded by the safe-area insets where it renders (on top of its
 * own padding, from `style` or classes). */
export function SafeAreaView(props: SafeAreaViewProps): YNode {
  const insets = useSafeAreaInsets();
  const [local, others] = splitProps(props, ["edges", "children"]);
  const node = createElement("view");
  spread(
    node,
    mergeSolidProps(others, {
      get insetPadding() {
        const i = insets();
        const on = (edge: "top" | "right" | "bottom" | "left") => !local.edges || local.edges.includes(edge);
        return {
          top: on("top") ? i.top : 0,
          right: on("right") ? i.right : 0,
          bottom: on("bottom") ? i.bottom : 0,
          left: on("left") ? i.left : 0,
        };
      },
    }),
    true,
  );
  insert(node, () => local.children);
  return node;
}

export interface KeyboardAvoidingViewProps extends ViewProps {
  /** Avoid the keyboard (default true). */
  enabled?: boolean;
  /**
   * How (React Native's names): `padding` (default) pads the bottom by the
   * keyboard's height; `height` shrinks the view instead (a bottom margin,
   * so its background ends at the keyboard); `position` moves an inner
   * content view (`contentContainerStyle`) up.
   */
  behavior?: "padding" | "height" | "position";
  /** Added to the keyboard's height while it shows (e.g. a tab bar below the view). */
  keyboardVerticalOffset?: number;
  /** The inner view of `behavior="position"`. */
  contentContainerStyle?: StyleProp<ViewStyle>;
}

/**
 * A view that keeps its content above the software keyboard, animated
 * with the keyboard (see `behavior`). For content outside scroll views,
 * such as a message bar at the bottom of a screen (inputs inside scroll
 * views are kept visible natively). It assumes it reaches the bottom of
 * the surface, as a screen's content does. Where the window shrinks for
 * the keyboard instead (an Android host that resizes), the keyboard covers
 * nothing and nothing moves.
 */
export function KeyboardAvoidingView(props: KeyboardAvoidingViewProps): YNode {
  const [local, others] = splitProps(props, [
    "enabled",
    "behavior",
    "keyboardVerticalOffset",
    "contentContainerStyle",
    "children",
  ]);
  const node = createElement("view");
  const lift = createMemo(() => {
    const height = keyboard().height;
    return local.enabled === false || height === 0 ? 0 : Math.max(0, height + (local.keyboardVerticalOffset ?? 0));
  });
  let last = 0;
  effect(() => {
    const height = lift();
    // Moves with the keyboard: its frame change animates natively.
    const duration = untrack(() => keyboard().duration);
    if (height !== last && duration > 0) {
      LayoutAnimation.configureNext({ duration: duration * 1000, update: { type: "easeInEaseOut" } });
    }
    last = height;
  });
  const behavior = () => local.behavior ?? "padding";
  const bottom = (on: boolean) => ({ top: 0, right: 0, bottom: on ? lift() : 0, left: 0 });
  spread(
    node,
    mergeSolidProps(others, {
      get insetPadding() {
        return bottom(behavior() === "padding");
      },
      get insetMargin() {
        return bottom(behavior() === "height");
      },
    }),
    true,
  );
  const content = createMemo(() => {
    if (behavior() !== "position") return local.children;
    const inner = createElement("view");
    spread(
      inner,
      {
        get style() {
          return [local.contentContainerStyle, { position: "relative", bottom: lift() }];
        },
      },
      true,
    );
    insert(inner, () => local.children);
    return inner;
  });
  insert(node, content);
  return node;
}

/** Keeps the insets a screen or modal reports (`onInsets`). */
function nativeInsets(initial: () => Insets) {
  const [insets, setInsets] = createSignal<Insets | null>(null, {
    equals: (a, b) => a !== null && b !== null && a.top === b.top && a.right === b.right && a.bottom === b.bottom && a.left === b.left,
  });
  return {
    insets: (() => insets() ?? initial()) as Accessor<Insets>,
    onInsets: (e: Insets) => setInsets({ top: e.top, right: e.right, bottom: e.bottom, left: e.left }),
  };
}

// --- Navigation -------------------------------------------------------------

/** A stack of routes for `<Stack>`: plain values, compared by identity. */
export interface Navigator<R> {
  /** The routes, bottom to top (reactive). */
  routes: Accessor<readonly R[]>;
  /** Pushes a screen. */
  push(route: R): void;
  /** Pops `count` screens (at least one stays). */
  pop(count?: number): void;
  /** Pops back to the first screen. */
  popToTop(): void;
  /** Replaces the top screen. */
  replace(route: R): void;
  /** Replaces all screens. */
  reset(routes: R[]): void;
  /** Removes a route wherever it is (the user popped its screen). */
  remove(route: R): void;
}

/** Creates the route stack of a `<Stack>`. */
export function createNavigator<R>(initial: R | R[]): Navigator<R> {
  const [routes, setRoutes] = createSignal<readonly R[]>(Array.isArray(initial) ? initial : [initial]);
  return {
    routes,
    push: (route) => setRoutes((r) => [...r, route]),
    pop: (count = 1) => setRoutes((r) => r.slice(0, Math.max(1, r.length - count))),
    popToTop: () => setRoutes((r) => r.slice(0, 1)),
    replace: (route) => setRoutes((r) => [...r.slice(0, -1), route]),
    reset: (rs) => setRoutes(rs),
    remove: (route) => setRoutes((r) => r.filter((x) => x !== route)),
  };
}

export interface StackProps<R> {
  navigator: Navigator<R>;
  style?: StyleProp<ViewStyle>;
  /** Renders a route; return a `<Screen>` for header options. */
  children: (route: R, index: Accessor<number>) => JSX.Element;
}

/**
 * A native navigation stack: UINavigationController on iOS (swipe back,
 * native header), a stack with a toolbar and predictive back on Android.
 * Screens stay mounted (with their state) while they are in the stack.
 */
export function Stack<R>(props: StackProps<R>): YNode {
  const stack = createElement("stack");
  effect((prev: unknown) => {
    const style = props.style;
    if (style !== prev) setProp(stack, "style", style, prev);
    return style;
  });
  const screens = mapArray(
    () => props.navigator.routes(),
    (route, index) => {
      const screen = (rendered: JSX.Element) => {
        const node = rendered instanceof YNode && rendered.tag === "screen" ? rendered : Screen({ children: rendered });
        // The user popped it natively: forget the route.
        node.popped = () => props.navigator.remove(route);
        return node;
      };
      const rendered = untrack(() => props.children(route, index));
      // An accessor (e.g. a component made hot in development) may render
      // another screen later.
      if (typeof rendered === "function") return createMemo(() => screen(rendered()));
      return screen(rendered);
    },
  );
  insert(stack, screens);
  return stack;
}

export interface ScreenProps {
  title?: string;
  headerShown?: boolean;
  /** iOS: a large title; the content scrolls under the header. */
  headerLargeTitle?: boolean;
  /** The content extends under the header. */
  headerTransparent?: boolean;
  headerTintColor?: string;
  headerBackgroundColor?: string;
  headerTitleColor?: string;
  /** Label of the back button leading back to this screen. */
  headerBackTitle?: string;
  headerBackVisible?: boolean;
  headerLeft?: JSX.Element;
  headerRight?: JSX.Element;
  /** Replaces the title. */
  headerTitle?: JSX.Element;
  /** The user can pop the screen (swipe back, back button). */
  gestureEnabled?: boolean;
  animation?: "default" | "fade" | "none";
  style?: StyleProp<ViewStyle>;
  onDismiss?: () => void;
  children?: JSX.Element;
}

/** One screen of a `<Stack>`, with its header options. */
export function Screen(props: ScreenProps): YNode {
  const node = createElement("screen");
  node.tag = "screen";
  const [local, others] = splitProps(props, ["children", "headerLeft", "headerRight", "headerTitle", "onDismiss"]);
  const { insets, onInsets } = nativeInsets(() => ({ ...safeArea(), top: 0 }));
  spread(
    node,
    mergeSolidProps(others, {
      onDismiss() {
        node.popped?.();
        local.onDismiss?.();
      },
      onInsets,
    }),
    true,
  );
  const slot = (name: string, content: Accessor<any>) =>
    createMemo(() => {
      const c = content();
      if (c == null || c === false) return null;
      const item = createElement("headeritem");
      setProp(item, "headerSlot", name);
      insert(item, c);
      return item;
    });
  const left = slot("left", resolveChildren(() => local.headerLeft));
  const right = slot("right", resolveChildren(() => local.headerRight));
  const title = slot("title", resolveChildren(() => local.headerTitle));
  insert(node, () => [
    left(),
    right(),
    title(),
    createComponent(InsetsContext.Provider, {
      value: insets,
      get children() {
        return local.children;
      },
    }),
  ]);
  return node;
}

// --- Canvas ---------------------------------------------------------------------

// Canvas operations: `crates/yf-js/src/canvas.rs`.
const CV_SAVE = 0;
const CV_RESTORE = 1;
const CV_TRANSLATE = 2;
const CV_SCALE = 3;
const CV_ROTATE = 4;
const CV_TRANSFORM = 5;
const CV_SET_TRANSFORM = 6;
const CV_BEGIN_PATH = 7;
const CV_MOVE_TO = 8;
const CV_LINE_TO = 9;
const CV_QUAD_TO = 10;
const CV_CUBIC_TO = 11;
const CV_ARC = 12;
const CV_ARC_TO = 13;
const CV_ELLIPSE = 14;
const CV_RECT = 15;
const CV_ROUND_RECT = 16;
const CV_CLOSE = 17;
const CV_FILL = 18;
const CV_STROKE = 19;
const CV_CLIP = 20;
const CV_FILL_RECT = 21;
const CV_STROKE_RECT = 22;
const CV_CLEAR_RECT = 23;
const CV_FILL_STYLE = 24;
const CV_STROKE_STYLE = 25;
const CV_LINE_WIDTH = 26;
const CV_LINE_CAP = 27;
const CV_LINE_JOIN = 28;
const CV_MITER_LIMIT = 29;
const CV_LINE_DASH = 30;
const CV_LINE_DASH_OFFSET = 31;
const CV_GLOBAL_ALPHA = 32;
const CV_COMPOSITE = 33;
const CV_SHADOW_COLOR = 34;
const CV_SHADOW_BLUR = 35;
const CV_SHADOW_OFFSET = 36;
const CV_FONT = 37;
const CV_TEXT_ALIGN = 38;
const CV_TEXT_BASELINE = 39;
const CV_FILL_TEXT = 40;
const CV_STROKE_TEXT = 41;
const CV_DRAW_IMAGE = 42;
const CV_SVG_PATH = 50;

declare function __yf_measure_text(font: string, text: string): number;

/** A number in a drawing: a constant, or a native value the canvas reads
 * while it draws (its animations redraw natively, without JS). */
export type CanvasNumber = number | NativeNode;

/** A color, a gradient, or a native value interpolating colors. */
export type CanvasPaint = string | CanvasGradient | NativeNode;

/** Path building, shared by the context (its current path) and `Path2D`. */
export abstract class CanvasPath {
  /** Where the operations go. */
  protected abstract out: unknown[];

  moveTo(x: CanvasNumber, y: CanvasNumber): void {
    this.out.push(CV_MOVE_TO, x, y);
  }
  lineTo(x: CanvasNumber, y: CanvasNumber): void {
    this.out.push(CV_LINE_TO, x, y);
  }
  quadraticCurveTo(cpx: CanvasNumber, cpy: CanvasNumber, x: CanvasNumber, y: CanvasNumber): void {
    this.out.push(CV_QUAD_TO, cpx, cpy, x, y);
  }
  bezierCurveTo(
    cp1x: CanvasNumber,
    cp1y: CanvasNumber,
    cp2x: CanvasNumber,
    cp2y: CanvasNumber,
    x: CanvasNumber,
    y: CanvasNumber,
  ): void {
    this.out.push(CV_CUBIC_TO, cp1x, cp1y, cp2x, cp2y, x, y);
  }
  /** Angles in radians, clockwise from the x axis. */
  arc(
    x: CanvasNumber,
    y: CanvasNumber,
    radius: CanvasNumber,
    startAngle: CanvasNumber,
    endAngle: CanvasNumber,
    counterclockwise = false,
  ): void {
    this.out.push(CV_ARC, x, y, radius, startAngle, endAngle, counterclockwise);
  }
  arcTo(x1: CanvasNumber, y1: CanvasNumber, x2: CanvasNumber, y2: CanvasNumber, radius: CanvasNumber): void {
    this.out.push(CV_ARC_TO, x1, y1, x2, y2, radius);
  }
  ellipse(
    x: CanvasNumber,
    y: CanvasNumber,
    radiusX: CanvasNumber,
    radiusY: CanvasNumber,
    rotation: CanvasNumber,
    startAngle: CanvasNumber,
    endAngle: CanvasNumber,
    counterclockwise = false,
  ): void {
    this.out.push(CV_ELLIPSE, x, y, radiusX, radiusY, rotation, startAngle, endAngle, counterclockwise);
  }
  rect(x: CanvasNumber, y: CanvasNumber, w: CanvasNumber, h: CanvasNumber): void {
    this.out.push(CV_RECT, x, y, w, h);
  }
  /** `radii`: one radius, or 2–4 as in CSS `border-radius` (top-left first). */
  roundRect(
    x: CanvasNumber,
    y: CanvasNumber,
    w: CanvasNumber,
    h: CanvasNumber,
    radii: CanvasNumber | CanvasNumber[] = 0,
  ): void {
    const r = Array.isArray(radii) ? radii : [radii];
    const [tl, tr, br, bl] =
      r.length >= 4 ? r : r.length === 3 ? [r[0], r[1], r[2], r[1]] : r.length === 2 ? [r[0], r[1], r[0], r[1]] : [r[0] ?? 0, r[0] ?? 0, r[0] ?? 0, r[0] ?? 0];
    this.out.push(CV_ROUND_RECT, x, y, w, h, tl, tr, br, bl);
  }
  closePath(): void {
    this.out.push(CV_CLOSE);
  }
}

/** A path to fill, stroke or clip; `new Path2D("M0 0 L10 10")` takes SVG
 * path data (parsed natively). */
export class Path2D extends CanvasPath {
  /** The recorded operations. */
  readonly ops: unknown[] = [];
  protected get out(): unknown[] {
    return this.ops;
  }

  constructor(path?: Path2D | string) {
    super();
    if (typeof path === "string") this.ops.push(CV_SVG_PATH, path);
    else if (path) this.ops.push(...path.ops);
  }

  /** Appends `path`'s subpaths. */
  addPath(path: Path2D): void {
    this.ops.push(...path.ops);
  }
}

/** A gradient from `createLinearGradient` / `createRadialGradient` /
 * `createConicGradient`. */
export class CanvasGradient {
  /** `[kind, ...geometry, stops]`: what the drawing carries. */
  readonly spec: unknown[];
  private readonly stops: unknown[] = [];

  constructor(kind: number, geometry: number[]) {
    this.spec = [kind, ...geometry, this.stops];
  }

  /** `offset` 0..1; any CSS color. */
  addColorStop(offset: number, color: string): void {
    this.stops.push(offset, color);
  }
}

export interface TextMetrics {
  /** The advance width in points. */
  width: number;
}

interface CanvasState {
  fillStyle: CanvasPaint;
  strokeStyle: CanvasPaint;
  lineWidth: CanvasNumber;
  lineCap: "butt" | "round" | "square";
  lineJoin: "miter" | "round" | "bevel";
  miterLimit: number;
  lineDash: number[];
  lineDashOffset: CanvasNumber;
  globalAlpha: CanvasNumber;
  globalCompositeOperation: string;
  shadowColor: string;
  shadowBlur: number;
  shadowOffsetX: number;
  shadowOffsetY: number;
  font: string;
  textAlign: "start" | "end" | "left" | "right" | "center";
  textBaseline: "top" | "hanging" | "middle" | "alphabetic" | "ideographic" | "bottom";
}

const initialCanvasState: CanvasState = {
  fillStyle: "#000",
  strokeStyle: "#000",
  lineWidth: 1,
  lineCap: "butt",
  lineJoin: "miter",
  miterLimit: 10,
  lineDash: [],
  lineDashOffset: 0,
  globalAlpha: 1,
  globalCompositeOperation: "source-over",
  shadowColor: "rgba(0,0,0,0)",
  shadowBlur: 0,
  shadowOffsetX: 0,
  shadowOffsetY: 0,
  font: "10px sans-serif",
  textAlign: "start",
  textBaseline: "alphabetic",
};

const paintValue = (p: CanvasPaint): unknown => (p instanceof CanvasGradient ? p.spec : p);

/**
 * The 2D context `<Canvas draw>` gets (an HTML Canvas 2D subset). Calls only
 * record operations; the platform draws them after `draw` returns (and again
 * whenever a native value they read changes). Coordinates are in points,
 * the origin at the canvas's top left.
 */
export class CanvasRenderingContext2D extends CanvasPath {
  protected out: unknown[];
  /** The canvas's size in points. */
  readonly canvas: { readonly width: number; readonly height: number };
  private s: CanvasState = { ...initialCanvasState };
  private stack: CanvasState[] = [];

  constructor(out: unknown[], size: { width: number; height: number }) {
    super();
    this.out = out;
    this.canvas = size;
  }

  /** Ends recording: later calls (e.g. from a timer) are ignored. */
  close(): void {
    this.out = [];
  }

  private set<K extends keyof CanvasState>(key: K, value: CanvasState[K], op: number, encoded: unknown): void {
    this.s[key] = value;
    this.out.push(op, encoded);
  }

  get fillStyle(): CanvasPaint {
    return this.s.fillStyle;
  }
  set fillStyle(v: CanvasPaint) {
    this.set("fillStyle", v, CV_FILL_STYLE, paintValue(v));
  }
  get strokeStyle(): CanvasPaint {
    return this.s.strokeStyle;
  }
  set strokeStyle(v: CanvasPaint) {
    this.set("strokeStyle", v, CV_STROKE_STYLE, paintValue(v));
  }
  get lineWidth(): CanvasNumber {
    return this.s.lineWidth;
  }
  set lineWidth(v: CanvasNumber) {
    this.set("lineWidth", v, CV_LINE_WIDTH, v);
  }
  get lineCap(): CanvasState["lineCap"] {
    return this.s.lineCap;
  }
  set lineCap(v: CanvasState["lineCap"]) {
    this.set("lineCap", v, CV_LINE_CAP, v);
  }
  get lineJoin(): CanvasState["lineJoin"] {
    return this.s.lineJoin;
  }
  set lineJoin(v: CanvasState["lineJoin"]) {
    this.set("lineJoin", v, CV_LINE_JOIN, v);
  }
  get miterLimit(): number {
    return this.s.miterLimit;
  }
  set miterLimit(v: number) {
    this.set("miterLimit", v, CV_MITER_LIMIT, v);
  }
  get lineDashOffset(): CanvasNumber {
    return this.s.lineDashOffset;
  }
  set lineDashOffset(v: CanvasNumber) {
    this.set("lineDashOffset", v, CV_LINE_DASH_OFFSET, v);
  }
  get globalAlpha(): CanvasNumber {
    return this.s.globalAlpha;
  }
  set globalAlpha(v: CanvasNumber) {
    this.set("globalAlpha", v, CV_GLOBAL_ALPHA, v);
  }
  get globalCompositeOperation(): string {
    return this.s.globalCompositeOperation;
  }
  set globalCompositeOperation(v: string) {
    this.set("globalCompositeOperation", v, CV_COMPOSITE, v);
  }
  get shadowColor(): string {
    return this.s.shadowColor;
  }
  set shadowColor(v: string) {
    this.set("shadowColor", v, CV_SHADOW_COLOR, v);
  }
  get shadowBlur(): number {
    return this.s.shadowBlur;
  }
  set shadowBlur(v: number) {
    this.set("shadowBlur", v, CV_SHADOW_BLUR, v);
  }
  get shadowOffsetX(): number {
    return this.s.shadowOffsetX;
  }
  set shadowOffsetX(v: number) {
    this.s.shadowOffsetX = v;
    this.out.push(CV_SHADOW_OFFSET, v, this.s.shadowOffsetY);
  }
  get shadowOffsetY(): number {
    return this.s.shadowOffsetY;
  }
  set shadowOffsetY(v: number) {
    this.s.shadowOffsetY = v;
    this.out.push(CV_SHADOW_OFFSET, this.s.shadowOffsetX, v);
  }
  /** CSS `font` shorthand, e.g. `"600 14px Inter"` (sizes in px, pt or rem). */
  get font(): string {
    return this.s.font;
  }
  set font(v: string) {
    this.set("font", v, CV_FONT, v);
  }
  get textAlign(): CanvasState["textAlign"] {
    return this.s.textAlign;
  }
  set textAlign(v: CanvasState["textAlign"]) {
    this.set("textAlign", v, CV_TEXT_ALIGN, v);
  }
  get textBaseline(): CanvasState["textBaseline"] {
    return this.s.textBaseline;
  }
  set textBaseline(v: CanvasState["textBaseline"]) {
    this.set("textBaseline", v, CV_TEXT_BASELINE, v);
  }

  save(): void {
    this.stack.push({ ...this.s });
    this.out.push(CV_SAVE);
  }
  restore(): void {
    const s = this.stack.pop();
    if (!s) return;
    this.s = s;
    this.out.push(CV_RESTORE);
  }
  translate(x: CanvasNumber, y: CanvasNumber): void {
    this.out.push(CV_TRANSLATE, x, y);
  }
  scale(x: CanvasNumber, y: CanvasNumber): void {
    this.out.push(CV_SCALE, x, y);
  }
  /** Radians, clockwise. */
  rotate(angle: CanvasNumber): void {
    this.out.push(CV_ROTATE, angle);
  }
  transform(a: CanvasNumber, b: CanvasNumber, c: CanvasNumber, d: CanvasNumber, e: CanvasNumber, f: CanvasNumber): void {
    this.out.push(CV_TRANSFORM, a, b, c, d, e, f);
  }
  setTransform(a: CanvasNumber, b: CanvasNumber, c: CanvasNumber, d: CanvasNumber, e: CanvasNumber, f: CanvasNumber): void {
    this.out.push(CV_SET_TRANSFORM, a, b, c, d, e, f);
  }
  resetTransform(): void {
    this.out.push(CV_SET_TRANSFORM, 1, 0, 0, 1, 0, 0);
  }
  setLineDash(segments: number[]): void {
    this.s.lineDash = [...segments];
    this.out.push(CV_LINE_DASH, this.s.lineDash);
  }
  getLineDash(): number[] {
    const d = this.s.lineDash;
    return d.length % 2 ? [...d, ...d] : [...d];
  }

  beginPath(): void {
    this.out.push(CV_BEGIN_PATH);
  }
  fill(path?: Path2D | "nonzero" | "evenodd", rule?: "nonzero" | "evenodd"): void {
    if (path instanceof Path2D) this.out.push(CV_FILL, path.ops, rule ?? "nonzero");
    else this.out.push(CV_FILL, null, path ?? "nonzero");
  }
  stroke(path?: Path2D): void {
    this.out.push(CV_STROKE, path ? path.ops : null);
  }
  clip(path?: Path2D | "nonzero" | "evenodd", rule?: "nonzero" | "evenodd"): void {
    if (path instanceof Path2D) this.out.push(CV_CLIP, path.ops, rule ?? "nonzero");
    else this.out.push(CV_CLIP, null, path ?? "nonzero");
  }
  fillRect(x: CanvasNumber, y: CanvasNumber, w: CanvasNumber, h: CanvasNumber): void {
    this.out.push(CV_FILL_RECT, x, y, w, h);
  }
  strokeRect(x: CanvasNumber, y: CanvasNumber, w: CanvasNumber, h: CanvasNumber): void {
    this.out.push(CV_STROKE_RECT, x, y, w, h);
  }
  clearRect(x: CanvasNumber, y: CanvasNumber, w: CanvasNumber, h: CanvasNumber): void {
    this.out.push(CV_CLEAR_RECT, x, y, w, h);
  }
  fillText(text: string, x: CanvasNumber, y: CanvasNumber, maxWidth?: number): void {
    this.out.push(CV_FILL_TEXT, String(text), x, y, maxWidth ?? null);
  }
  strokeText(text: string, x: CanvasNumber, y: CanvasNumber, maxWidth?: number): void {
    this.out.push(CV_STROKE_TEXT, String(text), x, y, maxWidth ?? null);
  }
  /** Measured by the platform's text engine with the current `font`. */
  measureText(text: string): TextMetrics {
    return { width: __yf_measure_text(this.s.font, String(text)) };
  }

  createLinearGradient(x0: number, y0: number, x1: number, y1: number): CanvasGradient {
    return new CanvasGradient(0, [x0, y0, x1, y1]);
  }
  createRadialGradient(x0: number, y0: number, r0: number, x1: number, y1: number, r1: number): CanvasGradient {
    return new CanvasGradient(1, [x0, y0, r0, x1, y1, r1]);
  }
  /** `startAngle` in radians, clockwise from the x axis. */
  createConicGradient(startAngle: number, x: number, y: number): CanvasGradient {
    return new CanvasGradient(2, [startAngle, x, y]);
  }

  /**
   * Draws an image (a URL or an imported asset), loaded natively:
   * `(image, dx, dy)` at its natural size, `(image, dx, dy, dw, dh)`, or
   * `(image, sx, sy, sw, sh, dx, dy, dw, dh)` for part of it (source in
   * image pixels). The canvas draws again once it has loaded.
   */
  drawImage(image: string | ImageAsset | { uri: string }, ...args: CanvasNumber[]): void {
    const uri = typeof image === "string" ? image : image.uri;
    const asset = typeof image === "string" ? null : (image as Partial<ImageAsset>);
    if (args.length >= 8) {
      this.out.push(CV_DRAW_IMAGE, uri, args.slice(0, 4), args[4], args[5], args[6], args[7]);
    } else if (args.length >= 4) {
      this.out.push(CV_DRAW_IMAGE, uri, null, args[0], args[1], args[2], args[3]);
    } else {
      this.out.push(CV_DRAW_IMAGE, uri, null, args[0] ?? 0, args[1] ?? 0, asset?.width ?? NaN, asset?.height ?? NaN);
    }
  }
}

export interface CanvasSize {
  width: number;
  height: number;
}

export interface CanvasProps extends ViewProps {
  /**
   * Draws the canvas. Runs again when signals it reads change and when the
   * canvas resizes (it first runs once the size is known); native values
   * passed as numbers or colors animate without running it.
   */
  draw: (ctx: CanvasRenderingContext2D, size: CanvasSize) => void;
}

/**
 * A view drawn with a 2D context (HTML Canvas 2D API). `draw` records the
 * drawing in one go; the platform replays it natively (CGContext, Android
 * Canvas), at the screen's resolution.
 */
export function Canvas(props: CanvasProps): YNode {
  const [local, others] = splitProps(props, ["draw", "onLayout"]);
  const node = createElement("canvas");
  const [size, setSize] = createSignal<CanvasSize | null>(null, {
    equals: (a, b) => a !== null && b !== null && a.width === b.width && a.height === b.height,
  });
  spread(
    node,
    mergeSolidProps(others, {
      onLayout(e: LayoutEvent) {
        setSize({ width: e.width, height: e.height });
        local.onLayout?.(e);
      },
    }),
    false,
  );
  createRenderEffect(() => {
    const s = size();
    if (!s) return;
    // Its own list: native values made while drawing push ops of their own.
    const out: unknown[] = [];
    const ctx = new CanvasRenderingContext2D(out, s);
    try {
      local.draw(ctx, s);
    } finally {
      ctx.close();
      ops.push(OP_DRAW, node.id, out);
      // The native values it reads live as long as the view.
      const used: NativeNode[] = [];
      for (const v of out) {
        if (v instanceof NativeNode) used.push(v);
        else if (Array.isArray(v)) for (const w of v) if (w instanceof NativeNode) used.push(w);
      }
      node.anims = { ...node.anims, draw: used };
      schedule();
    }
  });
  return node;
}

// --- Input accessories ------------------------------------------------------------

export interface InputAccessoryViewProps {
  /** Inputs with this `inputAccessoryViewID` show the view over the keyboard. */
  nativeID: string;
  backgroundColor?: ColorValue;
  style?: StyleProp<ViewStyle>;
  class?: string;
  children?: JSX.Element;
}

/**
 * A bar over the keyboard of the inputs whose `inputAccessoryViewID` is its
 * `nativeID` (iOS's `inputAccessoryView`; React Native's
 * `InputAccessoryView`): as wide as the window and as tall as its content,
 * moving with the keyboard natively. It renders nothing where it is placed
 * in the tree, and nothing at all on Android (as React Native).
 */
export function InputAccessoryView(props: InputAccessoryViewProps): YNode | null {
  if (Platform.OS !== "ios") return null;
  const node = createElement("inputaccessory");
  spread(
    node,
    {
      get nativeID() {
        return props.nativeID;
      },
      get style() {
        return [{ backgroundColor: props.backgroundColor }, props.style];
      },
      get class() {
        return props.class;
      },
    },
    true,
  );
  insert(node, () => props.children);
  return node;
}

export interface ModalProps {
  /** Shown while true (default). */
  visible?: boolean;
  animationType?: "slide" | "fade" | "none";
  /** iOS: `"pageSheet"` / `"formSheet"` keep what is behind visible. */
  presentationStyle?: "fullScreen" | "pageSheet" | "formSheet";
  /** The background shows what is behind. */
  transparent?: boolean;
  /** Android: the content reaches under the status bar (default: it starts below). */
  statusBarTranslucent?: boolean;
  /** iOS: the orientations it allows (default: the presenter's). */
  supportedOrientations?: ("portrait" | "portrait-upside-down" | "landscape" | "landscape-left" | "landscape-right")[];
  /** The user asked to close (Android back button, iOS sheet swipe): set
   * `visible` to false to close it. */
  onRequestClose?: () => void;
  onShow?: () => void;
  onDismiss?: () => void;
  style?: StyleProp<ViewStyle>;
  children?: JSX.Element;
}

/** Content presented over everything else, in its own native window layer. */
export function Modal(props: ModalProps): any {
  return createMemo(() => {
    if (props.visible === false) return null;
    return untrack(() => {
      const node = createElement("modal");
      const [local, others] = splitProps(props, ["children", "visible"]);
      const { insets, onInsets } = nativeInsets(() => safeArea());
      spread(node, mergeSolidProps(others, { onInsets }), true);
      insert(node, () =>
        createComponent(InsetsContext.Provider, {
          value: insets,
          get children() {
            return local.children;
          },
        }),
      );
      return node;
    });
  });
}

// --- Controls -----------------------------------------------------------------

/** A component rendering intrinsic element `tag` with its props. */
function hostComponent<P>(tag: string): (props: P) => YNode {
  return (props: P) => {
    const node = createElement(tag);
    spread(node, props as any, true);
    return node;
  };
}

export interface WebViewNavigation {
  url: string;
  title: string;
  loading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
}

export interface WebViewProps extends Omit<ViewProps, "children" | "onLoad" | "onError"> {
  source: string | { uri: string } | { html: string; baseUrl?: string };
  /** Runs after each page loads. */
  injectedJavaScript?: string;
  /** Runs before each page's own scripts. */
  injectedJavaScriptBeforeContentLoaded?: string;
  userAgent?: string;
  scrollEnabled?: boolean;
  javaScriptEnabled?: boolean;
  /** iOS: swipe back and forward. */
  allowsBackForwardNavigationGestures?: boolean;
  onLoadStart?: (e: WebViewNavigation) => void;
  onLoad?: (e: WebViewNavigation) => void;
  onError?: (e: { message: string }) => void;
  /** The page called `window.ReactNativeWebView.postMessage(data)`. */
  onMessage?: (e: { data: string }) => void;
  onNavigationStateChange?: (e: WebViewNavigation) => void;
}

/** A web page (WKWebView / android.webkit.WebView); its `ref` has
 * `reload()`, `goBack()`, `goForward()`, `stopLoading()`,
 * `injectJavaScript(script)` and `postMessage(data)`. */
export const WebView: (props: WebViewProps) => YNode = hostComponent<WebViewProps>("webview");

export interface VideoProps extends Omit<ViewProps, "children" | "onLoad" | "onError"> {
  source: string | { uri: string };
  paused?: boolean;
  muted?: boolean;
  /** 0..1. */
  volume?: number;
  rate?: number;
  repeat?: boolean;
  resizeMode?: "contain" | "cover" | "stretch";
  /** The platform's playback controls. */
  controls?: boolean;
  /** Milliseconds between `onProgress` events (250 by default). */
  progressUpdateInterval?: number;
  onLoadStart?: () => void;
  onLoad?: (e: { duration: number; width: number; height: number }) => void;
  onProgress?: (e: { currentTime: number; duration: number }) => void;
  onEnd?: () => void;
  onBuffer?: (e: { isBuffering: boolean }) => void;
  onError?: (e: { message: string }) => void;
}

/** A video player (AVPlayer / MediaPlayer); its `ref` has `seek(seconds)`. */
export const Video: (props: VideoProps) => YNode = hostComponent<VideoProps>("video");

// --- Back button (Android) ---------------------------------------------------

const backListeners: (() => boolean | void)[] = [];

function onBackPress() {
  for (let i = backListeners.length - 1; i >= 0; i--) {
    if (backListeners[i]()) return;
  }
  BackHandler.exitApp();
}

/** The Android back button, after modals and stacks had their turn. */
export const BackHandler = {
  /** `listener` returns true when it handled the press; the most recently
   * added listener runs first. Without a listener that handles it, the app
   * exits. */
  addEventListener(_event: "hardwareBackPress", listener: () => boolean | void): { remove(): void } {
    backListeners.push(listener);
    if (backListeners.length === 1) setProp(root, "onBackPress", onBackPress);
    return {
      remove(): void {
        const i = backListeners.indexOf(listener);
        if (i >= 0) backListeners.splice(i, 1);
        if (backListeners.length === 0) setProp(root, "onBackPress", undefined);
      },
    };
  },
  /** Leaves the app (Android; does nothing on iOS). */
  exitApp(): void {
    command(1, "exitApp");
  },
};

// --- Virtualized list ----------------------------------------------------

type ListBinder = (flat: number[]) => void;
const lists = new Map<number, ListBinder>();

/** A component (`Header`) or an element (`<Header />`). */
export type ListSlot = (() => JSX.Element) | JSX.Element;

/** An item that became viewable or stopped being viewable. */
export interface ViewToken<T> {
  item: T;
  key: string;
  index: number;
  isViewable: boolean;
}

export interface ViewabilityConfig {
  /** Percent of the viewport an item must cover to count as viewable. */
  viewAreaCoveragePercentThreshold?: number;
  /** Percent of an item that must be visible. Without either threshold, any visible pixel counts. */
  itemVisiblePercentThreshold?: number;
  /** Milliseconds items must stay viewable before they are reported. */
  minimumViewTime?: number;
}

export interface ListProps<T> extends ScrollBehaviorProps {
  each: readonly T[];
  /** Size used for items not measured yet (points). */
  estimatedItemSize?: number;
  horizontal?: boolean;
  /** Items per row (vertical lists). */
  numColumns?: number;
  /** Laid out from the end, item 0 last (chat style); starts scrolled to the end and keeps its distance from the end as content changes. */
  inverted?: boolean;
  /** Items that stick to the top while scrolled past (one column, not inverted). */
  stickyHeaderIndices?: number[];
  ListHeaderComponent?: ListSlot;
  ListFooterComponent?: ListSlot;
  /** Shown instead of the items while `each` is empty. */
  ListEmptyComponent?: ListSlot;
  /** Rendered between items (one column). */
  ItemSeparatorComponent?: ListSlot;
  /** Padding (`padding`, `paddingHorizontal`, `paddingTop`, …) and `gap` / `rowGap` / `columnGap` of the content. */
  contentContainerStyle?: {
    padding?: number;
    paddingHorizontal?: number;
    paddingVertical?: number;
    paddingTop?: number;
    paddingRight?: number;
    paddingBottom?: number;
    paddingLeft?: number;
    gap?: number;
    rowGap?: number;
    columnGap?: number;
  };
  /** Scrolls to this item before the first frame. */
  initialScrollIndex?: number;
  /** Key of an item in `onViewableItemsChanged` tokens (default: `item.key`, `item.id`, or the index). */
  keyExtractor?: (item: T, index: number) => string;
  onViewableItemsChanged?: (info: { viewableItems: ViewToken<T>[]; changed: ViewToken<T>[] }) => void;
  viewabilityConfig?: ViewabilityConfig;
  style?: StyleProp<ViewStyle>;
  showsIndicator?: boolean;
  refreshing?: boolean;
  onRefresh?: () => void;
  onEndReached?: () => void;
  /** How close to the end (in viewports) `onEndReached` fires; default 1. */
  onEndReachedThreshold?: number;
  onScrollEnd?: (e: { x: number; y: number }) => void;
  /** Native value fed with the scroll offset, for scroll-linked effects. */
  scrollValue?: NativeValue;
  /** Whether dragging the list dismisses the keyboard. */
  keyboardDismissMode?: "none" | "on-drag" | "interactive";
  ref?: YNode | ((node: YNode) => void);
  /** Renders one row. Called once per recycled cell; `item` and `index`
   * change when the cell is reused for another item. */
  children: (item: () => T, index: () => number) => JSX.Element;
}

/** Renders a list slot prop: a component is called, an element used as is. */
function slot(value: ListSlot): any {
  return typeof value === "function" && !(value instanceof YNode) ? untrack(() => createComponent(value, {})) : value;
}

/**
 * A virtualized, recycling list on a native scroll view. Only rows near the
 * viewport exist; the native side decides which, and cells are reused.
 * Positions, columns, sticky headers and viewability are computed natively.
 *
 * Rows are recycled: keep per-item state in your data (keyed by id), not in
 * signals created inside the row.
 */
export function List<T>(props: ListProps<T>): YNode {
  const list = createElement("list");
  const owner = getOwner();
  const cells: { node: YNode; setIndex: (i: number) => void; dispose: () => void }[] = [];

  // Tells the core how the items changed (by identity: the common start and
  // end stay), so measurements and cells follow their items and items added
  // above the viewport do not move what is on screen.
  let shown: readonly T[] = [];
  effect(() => {
    const next = props.each;
    const old = shown;
    shown = next;
    if (old.length > 0 && next !== old) {
      const max = Math.min(old.length, next.length);
      let start = 0;
      while (start < max && old[start] === next[start]) start++;
      let end = 0;
      while (end < max - start && old[old.length - 1 - end] === next[next.length - 1 - end]) end++;
      const removed = old.length - start - end;
      const inserted = next.length - start - end;
      if (removed > 0 || inserted > 0) setProp(list, "splice", [start, removed, inserted]);
    }
    setProp(list, "count", next.length);
  });
  effect(() => setProp(list, "estimatedItemSize", props.estimatedItemSize ?? 50));
  for (const name of [
    "style", "horizontal", "showsIndicator", "refreshing", "onRefresh", "onEndReached", "onScrollEnd", "scrollValue",
    "keyboardDismissMode", "numColumns", "inverted", "stickyHeaderIndices", "contentContainerStyle", "viewabilityConfig",
    "pagingEnabled", "snapToInterval", "snapToOffsets", "snapToAlignment", "decelerationRate", "scrollEnabled", "bounces",
    "contentInset", "contentOffset", "keyboardShouldPersistTaps", "scrollEventThrottle", "disableIntervalMomentum",
    "onScroll", "onScrollBeginDrag", "onScrollEndDrag", "onMomentumScrollBegin", "onMomentumScrollEnd",
  ] as const) {
    effect((prev: unknown) => {
      const value = (props as any)[name];
      if (value !== prev) setProp(list, name, value, prev);
      return value;
    });
  }
  // Not an event: `on*` props are handlers to the renderer.
  effect(() => setProp(list, "endReachedThreshold", props.onEndReachedThreshold ?? 1));

  // A new cell renders its first item right away: rows never see a
  // missing item.
  const createCell = (slotIndex: number, first: number) =>
    createRoot((dispose) => {
      const [index, setIndex] = createSignal(first);
      // Past the end (the data shrank before the core unbound the cell):
      // keep showing the last item; the core hides unbound cells.
      const item = createMemo((prev: T) => {
        const i = index();
        return i >= 0 && i < props.each.length ? props.each[i] : prev;
      }, first >= 0 && first < props.each.length ? props.each[first] : (undefined as T));
      const node = createElement("cell");
      setProp(node, "slot", slotIndex);
      insertNode(list, node);
      const content = untrack(() => props.children(item, index));
      if ("ItemSeparatorComponent" in props) {
        // Between items: after each one but the last (before it when
        // inverted, as the list runs upwards).
        let separator: any;
        const shown = createMemo(() => index() < props.each.length - 1 && props.ItemSeparatorComponent != null);
        insert(node, () => {
          if (!shown()) return content;
          separator ??= slot(props.ItemSeparatorComponent);
          return untrack(() => props.inverted) ? [separator, content] : [content, separator];
        });
      } else {
        insert(node, content);
      }
      return { node, setIndex, dispose };
    }, owner!);

  // Header and footer cells (always rendered), and the empty placeholder
  // while there are no items.
  const extraCell = (role: string, value: () => ListSlot) =>
    createRoot((dispose) => {
      const node = createElement("cell");
      setProp(node, "slot", role);
      insertNode(list, node);
      insert(node, () => slot(value()));
      return dispose;
    }, owner!);
  const extras: (() => void)[] = [];
  if ("ListHeaderComponent" in props) extras.push(extraCell("header", () => props.ListHeaderComponent));
  if ("ListFooterComponent" in props) extras.push(extraCell("footer", () => props.ListFooterComponent));
  if ("ListEmptyComponent" in props) {
    let disposeEmpty: (() => void) | null = null;
    let emptyNode: YNode | null = null;
    effect(() => {
      if (props.each.length === 0 && !disposeEmpty) {
        disposeEmpty = createRoot((dispose) => {
          emptyNode = createElement("cell");
          setProp(emptyNode, "slot", "empty");
          insertNode(list, emptyNode);
          insert(emptyNode, () => slot(props.ListEmptyComponent));
          return dispose;
        }, owner!);
      } else if (props.each.length > 0 && disposeEmpty) {
        disposeEmpty();
        detachNode(list, emptyNode!);
        disposeEmpty = emptyNode = null;
      }
    });
    extras.push(() => disposeEmpty?.());
  }

  if (props.onViewableItemsChanged) {
    let reported: [number, number] = [-1, -1];
    let timer: number | undefined;
    let latest: [number, number] = [-1, -1];
    const report = () => {
      timer = undefined;
      const [first, last] = latest;
      const [prevFirst, prevLast] = reported;
      if (first === prevFirst && last === prevLast) return;
      const each = props.each;
      const keyOf = props.keyExtractor;
      const token = (index: number, isViewable: boolean): ViewToken<T> => {
        const item = each[index];
        const key = keyOf ? keyOf(item, index) : String((item as any)?.key ?? (item as any)?.id ?? index);
        return { item, key, index, isViewable };
      };
      const viewableItems: ViewToken<T>[] = [];
      const changed: ViewToken<T>[] = [];
      if (first >= 0) for (let i = first; i <= last && i < each.length; i++) viewableItems.push(token(i, true));
      if (prevFirst >= 0) {
        for (let i = prevFirst; i <= prevLast && i < each.length; i++) {
          if (i < first || i > last) changed.push(token(i, false));
        }
      }
      for (const t of viewableItems) if (t.index < prevFirst || t.index > prevLast) changed.push(t);
      reported = latest;
      props.onViewableItemsChanged!({ viewableItems, changed });
    };
    setProp(list, "onViewableItemsChanged", (e: { first: number; last: number }) => {
      latest = [e.first, e.last];
      const wait = props.viewabilityConfig?.minimumViewTime ?? 0;
      if (wait <= 0) return report();
      if (timer !== undefined) clearTimeout(timer);
      timer = setTimeout(report, wait) as unknown as number;
    });
  }

  if (props.initialScrollIndex != null) {
    command(list.id, "scrollToIndex", { index: props.initialScrollIndex, animated: false });
  }
  if (typeof props.ref === "function") props.ref(list);

  lists.set(list.id, (flat) => {
    batch(() => {
      for (let i = 0; i < flat.length; i += 2) {
        const slotIndex = flat[i];
        const index = flat[i + 1];
        const cell = cells[slotIndex];
        if (cell) cell.setIndex(index);
        else cells[slotIndex] = createCell(slotIndex, index);
      }
    });
    // After the batch: the cells' new content is already in the op buffer.
    for (let i = 0; i < flat.length; i += 2) setProp(cells[flat[i]].node, "cellIndex", flat[i + 1]);
  });
  onCleanup(() => {
    lists.delete(list.id);
    for (const cell of cells) cell?.dispose();
    for (const dispose of extras) dispose();
  });
  return list;
}


(globalThis as any).__yf_list = (id: number, flat: number[]) => lists.get(id)?.(flat);

// --- Touches and the gesture responder system ------------------------------
//
// Native views whose node has a touch or responder handler listen to
// `onTouch`; their touches (and those on views inside them) arrive here with
// the path of listening views from the target outwards. Handlers then run
// as in React Native: touch handlers capture inwards and bubble outwards,
// and the responder is negotiated with the `…ShouldSetResponder` handlers.

const touchProps = new Set([
  "onTouchStart",
  "onTouchMove",
  "onTouchEnd",
  "onTouchCancel",
  "onTouchStartCapture",
  "onTouchMoveCapture",
  "onTouchEndCapture",
  "onTouchCancelCapture",
  "onStartShouldSetResponder",
  "onMoveShouldSetResponder",
  "onStartShouldSetResponderCapture",
  "onMoveShouldSetResponderCapture",
  "onResponderGrant",
  "onResponderReject",
  "onResponderStart",
  "onResponderMove",
  "onResponderEnd",
  "onResponderRelease",
  "onResponderTerminate",
  "onResponderTerminationRequest",
]);

function listensToTouches(handlers: Record<string, Handler>): boolean {
  for (const name in handlers) if (touchProps.has(name)) return true;
  return false;
}

interface NativeTouch extends TouchPoint {
  path: number[];
  changed: boolean;
}

interface NativeTouches {
  phase: "start" | "move" | "end" | "cancel";
  timestamp: number;
  points: NativeTouch[];
}

/** Listening views around each finger, innermost first. */
const touchPaths = new Map<number, YNode[]>();
let responder: YNode | null = null;
/** The responder keeps native scroll views from taking the touches. */
let responderBlocks = false;

type TouchEventImpl = GestureResponderEvent & { stopped: boolean };

function touchEvent(about: TouchPoint, touches: TouchPoint[], changed: TouchPoint[]): TouchEventImpl {
  const e = {
    identifier: about.identifier,
    locationX: about.locationX,
    locationY: about.locationY,
    pageX: about.pageX,
    pageY: about.pageY,
    target: about.target,
    timestamp: about.timestamp,
    touches,
    changedTouches: changed,
    stopped: false,
    stopPropagation() {
      e.stopped = true;
    },
  } as TouchEventImpl;
  e.nativeEvent = e;
  return e;
}

function call(node: YNode, name: string, e: GestureResponderEvent): unknown {
  const handler = node.handlers?.[name] as ((e: GestureResponderEvent) => unknown) | undefined;
  return handler?.(e);
}

/** `name` on `path` (innermost first): capture outside in, then bubble. */
function dispatchTwoPhase(path: YNode[], name: string, e: TouchEventImpl) {
  for (let i = path.length - 1; i >= 0 && !e.stopped; i--) call(path[i], name + "Capture", e);
  for (let i = 0; i < path.length && !e.stopped; i++) call(path[i], name, e);
}

function isAncestor(node: YNode, of: YNode): boolean {
  for (let n: YNode | null = of; n !== null; n = n.parent) if (n === node) return true;
  return false;
}

/** Asks `path` whether a view wants to become the responder (`kind` is
 * "onStartShouldSetResponder" or "onMoveShouldSetResponder"), from the
 * views around both the target and the current responder, the responder
 * itself excluded. */
function negotiate(kind: string, path: YNode[], e: TouchEventImpl) {
  let from = 0;
  if (responder !== null) {
    while (from < path.length && !isAncestor(path[from], responder)) from++;
    if (path[from] === responder) from++;
  }
  let wanting: YNode | null = null;
  for (let i = path.length - 1; i >= from && wanting === null; i--) {
    if (call(path[i], kind + "Capture", e) === true) wanting = path[i];
  }
  for (let i = from; i < path.length && wanting === null; i++) {
    if (call(path[i], kind, e) === true) wanting = path[i];
  }
  if (wanting === null || wanting === responder) return;
  if (responder !== null) {
    const handler = responder.handlers?.onResponderTerminationRequest as ((e: GestureResponderEvent) => boolean) | undefined;
    if (handler && handler(e) === false) {
      call(wanting, "onResponderReject", e);
      return;
    }
    call(responder, "onResponderTerminate", e);
  }
  grant(wanting, e);
}

function grant(node: YNode, e: GestureResponderEvent) {
  responder = node;
  const block = call(node, "onResponderGrant", e) === true;
  if (block) {
    command(node.id, "setJsResponder", true);
  } else if (responderBlocks) {
    command(1, "clearJsResponder");
  }
  responderBlocks = block;
}

function releaseResponder() {
  responder = null;
  if (responderBlocks) {
    responderBlocks = false;
    command(1, "clearJsResponder");
  }
}

function onTouch(native: NativeTouches) {
  const { phase, points } = native;
  for (const p of points) {
    if (phase === "start" && p.changed) {
      const path: YNode[] = [];
      for (const id of p.path) {
        const node = listening.get(id)?.deref();
        if (node) path.push(node);
      }
      touchPaths.set(p.identifier, path);
    }
  }
  const lifted = phase === "end" || phase === "cancel";
  const touches = lifted ? points.filter((p) => !p.changed) : points;
  const changed = points.filter((p) => p.changed);
  if (changed.length === 0) return;
  const about = changed[0];
  const path = touchPaths.get(about.identifier) ?? [];
  const e = touchEvent(about, touches, changed);
  const raw = { start: "onTouchStart", move: "onTouchMove", end: "onTouchEnd", cancel: "onTouchCancel" }[phase];
  dispatchTwoPhase(path, raw, e);
  e.stopped = false;
  if (phase === "start") negotiate("onStartShouldSetResponder", path, e);
  else if (phase === "move") negotiate("onMoveShouldSetResponder", path, e);
  if (responder !== null) {
    if (phase === "cancel") {
      call(responder, "onResponderTerminate", e);
      releaseResponder();
    } else {
      call(responder, phase === "start" ? "onResponderStart" : phase === "move" ? "onResponderMove" : "onResponderEnd", e);
      if (phase === "end" && touches.length === 0) {
        const node = responder;
        releaseResponder();
        call(node, "onResponderRelease", e);
      }
    }
  }
  if (lifted) for (const p of changed) touchPaths.delete(p.identifier);
}

let panStates = 0;

/** Pan gestures on top of the responder system (React Native's
 * `PanResponder`). */
export const PanResponder = {
  create(config: PanResponderCallbacks): PanResponderInstance {
    const gs: PanResponderGestureState = {
      stateID: ++panStates,
      moveX: 0,
      moveY: 0,
      x0: 0,
      y0: 0,
      dx: 0,
      dy: 0,
      vx: 0,
      vy: 0,
      numberActiveTouches: 0,
    };
    // The centroid and time of the last update, and the touches it had.
    let lastX = 0;
    let lastY = 0;
    let lastTime = 0;
    let lastCount = 0;
    const centroid = (touches: TouchPoint[]) => {
      let x = 0;
      let y = 0;
      for (const t of touches) {
        x += t.pageX;
        y += t.pageY;
      }
      const n = Math.max(1, touches.length);
      return [x / n, y / n];
    };
    const reset = (e: GestureResponderEvent) => {
      const [x, y] = centroid(e.touches);
      Object.assign(gs, { moveX: x, moveY: y, x0: x, y0: y, dx: 0, dy: 0, vx: 0, vy: 0 });
      gs.numberActiveTouches = e.touches.length;
      lastX = x;
      lastY = y;
      lastTime = e.timestamp;
      lastCount = e.touches.length;
    };
    const update = (e: GestureResponderEvent) => {
      if (e.timestamp === lastTime && e.touches.length === lastCount) return;
      const [x, y] = centroid(e.touches);
      gs.numberActiveTouches = e.touches.length;
      // A finger went down or up: the centroid jumps, which is no movement.
      if (e.touches.length === lastCount) {
        const dt = Math.max(1, e.timestamp - lastTime);
        gs.dx += x - lastX;
        gs.dy += y - lastY;
        // Smoothed over a few samples.
        gs.vx = gs.vx * 0.3 + ((x - lastX) / dt) * 0.7;
        gs.vy = gs.vy * 0.3 + ((y - lastY) / dt) * 0.7;
      }
      gs.moveX = x;
      gs.moveY = y;
      lastX = x;
      lastY = y;
      lastTime = e.timestamp;
      lastCount = e.touches.length;
    };
    return {
      panHandlers: {
        onStartShouldSetResponderCapture: (e) => {
          if (e.touches.length === 1) reset(e);
          gs.numberActiveTouches = e.touches.length;
          return config.onStartShouldSetPanResponderCapture?.(e, gs) ?? false;
        },
        onStartShouldSetResponder: (e) => config.onStartShouldSetPanResponder?.(e, gs) ?? false,
        onMoveShouldSetResponderCapture: (e) => {
          update(e);
          return config.onMoveShouldSetPanResponderCapture?.(e, gs) ?? false;
        },
        onMoveShouldSetResponder: (e) => {
          update(e);
          return config.onMoveShouldSetPanResponder?.(e, gs) ?? false;
        },
        onResponderGrant: (e) => {
          gs.x0 = gs.moveX = lastX = centroid(e.touches)[0];
          gs.y0 = gs.moveY = lastY = centroid(e.touches)[1];
          gs.dx = gs.dy = gs.vx = gs.vy = 0;
          lastTime = e.timestamp;
          lastCount = e.touches.length;
          config.onPanResponderGrant?.(e, gs);
          return config.onShouldBlockNativeResponder?.(e, gs) ?? true;
        },
        onResponderReject: (e) => config.onPanResponderReject?.(e, gs),
        onResponderStart: (e) => {
          update(e);
          config.onPanResponderStart?.(e, gs);
        },
        onResponderMove: (e) => {
          update(e);
          config.onPanResponderMove?.(e, gs);
        },
        onResponderEnd: (e) => {
          update(e);
          config.onPanResponderEnd?.(e, gs);
        },
        onResponderRelease: (e) => {
          config.onPanResponderRelease?.(e, gs);
          gs.stateID = ++panStates;
        },
        onResponderTerminate: (e) => {
          config.onPanResponderTerminate?.(e, gs);
          gs.stateID = ++panStates;
        },
        onResponderTerminationRequest: (e) => config.onPanResponderTerminationRequest?.(e, gs) ?? true,
      },
    };
  },
};

// Called by the engine for native input events.
(globalThis as any).__yf_dispatch = (id: number, name: string, payload: unknown) => {
  if (name === "onTouch") return onTouch(payload as NativeTouches);
  const node = listening.get(id)?.deref();
  const handler = node?.handlers?.[name];
  if (handler) handler(payload);
};
