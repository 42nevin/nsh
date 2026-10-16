// Card: elevated and outlined, with a header and a footer slot.
import { mount, signal } from "nsh";
import { Button, Card } from "nsh/ui/mobile";

function App() {
  const likes = signal(0);
  return (
    <scroll style={{ flex: 1, backgroundColor: "#ffffff" }}>
      <view style={{ padding: 16, paddingTop: 24, gap: 16 }}>
        <text style={{ fontSize: 22, fontWeight: "700" }}>Card</text>
        <Card
          header={<text>Weekly summary</text>}
          footer={
            <view style={{ flexDirection: "row", gap: 8 }}>
              <Button title="Reset" variant="plain" onPress={() => likes.set(0)} />
              <Button title={`Like (${likes()})`} onPress={() => likes.set(likes() + 1)} />
            </view>
          }
        >
          <text>You walked 42 km this week, 8 km more than last week.</text>
          <text style={{ color: "#737373" }}>The header, this body and the footer are the app's JSX.</text>
        </Card>
        <Card variant="outlined">
          <text style={{ fontWeight: "600" }}>Outlined</text>
          <text>A border, no shadow, and no header or footer: only children.</text>
        </Card>
        <Card
          variant="outlined"
          header={
            <view style={{ flexDirection: "row", alignItems: "center", gap: 10 }}>
              <view style={{ width: 32, height: 32, borderRadius: 16, backgroundColor: "#a78bfa" }} />
              <text>Ada Lovelace</text>
            </view>
          }
        >
          <text>A header can be any row, such as an avatar and a name.</text>
        </Card>
      </view>
    </scroll>
  );
}

mount(App);
