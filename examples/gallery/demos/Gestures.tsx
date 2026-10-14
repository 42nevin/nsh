import { createSignal, For } from "solid-js";
import { createNativeValue, PanResponder, Pressable, type GestureResponderEvent } from "yf";
import { Button, colors, Section } from "../ui";

/** Drag, pinch and rotate one card at the same time, all on the UI thread. */
function Card() {
  const x = createNativeValue(0);
  const y = createNativeValue(0);
  const scale = createNativeValue(1);
  const angle = createNativeValue(0);
  const reset = () => {
    x.spring(0);
    y.spring(0);
    scale.spring(1);
    angle.spring(0);
  };
  return (
    <view style={{ gap: 12, alignItems: "center" }}>
      <view style={{ height: 220, alignSelf: "stretch", alignItems: "center", justifyContent: "center", overflow: "hidden" }}>
        <view
          pan={{ x, y }}
          pinch={{ scale }}
          rotation={{ angle }}
          style={{
            width: 200,
            height: 150,
            borderRadius: 14,
            overflow: "hidden",
            transform: [{ translateX: x }, { translateY: y }, { scale }, { rotate: angle }],
          }}
        >
          <image src="https://picsum.photos/seed/yf-card/400/300" style={{ flex: 1 }} />
        </view>
      </view>
      <text style={{ color: colors.muted, fontSize: 13 }}>Drag with one finger; pinch and rotate with two (Option-drag in the simulator).</text>
      <Button label="Reset" color="#57606a" onPress={reset} />
    </view>
  );
}

/** A ball dragged with a PanResponder (positions go through JS). Its
 * responder blocks the page from scrolling while it is dragged. */
function Ball() {
  const [pos, setPos] = createSignal({ x: 0, y: 0 });
  const [state, setState] = createSignal("idle");
  let start = { x: 0, y: 0 };
  const pan = PanResponder.create({
    onStartShouldSetPanResponder: () => true,
    onPanResponderGrant: () => {
      start = pos();
      setState("dragging");
    },
    onPanResponderMove: (_, g) => setPos({ x: start.x + g.dx, y: start.y + g.dy }),
    onPanResponderRelease: (_, g) => setState(`released at ${g.vx.toFixed(2)}, ${g.vy.toFixed(2)} pt/ms`),
    onPanResponderTerminate: () => setState("taken over"),
  });
  return (
    <view style={{ gap: 8 }}>
      <view style={{ height: 180, borderRadius: 10, backgroundColor: "#f6f8fa", overflow: "hidden" }}>
        <view
          {...pan.panHandlers}
          style={{
            position: "absolute",
            left: 130,
            top: 60,
            width: 60,
            height: 60,
            borderRadius: 30,
            backgroundColor: colors.accent,
            transform: [{ translateX: pos().x }, { translateY: pos().y }],
          }}
        />
      </view>
      <text style={{ color: colors.muted, fontSize: 13 }}>{state()}</text>
      <Button label="Reset" color="#57606a" onPress={() => setPos({ x: 0, y: 0 })} />
    </view>
  );
}

/** Raw touch events, with every finger. */
function TouchLog() {
  const [log, setLog] = createSignal<string[]>([]);
  const note = (name: string) => (e: GestureResponderEvent) =>
    setLog((l) =>
      [
        `${name} at ${Math.round(e.locationX)},${Math.round(e.locationY)} (page ${Math.round(e.pageX)},${Math.round(e.pageY)}), ${e.touches.length} down`,
        ...l,
      ].slice(0, 4),
    );
  return (
    <view style={{ gap: 8 }}>
      <view
        onTouchStart={note("start")}
        onTouchMove={note("move")}
        onTouchEnd={note("end")}
        onTouchCancel={note("cancel")}
        style={{ height: 100, borderRadius: 10, backgroundColor: "#ddf4ff", alignItems: "center", justifyContent: "center" }}
      >
        <text style={{ color: colors.accent }}>Touch here (try two fingers)</text>
      </view>
      <text style={{ color: colors.muted, fontSize: 13 }}>{log().join("\n") || "No touches yet"}</text>
    </view>
  );
}

export function Gestures() {
  const [log, setLog] = createSignal<string[]>([]);
  const add = (e: string) => setLog((l) => [e, ...l].slice(0, 6));
  const [slop, setSlop] = createSignal(0);
  const [through, setThrough] = createSignal(0);
  return (
    <scroll style={{ flex: 1, backgroundColor: colors.background }}>
      <view style={{ padding: 16, gap: 20 }}>
        <Section title="Pan, pinch and rotation">
          <Card />
        </Section>
        <Section title="PanResponder (drag the ball; the page does not scroll)">
          <Ball />
        </Section>
        <Section title="Touch events">
          <TouchLog />
        </Section>
        <Section title="Press events">
          <view
            onPressIn={() => add("pressIn")}
            onPressOut={() => add("pressOut")}
            onPress={() => add("press")}
            onLongPress={() => add("longPress")}
            activeOpacity={0.5}
            style={{ padding: 18, borderRadius: 10, backgroundColor: "#ddf4ff", alignItems: "center" }}
          >
            <text style={{ fontWeight: "600" }}>Tap, hold, or slide off</text>
          </view>
          <text style={{ color: colors.muted }}>{log().join(" · ") || "No events yet"}</text>
          <view style={{ flexDirection: "row", gap: 10 }}>
            <Pressable
              onPress={() => add("ripple")}
              onLongPress={() => add("fast long press")}
              delayLongPress={250}
              android_ripple={{ color: "rgba(9,105,218,0.3)" }}
              style={{ flex: 1, padding: 14, borderRadius: 10, backgroundColor: "#f6f8fa", alignItems: "center" }}
            >
              <text>Ripple, long press 250 ms</text>
            </Pressable>
            <Pressable
              onPress={() => add("borderless")}
              android_ripple={{ borderless: true, radius: 40 }}
              style={{ width: 56, height: 56, alignItems: "center", justifyContent: "center" }}
            >
              <text style={{ fontSize: 22 }}>★</text>
            </Pressable>
          </view>
          <text onPress={() => add("text")} activeOpacity={0.3} style={{ color: colors.accent }}>
            Pressable text with activeOpacity
          </text>
        </Section>
        <Section title="Hit slop">
          <view style={{ flexDirection: "row", alignItems: "center", gap: 16 }}>
            <view
              hitSlop={30}
              onPress={() => setSlop(slop() + 1)}
              style={{ width: 12, height: 12, borderRadius: 6, backgroundColor: "#cf222e", margin: 20 }}
            />
            <text style={{ flex: 1 }}>The dot takes taps 30 pt around it: {slop()}</text>
          </view>
        </Section>
        <Section title="Pointer events">
          <view style={{ height: 70 }}>
            <view
              onPress={() => setThrough(through() + 1)}
              style={{ flex: 1, borderRadius: 10, backgroundColor: "#1a7f37", alignItems: "center", justifyContent: "center" }}
            >
              <text style={{ color: "#ffffff" }}>Button under an overlay: {through()}</text>
            </view>
            <view
              pointerEvents="none"
              style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, borderRadius: 10, borderWidth: 3, borderColor: "#bf8700", borderStyle: "dashed" }}
            />
          </view>
          <text style={{ color: colors.muted, fontSize: 13 }}>The dashed overlay has pointerEvents="none": taps reach the button.</text>
        </Section>
        <Section title="Accessibility">
          <For each={[{ label: "Play", hint: "Starts the song" }, { label: "Like", hint: "Adds to favorites" }]}>
            {(item) => (
              <view
                accessible
                accessibilityRole="button"
                accessibilityLabel={item.label}
                accessibilityHint={item.hint}
                onPress={() => add(item.label)}
                style={{ padding: 12, borderRadius: 10, backgroundColor: "#eaeef2" }}
              >
                <text>{`${item.label} (labelled for VoiceOver / TalkBack)`}</text>
              </view>
            )}
          </For>
        </Section>
      </view>
    </scroll>
  );
}
