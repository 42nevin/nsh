import { mount, signal } from "nsh";
import { Animated, createNativeValue, Easing } from "nsh/animated";

const TRACK = 240;
const curves: [string, Easing][] = [
  ["linear", Easing.linear],
  ["quad out", Easing.out(Easing.quad)],
  ["cubic inOut", Easing.inOut(Easing.cubic)],
  ["back out", Easing.out(Easing.back())],
  ["elastic out", Easing.out(Easing.elastic(1))],
  ["bounce out", Easing.out(Easing.bounce)],
];

function Button(props: { label: string; onPress: () => void }) {
  return (
    <text
      onPress={props.onPress}
      activeOpacity={0.6}
      style={{
        paddingHorizontal: 14,
        paddingVertical: 8,
        borderRadius: 16,
        overflow: "hidden",
        backgroundColor: "#e0e7ff",
        color: "#4338ca",
        fontWeight: "600",
      }}
    >
      {props.label}
    </text>
  );
}

function App() {
  const dots = curves.map(() => createNativeValue(0));
  const spin = createNativeValue(0);
  const status = signal("Tap a button");
  let there = false;

  const race = () => {
    there = !there;
    status.set("running…");
    Animated.parallel(
      dots.map((x, i) => Animated.timing(x, { toValue: there ? TRACK : 0, duration: 1000, easing: curves[i][1] })),
    )
      .start()
      .then((finished) => status.set(finished ? "finished" : "stopped"));
  };

  const turn = () => {
    spin.timing(0);
    spin.timing(1, { duration: 900, easing: Easing.inOut(Easing.cubic) });
  };

  return (
    <view style={{ flex: 1, backgroundColor: "#f8fafc", padding: 20, paddingTop: 24, gap: 16 }}>
      <text style={{ fontSize: 22, fontWeight: "700" }}>Native animation</text>
      <view style={{ gap: 8 }}>
        <each of={curves}>
          {(c, i) => (
            <view style={{ gap: 2 }}>
              <text style={{ fontSize: 12, color: "#64748b" }}>{c()[0]}</text>
              <view style={{ width: TRACK + 16, height: 16, borderRadius: 8, backgroundColor: "#e2e8f0" }}>
                <view
                  style={{
                    width: 16,
                    height: 16,
                    borderRadius: 8,
                    backgroundColor: "#6366f1",
                    transform: [{ translateX: dots[i()] }],
                  }}
                />
              </view>
            </view>
          )}
        </each>
      </view>
      <view style={{ flexDirection: "row", alignItems: "center", gap: 16 }}>
        <view
          style={{
            width: 64,
            height: 64,
            borderRadius: 14,
            backgroundColor: spin.interpolate({ inputRange: [0, 0.5, 1], outputRange: ["#22c55e", "#eab308", "#22c55e"] }),
            transform: [{ rotate: spin.interpolate({ inputRange: [0, 1], outputRange: ["0deg", "360deg"] }) }],
          }}
        />
        <text style={{ flex: 1, color: "#475569", fontSize: 13 }}>
          Rotation and color are interpolated from one native value.
        </text>
      </view>
      <view style={{ flexDirection: "row", gap: 10 }}>
        <Button label="Race" onPress={race} />
        <Button label="Spin" onPress={turn} />
      </view>
      <text style={{ color: "#64748b" }}>{status()}</text>
    </view>
  );
}

mount(App);
