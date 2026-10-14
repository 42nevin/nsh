import { For } from "solid-js";
import { render, root, Screen, Stack } from "yf";
import { Controls } from "./demos/Controls";
import { Form } from "./demos/Form";
import { Gestures } from "./demos/Gestures";
import { Images } from "./demos/Images";
import { Lists } from "./demos/Lists";
import { Media } from "./demos/Media";
import { Navigation } from "./demos/Navigation";
import { Photos } from "./demos/Photos";
import { Physics } from "./demos/Physics";
import { Scrolling } from "./demos/Scrolling";
import { Styles } from "./demos/Styles";
import { System } from "./demos/System";
import { TextDemo } from "./demos/TextDemo";
import { type DemoEntry, nav } from "./nav";
import { colors } from "./ui";

const demos: DemoEntry[] = [
  { name: "Photos", detail: "A long recycled list: images, pull to refresh, paging", screen: Photos },
  { name: "Lists", detail: "Columns, sticky sections, a chat, horizontal lists", screen: Lists },
  { name: "Scrolling", detail: "Paging, snapping, scroll events, prepending, keyboard taps", screen: Scrolling },
  { name: "Navigation", detail: "Header options, modals, the back button, safe areas", screen: Navigation },
  { name: "Form", detail: "Inputs, keyboard types, the keyboard over the content", screen: Form },
  { name: "Controls", detail: "Switch, slider, spinner, layout animation, sheet", screen: Controls },
  { name: "Gestures", detail: "Pinch, rotate, press states, hit slop, pointer events", screen: Gestures },
  { name: "Physics", detail: "Drags and springs on the UI thread", screen: Physics },
  { name: "Images", detail: "Resize modes, tint, load and error events", screen: Images },
  { name: "Styles", detail: "Corners, borders, shadows, clipping, 3D transforms", screen: Styles },
  { name: "Text", detail: "Fonts, spans, decorations, truncation, shadows", screen: TextDemo },
  { name: "Web and video", detail: "WKWebView / WebView, AVPlayer / MediaPlayer", screen: Media },
  { name: "System", detail: "Status bar, alerts, action sheets, environment, timers, fetch", screen: System },
];

function Home() {
  return (
    <Screen title="Gallery">
      <scroll style={{ flex: 1, backgroundColor: colors.background }}>
        <view style={{ margin: 16, borderRadius: 12, backgroundColor: colors.surface, overflow: "hidden" }}>
          <For each={demos}>
            {(demo, i) => (
              <view
                onPress={() => nav.push({ name: "demo", demo })}
                activeOpacity={0.5}
                accessibilityRole="button"
                style={{
                  paddingHorizontal: 16,
                  paddingVertical: 12,
                  gap: 2,
                  borderTopWidth: i() === 0 ? 0 : 1,
                  borderTopColor: "#eaeef2",
                }}
              >
                <text style={{ fontSize: 17, color: colors.text }}>{demo.name}</text>
                <text style={{ fontSize: 13, color: colors.muted }}>{demo.detail}</text>
              </view>
            )}
          </For>
        </view>
      </scroll>
    </Screen>
  );
}

function App() {
  return (
    <Stack navigator={nav} style={{ flex: 1 }}>
      {(route) => {
        switch (route.name) {
          case "home":
            return <Home />;
          case "demo": {
            const Demo = route.demo.screen;
            return (
              <Screen title={route.demo.name}>
                <Demo />
              </Screen>
            );
          }
          case "page": {
            const Page = route.page;
            return <Page />;
          }
        }
      }}
    </Stack>
  );
}

render(() => <App />, root);
