import { batch, cleanup, computed, context, effect, mount, signal, untracked } from "nsh";

const Accent = context(signal("#0969da"));

/** A stopwatch: the effect starts a timer and its cleanup stops it. */
function Stopwatch() {
  const running = signal(false);
  const tenths = signal(0);
  effect(() => {
    if (!running()) return;
    const id = setInterval(() => tenths.update((t) => t + 1), 100);
    cleanup(() => clearInterval(id));
  });
  const accent = Accent();
  return (
    <view style={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
      <text style={{ flex: 1, fontSize: 28, fontWeight: "600", fontVariant: ["tabular-nums"], color: "#1f2328" }}>
        {(tenths() / 10).toFixed(1)} s
      </text>
      <view onPress={() => running.set(!running())} activeOpacity={0.6} style={{ backgroundColor: accent(), borderRadius: 8, paddingHorizontal: 14, paddingVertical: 8 }}>
        <text style={{ color: "#ffffff", fontWeight: "600" }}>{running() ? "Stop" : "Start"}</text>
      </view>
      <view onPress={() => tenths.set(0)} activeOpacity={0.6} style={{ backgroundColor: "#57606a", borderRadius: 8, paddingHorizontal: 14, paddingVertical: 8 }}>
        <text style={{ color: "#ffffff" }}>Reset</text>
      </view>
    </view>
  );
}

function Section(props: { title: string; children: any }) {
  return (
    <view style={{ backgroundColor: "#ffffff", borderRadius: 12, padding: 14, gap: 10 }}>
      <text style={{ fontSize: 13, fontWeight: "600", color: "#6e7781" }}>{props.title}</text>
      {props.children}
    </view>
  );
}

function App() {
  const price = signal(12);
  const quantity = signal(1);
  const total = computed(() => price() * quantity());
  const writes = signal(0);
  const accent = Accent();

  // Runs again whenever `total` changes; reads of `writes` are not tracked.
  const log = signal<string[]>([]);
  effect(() => {
    const t = total();
    const n = untracked(writes);
    log.update((l) => [`total ${t} after ${n} writes`, ...l].slice(0, 3));
  });

  const step = (dp: number, dq: number) => {
    batch(() => {
      price.update((p) => Math.max(0, p + dp));
      quantity.update((q) => Math.max(0, q + dq));
      writes.update((w) => w + 2);
    });
  };

  const colors = ["#0969da", "#8250df", "#1a7f37", "#bf3989"];

  return (
    <scroll style={{ flex: 1, backgroundColor: "#f6f8fa" }}>
      <view style={{ padding: 20, paddingTop: 24, gap: 16 }}>
        <Section title="signal + computed">
          <text style={{ fontSize: 17, color: "#1f2328" }}>
            {price()} × {quantity()} = <text style={{ fontWeight: "700", color: accent() }}>{total()}</text>
          </text>
          <view style={{ flexDirection: "row", gap: 8 }}>
            <view onPress={() => step(1, 0)} activeOpacity={0.6} style={{ backgroundColor: "#ddf4ff", borderRadius: 8, padding: 8 }}>
              <text>price +1</text>
            </view>
            <view onPress={() => step(0, 1)} activeOpacity={0.6} style={{ backgroundColor: "#ddf4ff", borderRadius: 8, padding: 8 }}>
              <text>qty +1</text>
            </view>
            <view onPress={() => step(1, 1)} activeOpacity={0.6} style={{ backgroundColor: "#fff8c5", borderRadius: 8, padding: 8 }}>
              <text>both (batch)</text>
            </view>
          </view>
        </Section>

        <Section title="effect + untracked">
          <each of={log()}>{(line) => <text style={{ fontSize: 13, color: "#57606a" }}>{line()}</text>}</each>
        </Section>

        <Section title="effect + cleanup">
          <Stopwatch />
        </Section>

        <Section title="context (the value is a signal)">
          <view style={{ flexDirection: "row", gap: 10 }}>
            <each of={colors}>
              {(c) => (
                <view
                  onPress={() => accent.set(c())}
                  style={{ width: 36, height: 36, borderRadius: 18, backgroundColor: c(), borderWidth: 3, borderColor: accent() === c() ? "#1f2328" : "transparent" }}
                />
              )}
            </each>
          </view>
        </Section>
      </view>
    </scroll>
  );
}

mount(() => (
  <Accent.Provider value={signal("#0969da")}>
    <App />
  </Accent.Provider>
));
