// `nsh/accessibility`: screen readers and motion settings.

import { type Accessor, createRenderEffect, createRoot, untrack } from "solid-js";
import { command, reduceMotion, screenReaderEnabled, type YNode } from "./_core";

export { reduceMotion, screenReaderEnabled } from "./_core";

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
