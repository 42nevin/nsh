// The nsh app the embedding samples (examples/embed-ios, examples/embed-android)
// run. The badge is a native view of the host app, registered as
// `embed.badge` (`NshAppView.register(component:)`, `NshView.registerComponent`).

import { createSignal } from "solid-js";
import { render, root, type YNode } from "nsh";
import { nativeComponent } from "nsh/native";
import { SafeAreaView } from "nsh/ui/mobile";

/** The host app's badge: a label it draws, sized to its text. */
const Badge = nativeComponent<{ text: string; color?: string }, { tap: { count: number } }>("embed.badge", {
  props: ["text", "color"],
  events: ["tap"],
});

const colors = ["#8250df", "#0969da", "#1a7f37", "#cf222e"];

function App() {
  const [taps, setTaps] = createSignal(0);
  let badge: YNode | undefined;
  return (
    <SafeAreaView style={{ flex: 1, padding: 20, gap: 16, backgroundColor: "#f6f8fa" }}>
      <text style={{ fontSize: 22, fontWeight: "700" }}>Rendered by nsh</text>
      <text style={{ color: "#57606a" }}>
        The badge below is a native view of the host app: nsh lays it out, sends it props and gets its events.
      </text>
      <Badge
        ref={(node) => (badge = node)}
        text={taps() === 0 ? "Tap the native badge" : `Tapped ${taps()}×`}
        color={colors[taps() % colors.length]}
        onTap={(e) => {
          setTaps(e.count);
          badge?.command("pulse");
        }}
        style={{ alignSelf: "flex-start", borderRadius: 8, overflow: "hidden" }}
      />
      <text>{`JS saw ${taps()} taps`}</text>
    </SafeAreaView>
  );
}

render(() => <App />, root);
