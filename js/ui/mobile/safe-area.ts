// `nsh/ui/mobile`: content kept clear of system UI and headers.

import { type Accessor, mergeProps as mergeSolidProps, splitProps, useContext } from "solid-js";
import { createElement, type Insets, insert, InsetsContext, spread, type YNode } from "nsh/_core";
import type { ViewProps } from "nsh";

export type { Insets } from "nsh/_core";
export { safeArea } from "nsh/_core";

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
