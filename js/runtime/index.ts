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
import type { JSX, ScrollBehaviorProps, StyleProp, ViewProps, ViewStyle } from "./types";

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

// Sub-ops of OP_ANIM.
const ANIM_VALUE = 0; // node, value
const ANIM_INTERPOLATE = 1; // node, input, inputRange, outputRange, extrapolate
const ANIM_MATH = 2; // node, op, a, b
const ANIM_SET = 3; // node, value
const ANIM_START = 4; // node, animation id, config
const ANIM_STOP = 5; // node
const ANIM_RELEASE = 6; // node

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

  /** Unfocuses an `<input>`. */
  blur(): void {
    command(this.id, "blur");
  }

  /** Scrolls a `<scroll>` or `List` to an offset. */
  scrollTo(options: { x?: number; y?: number; animated?: boolean }): void {
    command(this.id, "scrollTo", options);
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

  /** Maps this node through piecewise-linear ranges (numbers or colors). */
  interpolate(config: {
    inputRange: number[];
    outputRange: number[] | string[];
    extrapolate?: "extend" | "clamp" | "identity";
  }): NativeNode {
    const node = new NativeNode([this]);
    ops.push(OP_ANIM, ANIM_INTERPOLATE, node.__nv, this.__nv, config.inputRange, config.outputRange, config.extrapolate ?? "clamp");
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
}

function math(op: string, a: Operand, b: Operand): NativeNode {
  const node = new NativeNode([a, b].filter((x): x is NativeNode => x instanceof NativeNode));
  ops.push(OP_ANIM, ANIM_MATH, node.__nv, op, a, b);
  schedule();
  return node;
}

export type Easing = "linear" | "easeIn" | "easeOut" | "easeInOut" | [number, number, number, number];

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

// --- Pressable --------------------------------------------------------------

export interface PressableState {
  pressed: boolean;
}

export interface PressableProps extends Omit<ViewProps, "style" | "children"> {
  style?: StyleProp<ViewStyle> | ((state: PressableState) => StyleProp<ViewStyle>);
  children?: JSX.Element | ((state: PressableState) => JSX.Element);
}

/**
 * A view that tracks whether it is pressed: `style` and `children` may be
 * functions of `{ pressed }`. For plain opacity feedback without JS, use
 * `<view onPress activeOpacity={0.5}>`.
 */
export function Pressable(props: PressableProps): YNode {
  const [pressed, setPressed] = createSignal(false);
  const [local, others] = splitProps(props, ["style", "children", "onPressIn", "onPressOut"]);
  const node = createElement("view");
  spread(
    node,
    mergeSolidProps(others, {
      get style() {
        const s = local.style;
        return typeof s === "function" ? s({ pressed: pressed() }) : s;
      },
      onPressIn() {
        setPressed(true);
        local.onPressIn?.();
      },
      onPressOut() {
        setPressed(false);
        local.onPressOut?.();
      },
    }),
    true,
  );
  insert(node, () => {
    const c = local.children;
    return typeof c === "function" && c.length === 1 ? c({ pressed: pressed() }) : c;
  });
  return node;
}

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

/** A style prop as one object: arrays are merged left to right, falsy
 * entries skipped (as the core does for `style`). */
function flattenStyle(style: StyleProp<ViewStyle>): Record<string, any> {
  if (!Array.isArray(style)) return (style as Record<string, any>) ?? {};
  const out: Record<string, any> = {};
  for (const s of style) if (s) Object.assign(out, flattenStyle(s as StyleProp<ViewStyle>));
  return out;
}

/** The padding a style sets on one edge (`paddingTop`, `paddingVertical`,
 * `padding`), in points. */
function edgePadding(s: Record<string, any>, edge: "top" | "right" | "bottom" | "left"): number {
  const key = `padding${edge[0].toUpperCase()}${edge.slice(1)}`;
  const axis = edge === "top" || edge === "bottom" ? "paddingVertical" : "paddingHorizontal";
  const v = s[key] ?? s[axis] ?? s.padding ?? 0;
  return typeof v === "number" ? v : 0;
}

/** A view padded by the safe-area insets where it renders. */
export function SafeAreaView(props: SafeAreaViewProps): YNode {
  const insets = useSafeAreaInsets();
  const [local, others] = splitProps(props, ["style", "edges", "children"]);
  const node = createElement("view");
  const pad = (edge: "top" | "right" | "bottom" | "left", _key: string) => {
    const own = edgePadding(flattenStyle(local.style), edge);
    const on = !local.edges || local.edges.includes(edge);
    return own + (on ? insets()[edge] : 0);
  };
  spread(
    node,
    mergeSolidProps(others, {
      get style() {
        return {
          ...flattenStyle(local.style),
          paddingTop: pad("top", "paddingTop"),
          paddingRight: pad("right", "paddingRight"),
          paddingBottom: pad("bottom", "paddingBottom"),
          paddingLeft: pad("left", "paddingLeft"),
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
}

/**
 * A view that keeps its content above the software keyboard by padding its
 * bottom by the keyboard's height, animated with the keyboard. For content
 * outside scroll views, such as a message bar at the bottom of a screen
 * (inputs inside scroll views are kept visible natively). It assumes it
 * reaches the bottom of the surface, as a screen's content does. Where the
 * window shrinks for the keyboard instead (an Android host that resizes),
 * the keyboard covers nothing and the padding stays 0.
 */
export function KeyboardAvoidingView(props: KeyboardAvoidingViewProps): YNode {
  const [local, others] = splitProps(props, ["style", "enabled", "children"]);
  const node = createElement("view");
  const lift = createMemo(() => (local.enabled === false ? 0 : keyboard().height));
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
  spread(
    node,
    mergeSolidProps(others, {
      get style() {
        const s = flattenStyle(local.style);
        return { ...s, paddingBottom: edgePadding(s, "bottom") + lift() };
      },
    }),
    true,
  );
  insert(node, () => local.children);
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

export interface ModalProps {
  /** Shown while true (default). */
  visible?: boolean;
  animationType?: "slide" | "fade" | "none";
  /** iOS: `"pageSheet"` / `"formSheet"` keep what is behind visible. */
  presentationStyle?: "fullScreen" | "pageSheet" | "formSheet";
  /** The background shows what is behind. */
  transparent?: boolean;
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

export interface SwitchProps extends Omit<ViewProps, "children" | "onLoad" | "onError"> {
  /** Controlled: the switch shows this; flip it in `onValueChange`. */
  value?: boolean;
  onValueChange?: (value: boolean) => void;
  disabled?: boolean;
  trackColor?: { false?: string; true?: string };
  thumbColor?: string;
}

/** The platform's on / off switch. */
export const Switch: (props: SwitchProps) => YNode = hostComponent<SwitchProps>("switch");

export interface SliderProps extends Omit<ViewProps, "children" | "onLoad" | "onError"> {
  value?: number;
  minimumValue?: number;
  maximumValue?: number;
  /** 0 is continuous. */
  step?: number;
  disabled?: boolean;
  minimumTrackTintColor?: string;
  maximumTrackTintColor?: string;
  thumbTintColor?: string;
  /** While sliding. */
  onValueChange?: (value: number) => void;
  /** When the user lets go. */
  onSlidingComplete?: (value: number) => void;
}

/** The platform's slider. */
export const Slider: (props: SliderProps) => YNode = hostComponent<SliderProps>("slider");

export interface ActivityIndicatorProps extends Omit<ViewProps, "children" | "onLoad" | "onError"> {
  animating?: boolean;
  color?: string;
  size?: "small" | "large";
  hidesWhenStopped?: boolean;
}

/** The platform's spinner. */
export const ActivityIndicator: (props: ActivityIndicatorProps) => YNode = hostComponent<ActivityIndicatorProps>("activityindicator");

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

/** A section of a `SectionList`. */
export interface Section<T> {
  data: readonly T[];
  [key: string]: any;
}

type SectionRow<S extends Section<unknown>> =
  | { header: true; section: S }
  | { header: false; section: S; item: S["data"][number]; index: number };

export interface SectionListProps<S extends Section<unknown>>
  extends Omit<ListProps<SectionRow<S>>, "each" | "children" | "stickyHeaderIndices"> {
  sections: readonly S[];
  /** Section headers stick to the top while their section scrolls (default true). */
  stickySectionHeadersEnabled?: boolean;
  renderSectionHeader: (section: () => S) => JSX.Element;
  /** `index` is the item's index within its section. */
  renderItem: (item: () => S["data"][number], index: () => number, section: () => S) => JSX.Element;
}

/**
 * A `List` of sections, each with a header (sticky by default) and its
 * items. Header and item rows share the recycled cells.
 */
export function SectionList<S extends Section<unknown>>(props: SectionListProps<S>): YNode {
  const flat = createMemo(() => {
    const rows: SectionRow<S>[] = [];
    const headers: number[] = [];
    for (const section of props.sections) {
      headers.push(rows.length);
      rows.push({ header: true, section });
      section.data.forEach((item, index) => rows.push({ header: false, section, item, index }));
    }
    return { rows, headers };
  });
  const [, rest] = splitProps(props, ["sections", "stickySectionHeadersEnabled", "renderSectionHeader", "renderItem"]);
  return List(
    mergeSolidProps(rest, {
      get each() {
        return flat().rows;
      },
      get stickyHeaderIndices() {
        return props.stickySectionHeadersEnabled === false ? undefined : flat().headers;
      },
      children: (row: () => SectionRow<S>) => {
        const header = createMemo(() => row().header);
        return createMemo(() =>
          header()
            ? untrack(() => props.renderSectionHeader(() => row().section))
            : untrack(() =>
                props.renderItem(
                  () => (row() as any).item,
                  () => (row() as any).index,
                  () => row().section,
                ),
              ),
        );
      },
    }) as ListProps<SectionRow<S>>,
  );
}

(globalThis as any).__yf_list = (id: number, flat: number[]) => lists.get(id)?.(flat);

// Called by the engine for native input events.
(globalThis as any).__yf_dispatch = (id: number, name: string, payload: unknown) => {
  const node = listening.get(id)?.deref();
  const handler = node?.handlers?.[name];
  if (handler) handler(payload);
};
