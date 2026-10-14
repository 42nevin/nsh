import { createSignal, For, onCleanup } from "solid-js";
import { ActionSheetIOS, Alert, colorScheme, fontScale, keyboard, safeArea, StatusBar, ToastAndroid, windowSize } from "yf";
import { Button, colors, Section } from "../ui";

function Row(props: { label: string; value: string }) {
  return (
    <view style={{ flexDirection: "row" }}>
      <text style={{ flex: 1, color: colors.muted }}>{props.label}</text>
      <text>{props.value}</text>
    </view>
  );
}

type Post = { id: number; title: string };

export function System() {
  const [fps, setFps] = createSignal(0);
  const [running, setRunning] = createSignal(false);
  const [ticks, setTicks] = createSignal(0);
  const [posts, setPosts] = createSignal<Post[]>([]);
  const [status, setStatus] = createSignal("");
  const [answer, setAnswer] = createSignal("");
  const [barStyle, setBarStyle] = createSignal<"default" | "light-content" | "dark-content">("default");
  const [barHidden, setBarHidden] = createSignal(false);

  // Frames per second, counted with requestAnimationFrame.
  let frame = 0;
  let frames = 0;
  let since = 0;
  const loop = (time: number) => {
    frames++;
    if (time - since >= 1000) {
      setFps(Math.round((frames * 1000) / (time - since)));
      frames = 0;
      since = time;
    }
    frame = requestAnimationFrame(loop);
  };
  const toggle = () => {
    if (running()) cancelAnimationFrame(frame);
    else {
      frames = 0;
      since = performance.now();
      frame = requestAnimationFrame(loop);
    }
    setRunning(!running());
  };
  const timer = setInterval(() => setTicks(ticks() + 1), 1000);
  onCleanup(() => {
    clearInterval(timer);
    cancelAnimationFrame(frame);
  });

  const load = async () => {
    setStatus("loading…");
    const started = performance.now();
    try {
      const response = await fetch("https://jsonplaceholder.typicode.com/posts?_limit=3");
      setPosts(await response.json());
      setStatus(`HTTP ${response.status} in ${Math.round(performance.now() - started)} ms`);
    } catch (e) {
      setStatus(String(e));
    }
  };

  return (
    <scroll style={{ flex: 1, backgroundColor: colors.background }}>
      <view style={{ padding: 16, gap: 20 }}>
        <Section title="Environment (reactive)">
          <Row label="Window" value={`${Math.round(windowSize().width)} × ${Math.round(windowSize().height)}`} />
          <Row label="Safe area" value={`${Math.round(safeArea().top)} / ${Math.round(safeArea().right)} / ${Math.round(safeArea().bottom)} / ${Math.round(safeArea().left)}`} />
          <Row label="Keyboard" value={`${Math.round(keyboard().height)} pt`} />
          <Row label="Color scheme" value={colorScheme()} />
          <Row label="Font scale" value={fontScale().toFixed(2)} />
        </Section>
        <Section title="Status bar">
          <StatusBar barStyle={barStyle()} hidden={barHidden()} animated />
          <Row label="Style" value={barStyle()} />
          <view style={{ flexDirection: "row", gap: 8 }}>
            <view style={{ flex: 1 }}>
              <Button label="Next style" onPress={() => setBarStyle(barStyle() === "default" ? "light-content" : barStyle() === "light-content" ? "dark-content" : "default")} />
            </view>
            <view style={{ flex: 1 }}>
              <Button label={barHidden() ? "Show" : "Hide"} color="#57606a" onPress={() => setBarHidden(!barHidden())} />
            </view>
          </view>
        </Section>
        <Section title="Alerts">
          <Button label="Alert with one button" onPress={() => Alert.alert("Saved", "Your changes were saved.", [{ text: "OK", onPress: () => setAnswer("OK") }])} />
          <Button
            label="Confirm (cancel / destructive)"
            color="#cf222e"
            onPress={() =>
              Alert.alert("Delete the photo?", "This cannot be undone.", [
                { text: "Cancel", style: "cancel", onPress: () => setAnswer("Cancel") },
                { text: "Delete", style: "destructive", onPress: () => setAnswer("Delete") },
              ], { cancelable: true, onDismiss: () => setAnswer("dismissed") })
            }
          />
          <Button
            label="Three buttons"
            color="#8250df"
            onPress={() =>
              Alert.alert("Update available", "Install it now?", [
                { text: "Later", onPress: () => setAnswer("Later") },
                { text: "Never", style: "cancel", onPress: () => setAnswer("Never") },
                { text: "Install", onPress: () => setAnswer("Install") },
              ])
            }
          />
          <Button
            label="Action sheet"
            color="#1a7f37"
            onPress={() =>
              ActionSheetIOS.showActionSheetWithOptions(
                { title: "Share the photo", options: ["Cancel", "Copy link", "Save", "Delete"], cancelButtonIndex: 0, destructiveButtonIndex: 3 },
                (i) => setAnswer(["Cancel", "Copy link", "Save", "Delete"][i] ?? String(i)),
              )
            }
          />
          <Button label="Toast (Android)" color="#57606a" onPress={() => ToastAndroid.show("Hello from a toast", ToastAndroid.SHORT)} />
          <Row label="Answer" value={answer() || "–"} />
        </Section>
        <Section title="Frames and timers">
          <Row label="requestAnimationFrame" value={running() ? `${fps()} fps` : "stopped"} />
          <Row label="setInterval (1 s)" value={String(ticks())} />
          <Button label={running() ? "Stop counting frames" : "Count frames"} onPress={toggle} />
        </Section>
        <Section title="Network and logs">
          <Button label="fetch() three posts" onPress={load} />
          <text style={{ color: colors.muted }}>{status()}</text>
          <For each={posts()}>
            {(post) => (
              <text numberOfLines={1}>
                <text style={{ fontWeight: "bold" }}>{`#${post.id} `}</text>
                {post.title}
              </text>
            )}
          </For>
          <Button label="console.log (shown by yf run)" color="#57606a" onPress={() => console.log("hello from the gallery", { at: new Date().toISOString() })} />
        </Section>
      </view>
    </scroll>
  );
}
