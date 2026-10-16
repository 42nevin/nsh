// Navigation: a stack of screens with header options, pushed and popped.
import { mount, signal } from "nsh";
import { Button, createNavigator, Screen, Stack } from "nsh/ui/mobile";

type Route = { name: "home" } | { name: "detail"; id: number };

const nav = createNavigator<Route>({ name: "home" });

function Home() {
  const opened = signal(0);
  return (
    <Screen title="Inbox">
      <view style={{ flex: 1, backgroundColor: "#ffffff", padding: 16, paddingTop: 24, gap: 12 }}>
        <text style={{ color: "#737373" }}>{`Opened ${opened()} messages`}</text>
        <Button
          title="Open message 1"
          onPress={() => {
            opened.set(opened() + 1);
            nav.push({ name: "detail", id: 1 });
          }}
        />
        <Button title="Open message 2" variant="secondary" onPress={() => nav.push({ name: "detail", id: 2 })} />
      </view>
    </Screen>
  );
}

function Detail(props: { id: number }) {
  const starred = signal(false);
  return (
    <Screen
      title={`Message ${props.id}`}
      headerRight={
        <text onPress={() => starred.set(!starred())} style={{ fontSize: 17 }}>
          {starred() ? "★" : "☆"}
        </text>
      }
    >
      <view style={{ flex: 1, backgroundColor: "#ffffff", padding: 16, gap: 12 }}>
        <text>{`The body of message ${props.id}.`}</text>
        <Button title="Next message" onPress={() => nav.push({ name: "detail", id: props.id + 1 })} />
        <Button title="Back to the inbox" variant="secondary" onPress={() => nav.popToTop()} />
      </view>
    </Screen>
  );
}

function App() {
  return (
    <Stack navigator={nav} style={{ flex: 1 }}>
      {(route) => (route.name === "home" ? <Home /> : <Detail id={route.id} />)}
    </Stack>
  );
}

mount(App);
