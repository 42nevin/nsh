// Sheet: a bottom panel dragged down, tapped outside or closed with a button.
import { mount, signal } from "nsh";
import { Button, Sheet } from "nsh/ui/mobile";

function App() {
  const open = signal(false);
  const closes = signal(0);
  const opens = signal(0);
  return (
    <view style={{ flex: 1, backgroundColor: "#ffffff", padding: 16, paddingTop: 24, gap: 12 }}>
      <text style={{ fontSize: 22, fontWeight: "700" }}>Sheet</text>
      <text style={{ color: "#737373" }}>{`Opened ${opens()} times, closed by the user ${closes()} times`}</text>
      <Button title="Open the sheet" onPress={() => {
          open.set(true);
          opens.set(opens() + 1);
        }}
      />
      <Sheet
        open={open()}
        onClose={() => {
          open.set(false);
          closes.set(closes() + 1);
        }}
      >
        <text style={{ fontSize: 17, fontWeight: "600" }}>Share the photo</text>
        <text style={{ color: "#737373" }}>Drag the panel down, tap the dimmed backdrop or press Done.</text>
        <Button title="Copy link" variant="secondary" onPress={() => open.set(false)} />
        <Button title="Done" onPress={() => open.set(false)} />
      </Sheet>
    </view>
  );
}

mount(App);
