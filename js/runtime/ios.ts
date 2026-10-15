// `nsh/ios`: what only iOS has. Importable everywhere (one bundle runs on
// both platforms); elsewhere it does nothing.

import { createElement, insert, os, spread, type YNode } from "./_core";
import type { ColorValue, JSX, StyleProp, ViewStyle } from "./types";

/** True on iOS (and Mac Catalyst). */
export const isAvailable: boolean = os === "ios";

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
 * in the tree, and nothing at all on Android.
 */
export function InputAccessoryView(props: InputAccessoryViewProps): YNode | null {
  if (!isAvailable) return null;
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
