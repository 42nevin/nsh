import { createSignal, For, Index } from "solid-js";
import { List, windowSize, type ScrollEvent } from "yf";
import { Button, colors, Section } from "../ui";

const palette = ["#0969da", "#8250df", "#bf3989", "#cf222e", "#bc4c00", "#1a7f37", "#0a3069"];

function Toggle(props: { label: string; value: boolean; onChange: (v: boolean) => void }) {
  return (
    <view style={{ flexDirection: "row", alignItems: "center" }}>
      <text style={{ flex: 1 }}>{props.label}</text>
      <switch value={props.value} onValueChange={props.onChange} />
    </view>
  );
}

function Choice<T extends string>(props: { options: readonly T[]; value: T; onChange: (v: T) => void }) {
  return (
    <view style={{ flexDirection: "row", gap: 6, flexWrap: "wrap" }}>
      <For each={props.options}>
        {(o) => (
          <text
            onPress={() => props.onChange(o)}
            style={{
              fontSize: 13,
              paddingHorizontal: 10,
              paddingVertical: 5,
              borderRadius: 6,
              overflow: "hidden",
              color: props.value === o ? "#ffffff" : colors.text,
              backgroundColor: props.value === o ? colors.accent : "#e1e4e8",
            }}
          >
            {o}
          </text>
        )}
      </For>
    </view>
  );
}

/** Full-width pages: one per fling, with a page indicator. */
function Pager() {
  // Pages as wide as the scroll view.
  const [width, setWidth] = createSignal(windowSize().width - 60);
  const [page, setPage] = createSignal(0);
  return (
    <view style={{ gap: 8 }}>
      <scroll
        horizontal
        pagingEnabled
        showsIndicator={false}
        style={{ height: 140, borderRadius: 12 }}
        onLayout={(e) => setWidth(e.width)}
        onMomentumScrollEnd={(e) => setPage(Math.round(e.x / width()))}
        onScrollEnd={(e) => setPage(Math.round(e.x / width()))}
      >
        <view style={{ flexDirection: "row" }}>
          <Index each={palette.slice(0, 5)}>
            {(color, i) => (
              <view style={{ width: width(), height: 140, backgroundColor: color(), alignItems: "center", justifyContent: "center" }}>
                <text style={{ color: "#ffffff", fontSize: 28, fontWeight: "700" }}>Page {i + 1}</text>
              </view>
            )}
          </Index>
        </view>
      </scroll>
      <view style={{ flexDirection: "row", justifyContent: "center", gap: 6 }}>
        <Index each={palette.slice(0, 5)}>
          {(_, i) => (
            <view style={{ width: 8, height: 8, borderRadius: 4, backgroundColor: page() === i ? colors.accent : colors.border }} />
          )}
        </Index>
      </view>
    </view>
  );
}

/** Cards that snap to the center; a fling can pass several unless limited to one. */
function Cards() {
  const card = 220;
  const gap = 12;
  const [oneStep, setOneStep] = createSignal(false);
  const [align, setAlign] = createSignal<"start" | "center" | "end">("center");
  return (
    <view style={{ gap: 10 }}>
      <List
        each={Array.from({ length: 20 }, (_, i) => i)}
        horizontal
        estimatedItemSize={card + gap}
        snapToInterval={card + gap}
        snapToAlignment={align()}
        decelerationRate="fast"
        disableIntervalMomentum={oneStep()}
        showsIndicator={false}
        style={{ height: 120 }}
        contentContainerStyle={{ gap }}
      >
        {(i) => (
          <view
            style={{
              width: card,
              height: 120,
              borderRadius: 14,
              backgroundColor: palette[i() % palette.length],
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <text style={{ color: "#ffffff", fontSize: 22, fontWeight: "700" }}>Card {i() + 1}</text>
          </view>
        )}
      </List>
      <Choice options={["start", "center", "end"] as const} value={align()} onChange={setAlign} />
      <Toggle label="One card per fling" value={oneStep()} onChange={setOneStep} />
    </view>
  );
}

/** onScroll (throttled), drag and momentum events, scrollEnabled, bounces, contentInset. */
function Events() {
  const [offset, setOffset] = createSignal<ScrollEvent | null>(null);
  const [log, setLog] = createSignal<string[]>([]);
  const [enabled, setEnabled] = createSignal(true);
  const [bounces, setBounces] = createSignal(true);
  const [inset, setInset] = createSignal(false);
  const [throttle, setThrottle] = createSignal<"0" | "100" | "500">("100");
  const note = (name: string) => (e: { y: number }) => setLog((l) => [`${name} @ ${Math.round(e.y)}`, ...l].slice(0, 5));
  return (
    <view style={{ gap: 10 }}>
      <scroll
        style={{ height: 180, borderRadius: 10, backgroundColor: "#f6f8fa" }}
        scrollEnabled={enabled()}
        bounces={bounces()}
        contentInset={inset() ? { top: 40, bottom: 40 } : undefined}
        scrollEventThrottle={Number(throttle())}
        onScroll={setOffset}
        onScrollBeginDrag={note("begin drag")}
        onScrollEndDrag={note("end drag")}
        onMomentumScrollBegin={note("momentum begin")}
        onMomentumScrollEnd={note("momentum end")}
      >
        <view style={{ padding: 12, gap: 6 }}>
          <For each={Array.from({ length: 30 }, (_, i) => i + 1)}>
            {(i) => <text style={{ color: colors.muted }}>Line {i}</text>}
          </For>
        </view>
      </scroll>
      <text style={{ fontVariant: ["tabular-nums"] }}>
        {offset()
          ? `y ${Math.round(offset()!.y)} of ${Math.round(offset()!.contentHeight)} (viewport ${Math.round(offset()!.height)})`
          : "Scroll the box"}
      </text>
      <text style={{ color: colors.muted, fontSize: 13 }}>{log().join("\n") || "No drags yet"}</text>
      <view style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
        <text style={{ flex: 1 }}>onScroll every</text>
        <Choice options={["0", "100", "500"] as const} value={throttle()} onChange={setThrottle} />
        <text>ms</text>
      </view>
      <Toggle label="Scroll enabled" value={enabled()} onChange={setEnabled} />
      <Toggle label="Bounces" value={bounces()} onChange={setBounces} />
      <Toggle label="Content inset 40 (top and bottom)" value={inset()} onChange={setInset} />
    </view>
  );
}

type Post = { id: number; label: string };

/** New items arrive at the top; the rows on screen stay where they are. */
function Feed() {
  let next = 0;
  const post = (label: string): Post => ({ id: next++, label });
  const [posts, setPosts] = createSignal(Array.from({ length: 30 }, (_, i) => post(`Post ${i + 1}`)));
  let fresh = 0;
  const addOnTop = () => setPosts((p) => [...Array.from({ length: 3 }, () => post(`New ${++fresh}`)), ...p]);
  return (
    <view style={{ gap: 10 }}>
      <List each={posts()} estimatedItemSize={44} style={{ height: 240, borderRadius: 10, backgroundColor: "#f6f8fa" }}>
        {(p) => (
          <view style={{ paddingHorizontal: 12, paddingVertical: 12, borderBottomWidth: 1, borderBottomColor: "#eaeef2" }}>
            <text style={{ color: p().label.startsWith("New") ? colors.accent : colors.text }}>{p().label}</text>
          </view>
        )}
      </List>
      <Button label="Add 3 posts on top" onPress={addOnTop} />
    </view>
  );
}

type Taps = "never" | "handled" | "always";

export function Scrolling() {
  const [taps, setTaps] = createSignal<Taps>("never");
  const [presses, setPresses] = createSignal(0);
  return (
    <scroll style={{ flex: 1, backgroundColor: colors.background }} keyboardShouldPersistTaps={taps()}>
      <view style={{ padding: 16, gap: 20 }}>
        <Section title="Paging">
          <Pager />
        </Section>
        <Section title="Snapping (snapToInterval)">
          <Cards />
        </Section>
        <Section title="Events and behavior">
          <Events />
        </Section>
        <Section title="Prepending keeps the position">
          <Feed />
        </Section>
        <Section title="Taps with the keyboard up (keyboardShouldPersistTaps)">
          <Choice options={["never", "handled", "always"] as const} value={taps()} onChange={setTaps} />
          <text style={{ color: colors.muted, fontSize: 13 }}>
            {taps() === "never"
              ? "A tap outside the input only hides the keyboard; the button does not get it."
              : taps() === "handled"
                ? "The button gets the tap and the keyboard stays; a tap on empty space hides it."
                : "Taps never hide the keyboard."}
          </text>
          <input
            placeholder="Type here, then tap the button"
            style={{ fontSize: 16, borderWidth: 1, borderColor: colors.border, borderRadius: 10, padding: 10 }}
          />
          <Button label={`Pressed ${presses()} times`} onPress={() => setPresses(presses() + 1)} />
          <view style={{ height: 300 }} />
        </Section>
      </view>
    </scroll>
  );
}
