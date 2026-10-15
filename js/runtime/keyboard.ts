// `nsh/keyboard`: the software keyboard.

import { createMemo, mergeProps as mergeSolidProps, splitProps, untrack } from "solid-js";
import { command, createElement, effect, insert, keyboard, spread, type YNode } from "./_core";
import { LayoutAnimation } from "./animated";
import type { StyleProp, ViewProps, ViewStyle } from "./types";

export { keyboard } from "./_core";

/** The software keyboard. */
export const Keyboard = {
  /** Unfocuses the focused input, hiding the keyboard. */
  dismiss(): void {
    command(1, "blur");
  },
};

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
