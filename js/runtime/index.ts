// yf runtime: the Solid universal renderer that turns JSX into a flat op
// buffer for the Rust core.
//
// Performance rules (QuickJS is a plain interpreter, the slowest part of the
// pipeline): do the minimum here. No parsing, no diffing, no string building.
// Push raw values into one reused array and let Rust decode them.
//
// The opcode values must match `crates/yf-js/src/protocol.rs`.

import { batch, createMemo, createRoot, createSignal, getOwner, onCleanup, untrack } from "solid-js";
import { createRenderer } from "solid-js/universal";

const OP_CREATE = 0; // id, tag
const OP_CREATE_TEXT = 1; // id, text
const OP_SET_TEXT = 2; // id, text
const OP_SET_PROP = 3; // id, name, value
const OP_INSERT = 4; // parent, child, before (0 = append)
const OP_REMOVE = 5; // parent, child
const OP_RELEASE = 6; // id
const OP_ANIM = 7; // sub-op, node, args...

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
  /** Native animation nodes used by this view's style: kept alive with it. */
  anims: NativeNode[] | null = null;
  readonly isText: boolean;

  constructor(id: number, isText: boolean) {
    this.id = id;
    this.isText = isText;
  }
}

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
export const root = new YNode(1, false);

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

export const {
  render,
  effect,
  memo,
  createComponent,
  createElement,
  createTextNode,
  insertNode,
  insert,
  spread,
  setProp,
  mergeProps,
  use,
} = createRenderer<YNode>({
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
      if (name === "style" || name === "pan" || name === "scrollValue") retainAnims(node, value);
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
  removeNode(parent: YNode, node: YNode) {
    unlink(node);
    ops.push(OP_REMOVE, parent.id, node.id);
    schedule();
  },
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

// --- Native animation values -------------------------------------------

let nextAnimNode = 1;
let nextAnimation = 1;
const animationEnds = new Map<number, (finished: boolean) => void>();
const animRegistry = new FinalizationRegistry<number>((node) => {
  ops.push(OP_ANIM, ANIM_RELEASE, node);
  schedule();
});

/** Collects the native nodes referenced by a style / prop value. */
function retainAnims(node: YNode, value: any) {
  let found: NativeNode[] | null = null;
  const visit = (v: any) => {
    if (v instanceof NativeNode) (found ??= []).push(v);
    else if (Array.isArray(v)) v.forEach(visit);
    else if (v !== null && typeof v === "object") for (const k in v) visit(v[k]);
  };
  visit(value);
  node.anims = found;
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
  set(value: number) {
    ops.push(OP_ANIM, ANIM_SET, this.__nv, value);
    schedule();
  }

  /** Stops the running animation where it is. */
  stop() {
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

// --- Virtualized list ----------------------------------------------------

type ListBinder = (flat: number[]) => void;
const lists = new Map<number, ListBinder>();

export interface ListProps<T> {
  each: readonly T[];
  /** Size used for items not measured yet (points). */
  estimatedItemSize?: number;
  horizontal?: boolean;
  style?: any;
  showsIndicator?: boolean;
  refreshing?: boolean;
  onRefresh?: () => void;
  onEndReached?: () => void;
  onScrollEnd?: (e: { x: number; y: number }) => void;
  /** Native value fed with the scroll offset, for scroll-linked effects. */
  scrollValue?: NativeValue;
  /** Renders one row. Called once per recycled cell; `item` and `index`
   * change when the cell is reused for another item. */
  children: (item: () => T, index: () => number) => any;
}

/**
 * A virtualized, recycling list on a native scroll view. Only rows near the
 * viewport exist; the native side decides which, and cells are reused.
 *
 * Rows are recycled: keep per-item state in your data (keyed by id), not in
 * signals created inside the row.
 */
export function List<T>(props: ListProps<T>): YNode {
  const list = createElement("list");
  const owner = getOwner();
  const cells: { node: YNode; setIndex: (i: number) => void; dispose: () => void }[] = [];

  effect(() => setProp(list, "count", props.each.length));
  effect(() => setProp(list, "estimatedItemSize", props.estimatedItemSize ?? 50));
  for (const name of ["style", "horizontal", "showsIndicator", "refreshing", "onRefresh", "onEndReached", "onScrollEnd", "scrollValue"] as const) {
    effect((prev: unknown) => {
      const value = (props as any)[name];
      if (value !== prev) setProp(list, name, value, prev);
      return value;
    });
  }

  // A new cell renders its first item right away: rows never see a
  // missing item.
  const createCell = (slot: number, first: number) =>
    createRoot((dispose) => {
      const [index, setIndex] = createSignal(first);
      const item = createMemo(() => {
        const i = index();
        return i >= 0 ? props.each[i] : (undefined as T);
      });
      const node = createElement("cell");
      setProp(node, "slot", slot);
      insertNode(list, node);
      insert(node, untrack(() => props.children(item, index)));
      return { node, setIndex, dispose };
    }, owner!);

  lists.set(list.id, (flat) => {
    batch(() => {
      for (let i = 0; i < flat.length; i += 2) {
        const slot = flat[i];
        const index = flat[i + 1];
        const cell = cells[slot];
        if (cell) cell.setIndex(index);
        else cells[slot] = createCell(slot, index);
      }
    });
    // After the batch: the cells' new content is already in the op buffer.
    for (let i = 0; i < flat.length; i += 2) setProp(cells[flat[i]].node, "cellIndex", flat[i + 1]);
  });
  onCleanup(() => {
    lists.delete(list.id);
    for (const cell of cells) cell.dispose();
  });
  return list;
}

(globalThis as any).__yf_list = (id: number, flat: number[]) => lists.get(id)?.(flat);

// Called by the engine for native input events.
(globalThis as any).__yf_dispatch = (id: number, name: string, payload: unknown) => {
  const node = listening.get(id)?.deref();
  const handler = node?.handlers?.[name];
  if (handler) handler(payload);
};
