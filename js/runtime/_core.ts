// nsh runtime core (internal; apps import the feature modules): the Solid
// universal renderer that turns JSX into a flat op buffer for the Rust core,
// the environment, and the hooks the engine calls.
//
// Performance rules (QuickJS is a plain interpreter, the slowest part of the
// pipeline): do the minimum here. No parsing, no diffing, no string building.
// Push raw values into one reused array and let Rust decode them.
//
// The opcode values must match `crates/nsh-js/src/protocol.rs`.

import {
  type Accessor,
  createContext,
  createMemo,
  createRenderEffect,
  createRoot,
  createSignal,
  untrack,
} from "solid-js";
import { createRenderer } from "solid-js/universal";
// The app's Tailwind theme (`nsh.json`), JSON or null.
import tailwindTheme from "nsh:tailwind-theme";
import type { GestureResponderEvent, TouchPoint } from "./types";

export const OP_CREATE = 0; // id, tag
export const OP_CREATE_TEXT = 1; // id, text
export const OP_SET_TEXT = 2; // id, text
export const OP_SET_PROP = 3; // id, name, value
export const OP_INSERT = 4; // parent, child, before (0 = append)
export const OP_REMOVE = 5; // parent, child
export const OP_RELEASE = 6; // id
export const OP_ANIM = 7; // sub-op, node, args...
export const OP_COMMAND = 8; // id, name, argument
export const OP_DRAW = 9; // id, [canvas operations (CV_*)]

declare function __nsh_commit(ops: unknown[]): void;

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
  anims: Record<string, object[]> | null = null;
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

  /** Calls command `name` of a registered native component
   * (`nsh/native`) with `args` (any JSON value). */
  command(name: string, args?: unknown): void {
    command(this.id, "native", { name, args });
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

export function detachNode(parent: YNode, node: YNode): void {
  unlink(node);
  ops.push(OP_REMOVE, parent.id, node.id);
  schedule();
}

/** Sends `name` to node `id`'s native view (after the commit's other changes). */
export function command(id: number, name: string, argument?: unknown): void {
  ops.push(OP_COMMAND, id, name, argument);
  schedule();
}

export const ops: unknown[] = [];
let scheduled = false;
let nextId = 2; // 1 is the root view, owned by the platform.

export function schedule(): void {
  if (!scheduled) {
    scheduled = true;
    queueMicrotask(flush);
  }
}

function flush() {
  scheduled = false;
  if (ops.length > 0) {
    __nsh_commit(ops);
    ops.length = 0;
  }
}

// Nodes with listeners, for event dispatch. Weak so handlers never keep a
// collected node alive.
export const listening: Map<number, WeakRef<YNode>> = new Map();

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
Object.defineProperty(globalThis, "__nsh", { value: { root, registry, listening } });

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
  setProperty(node: YNode, name: string, value: any, prev?: any) {
    if (
      (name === "ios" || name === "android" || name === "desktop") &&
      (value === null || typeof value === "object" || value === undefined)
    ) {
      platformProps(node, name, value, prev);
      return;
    }
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
  (globalThis as any).__nsh_hot?.rendered(dispose);
  return dispose;
}

/** `ios={{…}}` / `android={{…}}` / `desktop={{…}}`: the props of this
 * platform only, each set (or cleared) on its own; the other platforms'
 * objects are ignored. */
function platformProps(node: YNode, name: "ios" | "android" | "desktop", value: any, prev: any) {
  if (name === "desktop" ? !desktop : name !== os) return;
  if (prev) {
    for (const key in prev) if (!value || !(key in value)) renderer.setProp(node, key, undefined, prev[key]);
  }
  if (value) for (const key in value) renderer.setProp(node, key, value[key], prev?.[key]);
}

/** A component rendering intrinsic element `tag` with its props. */
export function hostComponent<P>(tag: string): (props: P) => YNode {
  return (props: P) => {
    const node = createElement(tag);
    spread(node, props as any, true);
    return node;
  };
}

/** Keeps the native animation nodes referenced by a style / prop value alive
 * with the view (`nsh/animated` nodes carry a numeric `__nv`). */
function retainAnims(node: YNode, name: string, value: any) {
  let found: object[] | null = null;
  const visit = (v: any) => {
    if (v !== null && typeof v === "object") {
      if (typeof v.__nv === "number") (found ??= []).push(v);
      else if (Array.isArray(v)) v.forEach(visit);
      else for (const k in v) visit(v[k]);
    }
  };
  visit(value);
  if (found) (node.anims ??= {})[name] = found;
  else if (node.anims) delete node.anims[name];
}

// --- Environment ----------------------------------------------------------

export interface Insets {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

/** What the platform reports about the surface; see the accessors below. */
/** An operating system (`Platform.OS`). */
export type OS = "ios" | "android" | "macos" | "windows" | "linux";

export interface Environment {
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
  os: OS;
  osVersion: string | number;
  idiom: "phone" | "pad" | "mac" | "tv" | "desktop";
}

const [environmentSignal, setEnvironment] = createSignal<Environment>((globalThis as any).__nsh_env_value, {
  equals: false,
});
/** Everything the platform reports (reactive). */
export const environment: Accessor<Environment> = environmentSignal;
(globalThis as any).__nsh_env = (env: Environment) => setEnvironment(env);

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

/** The environment when the bundle started (platform constants). */
export const initialEnvironment: Environment = untrack(environment);
/** The platform this bundle runs on. */
export const os: OS = initialEnvironment.os;
/** It is a desktop OS (`desktop={{…}}` props apply). */
export const desktop: boolean = os === "macos" || os === "windows" || os === "linux";

// --- Right to left ------------------------------------------------------------

const [allowRTL, setAllowRTLSignal] = createSignal(true);
const [forceRTL, setForceRTLSignal] = createSignal(false);
/** `I18nManager.allowRTL`. */
export const setAllowRTL = (allow: boolean): void => {
  setAllowRTLSignal(allow);
};
/** `I18nManager.forceRTL`. */
export const setForceRTL = (force: boolean): void => {
  setForceRTLSignal(force);
};
/** Whether the surface lays out right to left (reactive). */
export const isRTL: Accessor<boolean> = createRoot(() => {
  const rtl = createMemo(() => (allowRTL() && environment().rtl) || forceRTL());
  // The surface's direction; the core lays every view out by it.
  createRenderEffect(() => {
    ops.push(OP_SET_PROP, 1, "layoutDirectionRtl", rtl());
    schedule();
  });
  return rtl;
});

// --- Safe areas -------------------------------------------------------------

/** The insets of the nearest screen or modal (the surface's safe area outside
 * of them). */
export const InsetsContext: ReturnType<typeof createContext<Accessor<Insets>>> = createContext<Accessor<Insets>>(() =>
  safeArea(),
);

/** Keeps the insets a screen or modal reports (`onInsets`). */
export function nativeInsets(initial: () => Insets): { insets: Accessor<Insets>; onInsets: (e: Insets) => void } {
  const [insets, setInsets] = createSignal<Insets | null>(null, {
    equals: (a, b) => a !== null && b !== null && a.top === b.top && a.right === b.right && a.bottom === b.bottom && a.left === b.left,
  });
  return {
    insets: (() => insets() ?? initial()) as Accessor<Insets>,
    onInsets: (e: Insets) => setInsets({ top: e.top, right: e.right, bottom: e.bottom, left: e.left }),
  };
}

// --- Status bar -------------------------------------------------------------

/** What the status bar shows (the command's payload). */
export interface StatusBarState {
  barStyle?: "default" | "light-content" | "dark-content";
  hidden?: boolean;
  animated?: boolean;
  backgroundColor?: string;
}

/** Status bar settings by owner: rendered `<StatusBar>`s (the last mounted
 * wins, prop by prop) over the imperative ones. */
export const statusBarStack: (() => StatusBarState)[] = [];
const [statusBarBase, setStatusBarBase] = createSignal<StatusBarState>({});
const [statusBarVersion, setStatusBarVersionSignal] = createSignal(0);
/** A rendered `<StatusBar>` changed: merge again. */
export const bumpStatusBar = (): void => {
  setStatusBarVersionSignal((v) => v + 1);
};
let statusBarStarted = false;

export function startStatusBar(): void {
  if (statusBarStarted) return;
  statusBarStarted = true;
  createRoot(() => {
    let last = "";
    effect(() => {
      statusBarVersion();
      const merged: StatusBarState = { ...statusBarBase() };
      for (const entry of statusBarStack) {
        const p = entry();
        for (const key of Object.keys(p) as (keyof StatusBarState)[]) {
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

export const setStatusBar = (patch: StatusBarState): void => {
  startStatusBar();
  setStatusBarBase((b) => ({ ...b, ...patch }));
};

// --- Touches and the gesture responder system ------------------------------
//
// Native views whose node has a touch or responder handler listen to
// `onTouch`; their touches (and those on views inside them) arrive here with
// the path of listening views from the target outwards. Handlers then run
// as in React Native: touch handlers capture inwards and bubble outwards,
// and the responder is negotiated with the `…ShouldSetResponder` handlers.

const touchProps: Set<string> = new Set([
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

// Called by the engine for native input events.
(globalThis as any).__nsh_dispatch = (id: number, name: string, payload: unknown) => {
  if (name === "onTouch") return onTouch(payload as NativeTouches);
  const node = listening.get(id)?.deref();
  const handler = node?.handlers?.[name];
  if (handler) handler(payload);
};
