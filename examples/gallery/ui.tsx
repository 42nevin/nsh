import type { JSX } from "yf";

export const colors = {
  text: "#1f2328",
  muted: "#6e7781",
  border: "#d0d7de",
  surface: "#ffffff",
  background: "#f4f5f7",
  accent: "#0969da",
} as const;

export function Button(props: { label: string; color?: string; onPress: () => void }) {
  return (
    <view
      onPress={props.onPress}
      activeOpacity={0.6}
      accessibilityRole="button"
      style={{ backgroundColor: props.color ?? colors.accent, borderRadius: 10, paddingVertical: 10, paddingHorizontal: 16 }}
    >
      <text style={{ color: "#ffffff", fontWeight: "600", textAlign: "center" }}>{props.label}</text>
    </view>
  );
}

/** A titled group of controls on a white card. */
export function Section(props: { title: string; children: JSX.Element }) {
  return (
    <view style={{ gap: 8 }}>
      <text style={{ fontSize: 13, fontWeight: "600", color: colors.muted, textTransform: "uppercase" }}>{props.title}</text>
      <view style={{ backgroundColor: colors.surface, borderRadius: 12, padding: 14, gap: 12 }}>{props.children}</view>
    </view>
  );
}
