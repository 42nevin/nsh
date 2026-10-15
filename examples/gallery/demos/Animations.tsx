import { createSignal, For, Index } from "solid-js";
import { Animated, createNativeValue, Easing, LayoutAnimation } from "nsh/animated";
import { Button, colors, Section } from "../ui";

const TRACK = 220;

/** One dot per easing, all started together natively. */
function Curves() {
  const curves: [string, Easing][] = [
    ["linear", Easing.linear],
    ["ease", Easing.ease],
    ["quad out", Easing.out(Easing.quad)],
    ["cubic inOut", Easing.inOut(Easing.cubic)],
    ["sin in", Easing.sin],
    ["circle out", Easing.out(Easing.circle)],
    ["exp inOut", Easing.inOut(Easing.exp)],
    ["back out", Easing.out(Easing.back())],
    ["elastic out", Easing.out(Easing.elastic(1))],
    ["bounce out", Easing.out(Easing.bounce)],
  ];
  const dots = curves.map(() => createNativeValue(0));
  let there = false;
  const play = () => {
    there = !there;
    Animated.parallel(
      dots.map((x, i) => Animated.timing(x, { toValue: there ? TRACK : 0, duration: 1000, easing: curves[i][1] })),
    ).start();
  };
  return (
    <view style={{ gap: 6 }}>
      <Index each={curves}>
        {(c, i) => (
          <view style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
            <text style={{ width: 84, fontSize: 12, color: colors.muted }}>{c()[0]}</text>
            <view style={{ width: TRACK + 14, height: 14, borderRadius: 7, backgroundColor: "#f6f8fa" }}>
              <view
                style={{
                  width: 14,
                  height: 14,
                  borderRadius: 7,
                  backgroundColor: colors.accent,
                  transform: [{ translateX: dots[i] }],
                }}
              />
            </view>
          </view>
        )}
      </Index>
      <Button label="Play" onPress={play} />
    </view>
  );
}

const palette = ["#fd8c73", "#f2cc60", "#4ac26b", "#54aeff", "#8250df"];

/** sequence, parallel, stagger and loop, with stop and reset. */
function Composition() {
  const ys = palette.map(() => createNativeValue(0));
  const spin = createNativeValue(0);
  const fade = createNativeValue(1);
  const [state, setState] = createSignal("idle");
  const report = (name: string) => (finished: boolean) => setState(`${name}: ${finished ? "finished" : "stopped"}`);

  const stagger = Animated.stagger(
    80,
    ys.map((y) =>
      Animated.sequence([
        Animated.timing(y, { toValue: -36, duration: 220, easing: Easing.out(Easing.quad) }),
        Animated.spring(y, { toValue: 0, stiffness: 300, damping: 12 }),
      ]),
    ),
  );
  const sequence = Animated.sequence([
    Animated.timing(spin, { toValue: 1, duration: 500, easing: Easing.inOut(Easing.cubic) }),
    Animated.delay(200),
    Animated.parallel([
      Animated.timing(fade, { toValue: 0.2, duration: 300 }),
      Animated.timing(spin, { toValue: 0, duration: 600, easing: Easing.out(Easing.back()) }),
    ]),
    Animated.timing(fade, { toValue: 1, duration: 300 }),
  ]);
  const pulse = Animated.loop(
    Animated.sequence([
      Animated.timing(fade, { toValue: 0.3, duration: 400, easing: Easing.sin }),
      Animated.timing(fade, { toValue: 1, duration: 400, easing: Easing.sin }),
    ]),
  );
  const run = (name: string, a: typeof stagger) => () => {
    setState(`${name}…`);
    a.start().then(report(name));
  };
  return (
    <view style={{ gap: 12 }}>
      <view style={{ flexDirection: "row", justifyContent: "center", alignItems: "flex-end", gap: 10, height: 80 }}>
        <Index each={palette}>
          {(color, i) => (
            <view
              style={{
                width: 34,
                height: 34,
                borderRadius: 8,
                backgroundColor: color(),
                opacity: fade,
                transform: [
                  { translateY: ys[i] },
                  { rotate: spin.interpolate({ inputRange: [0, 1], outputRange: ["0deg", `${(i + 1) * 36}deg`] }) },
                ],
              }}
            />
          )}
        </Index>
      </view>
      <text style={{ fontSize: 13, color: colors.muted, textAlign: "center" }}>{state()}</text>
      <view style={{ flexDirection: "row", flexWrap: "wrap", gap: 8 }}>
        <Chip label="Stagger" onPress={run("stagger", stagger)} />
        <Chip label="Sequence" onPress={run("sequence", sequence)} />
        <Chip label="Loop" onPress={run("loop", pulse)} />
        <Chip label="Stop" onPress={() => [stagger, sequence, pulse].forEach((a) => a.stop())} />
        <Chip label="Reset" onPress={() => [stagger, sequence, pulse].forEach((a) => a.reset())} />
      </view>
    </view>
  );
}

function Chip(props: { label: string; onPress: () => void }) {
  return (
    <text
      onPress={props.onPress}
      style={{
        paddingHorizontal: 12,
        paddingVertical: 6,
        borderRadius: 14,
        overflow: "hidden",
        backgroundColor: "#ddf4ff",
        color: colors.accent,
        fontWeight: "600",
      }}
    >
      {props.label}
    </text>
  );
}

/** Springs with different settings, released together. */
function Springs() {
  const settings = [
    { label: "gentle", stiffness: 120, damping: 14 },
    { label: "wobbly", stiffness: 180, damping: 8 },
    { label: "stiff", stiffness: 400, damping: 30 },
  ];
  const xs = settings.map(() => createNativeValue(0));
  let there = false;
  const play = () => {
    there = !there;
    Animated.parallel(xs.map((x, i) => Animated.spring(x, { toValue: there ? TRACK : 0, ...settings[i] }))).start();
  };
  return (
    <view style={{ gap: 6 }}>
      <For each={settings}>
        {(s, i) => (
          <view style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
            <text style={{ width: 84, fontSize: 12, color: colors.muted }}>{s.label}</text>
            <view style={{ width: TRACK + 20, height: 20 }}>
              <view
                style={{ width: 20, height: 20, borderRadius: 4, backgroundColor: "#8250df", transform: [{ translateX: xs[i()] }] }}
              />
            </view>
          </view>
        )}
      </For>
      <Button label="Release" color="#8250df" onPress={play} />
    </view>
  );
}

/** A header that hides while the list scrolls up and comes back as it
 * scrolls down: `diffClamp` of the scroll offset, natively. */
function HidingHeader() {
  const scroll = createNativeValue(0);
  const header = scroll.diffClamp(0, 44);
  return (
    <view style={{ height: 260, borderRadius: 10, overflow: "hidden", backgroundColor: "#f6f8fa" }}>
      <scroll style={{ flex: 1 }} scrollValue={scroll}>
        <view style={{ paddingTop: 52, paddingHorizontal: 12, paddingBottom: 12, gap: 8 }}>
          <For each={Array.from({ length: 30 }, (_, i) => i + 1)}>
            {(n) => (
              <view style={{ padding: 10, borderRadius: 8, backgroundColor: colors.surface }}>
                <text>Row {n}</text>
              </view>
            )}
          </For>
        </view>
      </scroll>
      <view
        style={{
          position: "absolute",
          left: 0,
          right: 0,
          top: 0,
          height: 44,
          justifyContent: "center",
          paddingHorizontal: 12,
          backgroundColor: colors.accent,
          transform: [{ translateY: header.mul(-1) }],
          opacity: header.interpolate({ inputRange: [0, 44], outputRange: [1, 0], easing: Easing.in(Easing.quad) }),
        }}
      >
        <text style={{ color: "#ffffff", fontWeight: "700" }}>Scroll up to hide me</text>
      </view>
    </view>
  );
}

/** Colors and angles from one value; outside its range it extends on the
 * right only. */
function Interpolations() {
  const t = createNativeValue(0);
  const [at, setAt] = createSignal(0);
  const go = (to: number) => {
    setAt(to);
    t.timing(to, { duration: 700, easing: Easing.inOut(Easing.cubic) });
  };
  return (
    <view style={{ gap: 10 }}>
      <view style={{ flexDirection: "row", justifyContent: "center", gap: 24, height: 90, alignItems: "center" }}>
        <view
          style={{
            width: 60,
            height: 60,
            borderRadius: 12,
            backgroundColor: t.interpolate({ inputRange: [0, 1, 2], outputRange: ["#54aeff", "#8250df", "#cf222e"] }),
            transform: [{ rotate: t.interpolate({ inputRange: [0, 1], outputRange: ["0deg", "180deg"], extrapolateRight: "extend" }) }],
          }}
        />
        <view
          style={{
            width: 60,
            height: 60,
            borderRadius: 30,
            backgroundColor: "#4ac26b",
            transform: [{ scale: t.interpolate({ inputRange: [0, 1], outputRange: [0.6, 1.2], extrapolate: "clamp" }) }],
          }}
        />
      </view>
      <view style={{ flexDirection: "row", gap: 8 }}>
        <Index each={[0, 1, 2]}>
          {(n) => <Chip label={`t = ${n()}${at() === n() ? " ●" : ""}`} onPress={() => go(n())} />}
        </Index>
      </view>
      <text style={{ fontSize: 12, color: colors.muted }}>
        Color through three stops; rotation extends past 1 (to 360° at 2); scale clamps at 1.2.
      </text>
    </view>
  );
}

/** Layout changes animated natively by the next commit. */
function Layout() {
  const [open, setOpen] = createSignal(false);
  return (
    <view style={{ gap: 10 }}>
      <view style={{ flexDirection: open() ? "column" : "row", gap: 8 }}>
        <Index each={palette}>
          {(color) => <view style={{ height: open() ? 24 : 44, flex: open() ? undefined : 1, borderRadius: 8, backgroundColor: color() }} />}
        </Index>
      </view>
      <Button
        label={open() ? "Row" : "Column"}
        color="#57606a"
        onPress={() => {
          LayoutAnimation.configureNext(LayoutAnimation.Presets.spring);
          setOpen(!open());
        }}
      />
    </view>
  );
}

export function Animations() {
  return (
    <scroll style={{ flex: 1, backgroundColor: colors.background }}>
      <view style={{ padding: 16, gap: 20 }}>
        <Section title="Easing curves">
          <Curves />
        </Section>
        <Section title="Sequence, parallel, stagger, loop">
          <Composition />
        </Section>
        <Section title="Springs">
          <Springs />
        </Section>
        <Section title="Interpolation">
          <Interpolations />
        </Section>
        <Section title="diffClamp: a header that hides on scroll">
          <HidingHeader />
        </Section>
        <Section title="Layout animation">
          <Layout />
        </Section>
      </view>
    </scroll>
  );
}
