import { type InputProps, type JSX, mount, signal, split, type YNode } from "nsh";

/** A component: a function of its props, run once. */
function Badge(props: { label: string; color?: string }) {
  return (
    <view style={{ alignSelf: "flex-start", borderRadius: 999, paddingHorizontal: 10, paddingVertical: 4, backgroundColor: props.color ?? "#ddf4ff" }}>
      <text style={{ fontSize: 13, fontWeight: "600", color: "#1f2328" }}>{props.label}</text>
    </view>
  );
}

/** Children are whatever the caller put between the tags. */
function Card(props: { title: string; children?: JSX.Element }) {
  return (
    <view style={{ backgroundColor: "#ffffff", borderRadius: 12, padding: 14, gap: 10, borderWidth: 1, borderColor: "#d0d7de" }}>
      <text style={{ fontSize: 13, fontWeight: "600", color: "#6e7781" }}>{props.title}</text>
      {props.children}
    </view>
  );
}

/** Own props are split off; the rest is spread onto the input and stays reactive. */
function Field(props: InputProps & { label: string }) {
  const [own, input] = split(props, ["label"]);
  return (
    <view style={{ gap: 4 }}>
      <text style={{ fontSize: 13, color: "#6e7781" }}>{own.label}</text>
      <input
        style={{ fontSize: 16, borderWidth: 1, borderColor: "#d0d7de", borderRadius: 8, paddingHorizontal: 10, paddingVertical: 8, backgroundColor: "#ffffff" }}
        {...input}
      />
    </view>
  );
}

function App() {
  const name = signal("");
  const taps = signal(0);
  let field: YNode | undefined;

  return (
    <scroll style={{ flex: 1, backgroundColor: "#f6f8fa" }}>
      <view style={{ padding: 20, paddingTop: 24, gap: 16 }}>
        <Card title="Props are live">
          <Badge label={`Tapped ${taps()} times`} color={taps() % 2 ? "#fff8c5" : "#ddf4ff"} />
          <view onPress={() => taps.update((n) => n + 1)} activeOpacity={0.6} style={{ backgroundColor: "#0969da", borderRadius: 8, padding: 10 }}>
            <text style={{ color: "#ffffff", textAlign: "center", fontWeight: "600" }}>Tap</text>
          </view>
        </Card>

        <Card title="split, spread and ref">
          <Field label="Your name" placeholder="Type here" value={name()} onChangeText={(t) => name.set(t)} ref={(node) => (field = node)} />
          <text style={{ color: "#1f2328" }}>{name() ? `Hello, ${name()}!` : "Nobody yet."}</text>
          <view style={{ flexDirection: "row", gap: 8 }}>
            <view onPress={() => field?.focus()} activeOpacity={0.6} style={{ backgroundColor: "#8250df", borderRadius: 8, paddingHorizontal: 12, paddingVertical: 8 }}>
              <text style={{ color: "#ffffff" }}>Focus</text>
            </view>
            <view onPress={() => (name.set(""), field?.blur())} activeOpacity={0.6} style={{ backgroundColor: "#57606a", borderRadius: 8, paddingHorizontal: 12, paddingVertical: 8 }}>
              <text style={{ color: "#ffffff" }}>Clear</text>
            </view>
          </view>
        </Card>

        <Card title="Kebab-case tags">
          <badge label="<badge> is the Badge component" color="#dafbe1" />
        </Card>
      </view>
    </scroll>
  );
}

mount(App);
