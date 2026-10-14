import { createSignal } from "solid-js";
import { fontScale } from "yf";
import pacifico from "../fonts/Pacifico-Regular.ttf";

const LOREM =
  "Native text, typeset once on the engine thread with CoreText and drawn on the main thread without any layout work.";

export function TextDemo() {
  const caption = { fontSize: 12, color: "#6e7781", marginTop: 10 } as const;
  const link = { color: "#0969da", textDecorationLine: "underline" } as const;
  const [tapped, setTapped] = createSignal("Tap a link in the sentence");
  return (
    <scroll style={{ flex: 1 }}>
      <view style={{ padding: 20, gap: 4 }}>
        <text style={{ fontSize: 16, lineHeight: 24 }} onPress={() => setTapped("the sentence (not a link)")}>
          {"By continuing you agree to the "}
          <text style={link} onPress={() => setTapped("Terms of service")} onLongPress={() => setTapped("Terms (long press)")}>
            Terms of service
          </text>
          {" and the "}
          <text style={link} onPress={() => setTapped("Privacy policy")}>
            Privacy policy
          </text>
          {", which mention "}
          <text style={{ fontWeight: "700" }}>
            nested <text style={link} onPress={() => setTapped("a link inside bold text")}>links</text> too
          </text>
          .
        </text>
        <text style={caption}>{`Pressed: ${tapped()}`}</text>
        <text style={{ fontFamily: pacifico, fontSize: 30, color: "#8250df" }}>Custom fonts</text>
        <text style={caption}>Styles</text>
        <text style={{ fontSize: 17 }}>
          <text style={{ fontWeight: "300" }}>Light </text>
          <text style={{ fontWeight: "bold" }}>Bold </text>
          <text style={{ fontStyle: "italic" }}>Italic </text>
          <text style={{ fontWeight: "bold", fontStyle: "italic" }}>Both </text>
          <text style={{ letterSpacing: 4 }}>Spaced</text>
        </text>
        <text style={{ fontSize: 17 }}>
          <text style={{ textDecorationLine: "underline" }}>Underline</text>{" "}
          <text style={{ textDecorationLine: "line-through", textDecorationColor: "#cf222e" }}>Strikethrough</text>{" "}
          <text style={{ textDecorationLine: "underline line-through", color: "#0969da" }}>Both</text>
        </text>
        <text style={{ fontSize: 17, textTransform: "uppercase" }}>
          uppercase <text style={{ textTransform: "capitalize" }}>capitalized words here</text>
        </text>
        <text style={{ fontSize: 26, fontWeight: "800", color: "#ffffff", textShadowColor: "rgba(0,0,0,0.6)", textShadowOffset: { width: 2, height: 2 }, textShadowRadius: 4, backgroundColor: "#54aeff", paddingHorizontal: 8 }}>
          Text shadow
        </text>
        <text style={caption}>System text size (fontScale {fontScale().toFixed(2)})</text>
        <text style={{ fontSize: 17 }}>Scales with the system setting</text>
        <text allowFontScaling={false} style={{ fontSize: 17 }}>
          Fixed size (allowFontScaling false)
        </text>
        <text maxFontSizeMultiplier={1.2} style={{ fontSize: 17 }}>
          At most 1.2× (maxFontSizeMultiplier)
        </text>
        <text style={caption}>numberOfLines=2 (tail)</text>
        <text numberOfLines={2} style={{ fontSize: 15 }}>{LOREM}</text>
        <text style={caption}>numberOfLines=1 head / middle / clip</text>
        <text numberOfLines={1} ellipsizeMode="head" style={{ fontSize: 15 }}>{LOREM}</text>
        <text numberOfLines={1} ellipsizeMode="middle" style={{ fontSize: 15 }}>{LOREM}</text>
        <text numberOfLines={1} ellipsizeMode="clip" style={{ fontSize: 15 }}>{LOREM}</text>
        <text style={caption}>Centered, 3 lines</text>
        <text numberOfLines={3} style={{ fontSize: 15, textAlign: "center", lineHeight: 22 }}>
          {LOREM} {LOREM}
        </text>
      </view>
    </scroll>
  );
}
