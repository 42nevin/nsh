// `nsh/android`: what only Android has. Importable everywhere (one bundle
// runs on both platforms); elsewhere it does nothing.

import { command, os, root, setProp, setStatusBar } from "./_core";

/** True on Android. */
export const isAvailable: boolean = os === "android";

/** Short messages at the bottom of the screen. */
export const Toast = {
  SHORT: 0 as const,
  LONG: 1 as const,
  show(message: string, duration: 0 | 1 = 0): void {
    command(1, "toast", { text: message, long: duration === 1 });
  },
};

// --- Back button (Android) ---------------------------------------------------

const backListeners: (() => boolean | void)[] = [];

function onBackPress() {
  for (let i = backListeners.length - 1; i >= 0; i--) {
    if (backListeners[i]()) return;
  }
  BackHandler.exitApp();
}

/** The status bar's color (before edge-to-edge, API 35); `<StatusBar
 * android={{ backgroundColor }}>` is the declarative form. */
export function setStatusBarBackgroundColor(backgroundColor: string, animated?: boolean): void {
  setStatusBar({ backgroundColor, animated });
}

/** The back button, after modals and stacks had their turn. */
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
  /** Leaves the app. */
  exitApp(): void {
    command(1, "exitApp");
  },
};
