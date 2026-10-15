// `Button`: a press target with a label, the same on iOS and Android. A demo
// of how components of nsh/ui/mobile are built (see README.md).

import type { JSX, StyleProp, ViewStyle } from "nsh";

export interface ButtonProps {
  /** The label. */
  title: string;
  onPress?: () => void;
  /** `primary` (default): filled; `secondary`: tinted; `plain`: text only. */
  variant?: "primary" | "secondary" | "plain";
  /** No presses, no feedback, dimmed. */
  disabled?: boolean;
  /** Classes appended to the button's own: they win over them. */
  class?: string;
  style?: StyleProp<ViewStyle>;
  /** For screen readers, when `title` is not enough. */
  accessibilityLabel?: string;
  testID?: string;
}

const base = "flex-row items-center justify-center rounded-xl px-4 py-3 disabled:opacity-40";

const variants: Record<NonNullable<ButtonProps["variant"]>, string> = {
  primary: "bg-blue-600 active:bg-blue-700 text-white",
  secondary: "bg-blue-100 active:bg-blue-200 text-blue-700 dark:bg-blue-950 dark:active:bg-blue-900 dark:text-blue-300",
  plain: "active:opacity-60 text-blue-600 dark:text-blue-400",
};

/** A button drawn with nsh's primitives: the feedback is native (`active:`),
 * the label inherits the button's text classes. */
export function Button(props: ButtonProps): JSX.Element {
  return (
    <view
      onPress={() => props.onPress?.()}
      disabled={props.disabled}
      accessibilityRole="button"
      accessibilityLabel={props.accessibilityLabel ?? props.title}
      accessibilityState={{ disabled: !!props.disabled }}
      testID={props.testID}
      class={`${base} ${variants[props.variant ?? "primary"]} ${props.class ?? ""}`}
      style={props.style}
    >
      <text class="text-[17px] font-semibold">{props.title}</text>
    </view>
  );
}
