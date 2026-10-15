// `nsh/canvas`: a view drawn with an HTML Canvas 2D context, replayed
// natively.

import { createRenderEffect, createSignal, mergeProps as mergeSolidProps, splitProps } from "solid-js";
import { createElement, OP_DRAW, ops, schedule, spread, type YNode } from "./_core";
import { NativeNode } from "./animated";
import type { ImageAsset, LayoutEvent, ViewProps } from "./types";

// --- Canvas ---------------------------------------------------------------------

// Canvas operations: `crates/nsh-js/src/canvas.rs`.
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

declare function __nsh_measure_text(font: string, text: string): number;

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
    return { width: __nsh_measure_text(this.s.font, String(text)) };
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
