// `nsh/native`: components the embedding app registers natively (a map, a
// player, its own views), used like any other component.

import { mergeProps as mergeSolidProps, splitProps } from "solid-js";
import { createElement, setProp, spread, type YNode } from "./_core";
import type { ViewProps } from "./types";

/** Handlers of a native component's events: `regionChange` -> `onRegionChange`. */
export type NativeEventHandlers<E> = {
  [K in keyof E & string as `on${Capitalize<K>}`]?: (data: E[K]) => void;
};

/** The props of a native component: a view's (style, classes, layout,
 * accessibility, `ref`), its own `P` and handlers of its events `E`. */
export type NativeComponentProps<P, E> = Omit<ViewProps, "children"> & P & NativeEventHandlers<E>;

/** What a native component takes and emits. */
export interface NativeComponentSpec<P, E> {
  /** Its own props: sent to the component (any JSON value), each when it
   * changes. */
  props: readonly (keyof P & string)[];
  /** The events it emits. */
  events?: readonly (keyof E & string)[];
}

const handlerName = (event: string): string => "on" + event.charAt(0).toUpperCase() + event.slice(1);

/**
 * A component for the native component the embedding app registered as
 * `name` (Swift `NshAppView.register(component:)`, Java
 * `NshView.registerComponent`). It is laid out like a view, at the size its
 * style gives it, else at the size the component reports. Its `ref` calls
 * commands: `ref.command("zoomTo", { level: 3 })`. Where nothing is
 * registered under `name`, it renders an empty view.
 */
export function nativeComponent<P extends object = {}, E extends object = {}>(
  name: string,
  spec: NativeComponentSpec<P, E>,
): (props: NativeComponentProps<P, E>) => YNode {
  const own: string[] = [...spec.props];
  const events: string[] = [...(spec.events ?? [])];
  const handlers = events.map(handlerName);
  return (props: NativeComponentProps<P, E>): YNode => {
    const node = createElement("native");
    setProp(node, "component", name);
    const [mine, on, view] = splitProps(props as any, own, handlers);
    const values: Record<string, unknown> = {};
    for (const key of own) {
      Object.defineProperty(values, "$" + key, { enumerable: true, get: () => mine[key] });
    }
    const listener =
      events.length > 0
        ? { onNativeEvent: (e: { name: string; data: unknown }) => on[handlerName(e.name)]?.(e.data) }
        : {};
    spread(node, mergeSolidProps(view, values, listener), true);
    return node;
  };
}
