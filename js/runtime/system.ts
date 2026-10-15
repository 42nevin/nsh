// `nsh/system`: the status bar and native dialogs.

import { onCleanup } from "solid-js";
import {
  bumpStatusBar,
  command,
  effect,
  root,
  setProp,
  setStatusBar,
  startStatusBar,
  statusBarStack,
} from "./_core";

export type StatusBarStyle = "default" | "light-content" | "dark-content";

export interface StatusBarProps {
  barStyle?: StatusBarStyle;
  hidden?: boolean;
  /** Animate changes of `barStyle` and `hidden`. */
  animated?: boolean;
  android?: {
    /** The bar's color (before edge-to-edge, API 35). */
    backgroundColor?: string;
  };
}

/** `<StatusBar>` and its imperative methods. */
export interface StatusBarComponent {
  (props: StatusBarProps): null;
  setBarStyle(barStyle: StatusBarStyle, animated?: boolean): void;
  setHidden(hidden: boolean, animation?: "none" | "fade" | "slide"): void;
}

/**
 * The status bar: render `<StatusBar barStyle="light-content" />` in a
 * screen (the most recently mounted one wins while it is mounted), or call
 * `StatusBar.setBarStyle(...)`. In an app embedded in a native one, the
 * host app's view controller decides on iOS.
 */
export const StatusBar: StatusBarComponent = Object.assign(
  (props: StatusBarProps): null => {
    startStatusBar();
    const entry = () => ({
      barStyle: props.barStyle,
      hidden: props.hidden,
      animated: props.animated,
      backgroundColor: props.android?.backgroundColor,
    });
    statusBarStack.push(entry);
    effect(() => {
      entry();
      bumpStatusBar();
    });
    onCleanup(() => {
      const i = statusBarStack.indexOf(entry);
      if (i >= 0) statusBarStack.splice(i, 1);
      bumpStatusBar();
    });
    return null;
  },
  {
    setBarStyle: (barStyle: StatusBarStyle, animated?: boolean): void => setStatusBar({ barStyle, animated }),
    setHidden: (hidden: boolean, animation?: "none" | "fade" | "slide"): void =>
      setStatusBar({ hidden, animated: animation !== undefined && animation !== "none" }),
  },
);

export interface AlertButton {
  text?: string;
  onPress?: () => void;
  style?: "default" | "cancel" | "destructive";
}

export interface AlertOptions {
  onDismiss?: () => void;
  android?: {
    /** Tapping outside or the back button closes it (default false). */
    cancelable?: boolean;
  };
}

let nextDialog = 1;
const dialogs = new Map<number, (index: number) => void>();

/** Shows a native dialog; `answer` gets the button index, or -1. */
function showDialog(spec: Record<string, unknown>, answer: (index: number) => void) {
  const id = nextDialog++;
  if (dialogs.size === 0) setProp(root, "onDialog", onDialog);
  dialogs.set(id, answer);
  command(1, "dialog", { ...spec, id });
}

function onDialog(e: { id: number; index: number }) {
  const answer = dialogs.get(e.id);
  dialogs.delete(e.id);
  if (dialogs.size === 0) setProp(root, "onDialog", undefined);
  answer?.(e.index);
}

/** Native alerts (UIAlertController / AlertDialog). */
export const Alert = {
  /** At most three buttons on Android; "OK" when none is given. */
  alert(title: string, message?: string, buttons?: AlertButton[], options?: AlertOptions): void {
    const list = buttons && buttons.length > 0 ? buttons : [{ text: "OK" }];
    showDialog(
      {
        title,
        message,
        buttons: list.map((b) => ({ text: b.text ?? "OK", style: b.style ?? "default" })),
        cancelable: options?.android?.cancelable ?? false,
      },
      (index) => {
        if (index >= 0) list[index]?.onPress?.();
        else options?.onDismiss?.();
      },
    );
  },
};

export interface ActionSheetOptions {
  options: string[];
  cancelButtonIndex?: number;
  destructiveButtonIndex?: number | number[];
  title?: string;
  message?: string;
}

/** A sheet of actions (UIAlertController action sheet; a list dialog on
 * Android). */
export const ActionSheet = {
  /** `callback` gets the chosen index (the cancel index when dismissed). */
  show(options: ActionSheetOptions, callback: (index: number) => void): void {
    const destructive = ([] as number[]).concat(options.destructiveButtonIndex ?? []);
    showDialog(
      {
        title: options.title,
        message: options.message,
        sheet: true,
        cancelable: true,
        buttons: options.options.map((text, i) => ({
          text,
          style: i === options.cancelButtonIndex ? "cancel" : destructive.includes(i) ? "destructive" : "default",
        })),
      },
      (index) => callback(index >= 0 ? index : (options.cancelButtonIndex ?? -1)),
    );
  },
};
