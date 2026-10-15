// `nsh/animated`: native values, the animation graph and layout animations.
// Everything runs on the UI thread; JS only describes it.

import { command, OP_ANIM, ops, schedule } from "./_core";

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

// --- Native animation values -------------------------------------------

let nextAnimNode = 1;
let nextAnimation = 1;
const animationEnds = new Map<number, (finished: boolean) => void>();
const animRegistry = new FinalizationRegistry<number>((node) => {
  ops.push(OP_ANIM, ANIM_RELEASE, node);
  schedule();
});


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

(globalThis as any).__nsh_anim_end = (id: number, finished: boolean) => {
  const resolve = animationEnds.get(id);
  if (resolve) {
    animationEnds.delete(id);
    resolve(finished);
  }
};

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
