// ListView: 1,000 recycled rows the app renders; plain or inset, with or without dividers.
import { mount, signal } from "nsh";
import { Button, ListView } from "nsh/ui/mobile";

const people = Array.from({ length: 1000 }, (_, i) => ({
  id: i,
  name: `Person ${i + 1}`,
  role: ["Design", "Engineering", "Sales", "Support"][i % 4],
}));

function Header(props: { inset: () => boolean; dividers: () => boolean; toggleInset: () => void; toggleDividers: () => void }) {
  return (
    <view style={{ padding: 16, paddingTop: 24, gap: 8 }}>
      <text style={{ fontSize: 22, fontWeight: "700" }}>ListView</text>
      <view style={{ flexDirection: "row", gap: 8 }}>
        <Button class="flex-1" variant="secondary" title={props.inset() ? "Plain" : "Inset"} onPress={props.toggleInset} />
        <Button
          class="flex-1"
          variant="secondary"
          title={props.dividers() ? "No dividers" : "Dividers"}
          onPress={props.toggleDividers}
        />
      </view>
    </view>
  );
}

function App() {
  const inset = signal(false);
  const dividers = signal(true);
  return (
    <ListView
      variant={inset() ? "inset" : "plain"}
      dividers={dividers()}
      each={people}
      keyExtractor={(p) => String(p.id)}
      ListHeaderComponent={
        <Header
          inset={inset}
          dividers={dividers}
          toggleInset={() => inset.set(!inset())}
          toggleDividers={() => dividers.set(!dividers())}
        />
      }
    >
      {(person) => (
        <view style={{ flexDirection: "row", alignItems: "center" }}>
          <text style={{ flex: 1, fontWeight: "600" }}>{person().name}</text>
          <text style={{ color: "#737373" }}>{person().role}</text>
        </view>
      )}
    </ListView>
  );
}

mount(App);
