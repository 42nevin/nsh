// Group: settings-style rows; each child becomes a row with a divider.
import { mount, signal } from "nsh";
import { Group, Stepper } from "nsh/ui/mobile";

function Row(props: { label: string; value: string; onPress?: () => void }) {
  return (
    <view onPress={props.onPress} style={{ flexDirection: "row", justifyContent: "space-between" }}>
      <text>{props.label}</text>
      <text style={{ color: "#737373" }}>{props.value}</text>
    </view>
  );
}

function App() {
  const notifications = signal(true);
  const size = signal(16);
  return (
    <view style={{ flex: 1, backgroundColor: "#f5f5f5", padding: 16, paddingTop: 24, gap: 24 }}>
      <Group header={<text>ACCOUNT</text>}>
        <Row label="Name" value="Ada Lovelace" />
        <Row label="E-mail" value="ada@example.com" />
      </Group>
      <Group
        header={<text>PREFERENCES</text>}
        footer={<text>Tap the first row to toggle it. Rows are the app's JSX; the group adds the box and the dividers.</text>}
      >
        <Row
          label="Notifications"
          value={notifications() ? "On" : "Off"}
          onPress={() => notifications.set(!notifications())}
        />
        <view style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
          <text>{`Text size ${size()}`}</text>
          <Stepper value={size()} min={12} max={24} onChange={(v) => size.set(v)} />
        </view>
      </Group>
    </view>
  );
}

mount(App);
