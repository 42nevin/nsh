import { mount, signal } from "nsh";
import { Animated, createNativeValue, Easing, LayoutAnimation } from "nsh/animated";

const TRACK = 250;
const muted = "#57606a";
const palette = ["#fd8c73", "#f2cc60", "#4ac26b", "#54aeff", "#8250df"];

function Chip(props: { label: string; onPress: () => void }) {
  return (
    <view onPress={props.onPress} activeOpacity={0.6} style={{ paddingHorizontal: 12, paddingVertical: 7, borderRadius: 14, backgroundColor: "#ddf4ff" }}>
      <text style={{ color: "#0969da", fontWeight: "600" }}>{props.label}</text>
    </view>
  );
}

function App() {
  // 1. A style transition: change the style, the platform animates it.
  const on = signal(false);

  // 2. Native values: timing and spring, driven on the UI thread.
  const timed = createNativeValue(0);
  const sprung = createNativeValue(0);
  let there = false;
  const run = () => {
    there = !there;
    timed.timing(there ? TRACK : 0, { duration: 700, easing: Easing.inOut(Easing.cubic) });
    sprung.spring(there ? TRACK : 0, { stiffness: 180, damping: 9 });
  };

  // 3. A composite: a staggered hop, then all of them spin; runs natively end to end.
  const ys = palette.map(() => createNativeValue(0));
  const spin = createNativeValue(0);
  const status = signal("idle");
  const show = Animated.sequence([
    Animated.stagger(
      70,
      ys.map((y) =>
        Animated.sequence([
          Animated.timing(y, { toValue: -30, duration: 200, easing: Easing.out(Easing.quad) }),
          Animated.spring(y, { toValue: 0, stiffness: 300, damping: 12 }),
        ]),
      ),
    ),
    Animated.timing(spin, { toValue: 1, duration: 600, easing: Easing.inOut(Easing.cubic) }),
  ]);
  const play = () => {
    status.set("running…");
    show.reset();
    show.start(({ finished }) => status.set(finished ? "finished" : "stopped"));
  };

  // 4. Layout animation: the next commit's frame changes animate.
  const wide = signal(false);

  return (
    <view style={{ flex: 1, backgroundColor: "#f6f8fa", padding: 16, paddingTop: 24, gap: 14 }}>
      <text style={{ fontWeight: "600" }}>Transition (style changes animate)</text>
      <view style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
        <view
          onPress={() => on.set(!on())}
          style={{
            width: 64,
            height: 64,
            borderRadius: on() ? 32 : 12,
            backgroundColor: on() ? "#8250df" : "#0969da",
            opacity: on() ? 0.6 : 1,
            transform: [{ rotate: on() ? "45deg" : "0deg" }],
            transitionProperty: "all",
            transitionDuration: 400,
            transitionTimingFunction: "ease-in-out",
          }}
        />
        <view class="size-10 rounded-full border-4 border-zinc-300 border-t-blue-600 animate-spin" />
        <view class="size-10 rounded-full bg-emerald-500 animate-pulse" />
        <text style={{ flex: 1, color: muted, fontSize: 13 }}>Tap the square. animate-spin / animate-pulse run natively.</text>
      </view>

      <text style={{ fontWeight: "600" }}>timing and spring</text>
      <view style={{ gap: 8 }}>
        <view style={{ width: 20, height: 20, borderRadius: 10, backgroundColor: "#0969da", transform: [{ translateX: timed }] }} />
        <view style={{ width: 20, height: 20, borderRadius: 4, backgroundColor: "#bf3989", transform: [{ translateX: sprung }] }} />
        <view
          style={{
            height: 8,
            borderRadius: 4,
            // One value, interpolated into a color and a width fraction.
            backgroundColor: timed.interpolate({ inputRange: [0, TRACK], outputRange: ["#d0d7de", "#1a7f37"] }),
            transform: [{ scaleX: timed.interpolate({ inputRange: [0, TRACK], outputRange: [0.1, 1] }) }],
            transformOrigin: "left",
          }}
        />
        <Chip label="Move" onPress={run} />
      </view>

      <text style={{ fontWeight: "600" }}>Animated.sequence / stagger</text>
      <view style={{ flexDirection: "row", justifyContent: "center", alignItems: "flex-end", gap: 10, height: 64 }}>
        <each of={palette}>
          {(color, i) => (
            <view
              style={{
                width: 32,
                height: 32,
                borderRadius: 8,
                backgroundColor: color(),
                transform: [
                  { translateY: ys[i()] },
                  { rotate: spin.interpolate({ inputRange: [0, 1], outputRange: ["0deg", "180deg"] }) },
                ],
              }}
            />
          )}
        </each>
      </view>
      <view style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
        <Chip label="Play" onPress={play} />
        <Chip label="Stop" onPress={() => show.stop()} />
        <text style={{ color: muted }}>{status()}</text>
      </view>

      <text style={{ fontWeight: "600" }}>LayoutAnimation</text>
      <view style={{ flexDirection: wide() ? "column" : "row", gap: 8 }}>
        <each of={palette}>
          {(color) => <view style={{ height: wide() ? 18 : 40, flex: wide() ? undefined : 1, borderRadius: 6, backgroundColor: color() }} />}
        </each>
      </view>
      <Chip
        label={wide() ? "Back to a row" : "Stack them"}
        onPress={() => {
          LayoutAnimation.configureNext(LayoutAnimation.Presets.spring);
          wide.set(!wide());
        }}
      />
    </view>
  );
}

mount(App);
