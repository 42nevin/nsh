// `nsh/ui/mobile`'s Rust components laying out the app's content: a
// `ListView` recycles rows the app renders, a `Group` wraps each child in a
// row. The app writes the content; the components only place it.
import { signal } from "nsh";
import { Button, Group, ListView, SelectList, Sheet, Stepper, type StepperRef, TextField } from "nsh/ui/mobile";

const inset = signal(false);

const cities = Array.from({ length: 500 }, (_, i) => `City ${i + 1}`);

const stocks = Array.from({ length: 300 }, (_, i) => ({
  id: i,
  symbol: `SYM${String(i).padStart(3, "0")}`,
  price: (50 + ((i * 37) % 400) + (i % 7) * 0.13).toFixed(2),
  change: (((i * 13) % 21) - 10) / 4,
}));

function Header() {
  const taps = signal(0);
  const quantity = signal(2);
  const asked = signal("");
  let own!: StepperRef;
  const name = signal("Ada");
  const sheet = signal(false);
  const city = signal<number | null>(null);
  return (
    <view style={{ padding: 16, gap: 16 }}>
      <TextField label="Name (controlled; × clears and focuses)" placeholder="Your name" value={name()} onChange={(v) => name.set(v)} />
      <TextField label="Password" placeholder="At least 8 characters" secure />
      <text style={{ fontSize: 13, color: "#57606a" }}>{`Hello, ${name() || "nobody"}`}</text>
      <Button title="Open the sheet (drag it down)" onPress={() => sheet.set(true)} />
      <Sheet open={sheet()} onClose={() => sheet.set(false)}>
        <text style={{ fontSize: 17, fontWeight: "600" }}>A sheet in Rust</text>
        <text style={{ color: "#57606a" }}>It follows the finger natively and springs back; drag it down, tap outside or press back to close.</text>
        <Button title="Done" onPress={() => sheet.set(false)} />
      </Sheet>
      <text style={{ fontSize: 13, color: "#57606a" }}>{`SelectList: 500 rows of Rust data, chosen: ${city() === null ? "none" : cities[city()!]}`}</text>
      <view style={{ height: 240, borderRadius: 12, overflow: "hidden" }}>
        <SelectList options={cities} value={city()} onSelect={(v) => city.set(v)} />
      </view>
      <Group header={<text>Group: each child is a row</text>} footer={<text>The rows are JSX; the group adds the box and the dividers.</text>}>
        <text>Notifications</text>
        <text>Privacy</text>
        <view style={{ flexDirection: "row", justifyContent: "space-between" }}>
          <text>Taps</text>
          <text>{taps()}</text>
        </view>
      </Group>
      <Group header={<text>Stepper: state and timers in Rust (hold a button)</text>}>
        <view style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
          <text>Own value</text>
          <Stepper ref={own} defaultValue={5} min={0} max={50} />
        </view>
        <view style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
          <text>{`Controlled: ${quantity()}`}</text>
          <Stepper value={quantity()} min={0} max={10} step={0.5} onChange={(v) => quantity.set(v)} />
        </view>
      </Group>
      <Button
        title={asked() || "Ask the stepper, then reset it"}
        variant="secondary"
        onPress={async () => {
          // Methods of a Rust component, called from JS.
          asked.set(`It was ${await own.value()}; reset to 5`);
          own.setValue(5);
        }}
      />
      <Button title="Tap" variant="secondary" onPress={() => taps.set(taps() + 1)} />
      <Button title={inset() ? "Plain rows" : "Inset rows"} variant="plain" onPress={() => inset.set(!inset())} />
      <text style={{ fontSize: 13, color: "#57606a" }}>ListView below: 300 rows the app renders, recycled natively.</text>
    </view>
  );
}

export function MobileUi() {
  return (
    <ListView
      variant={inset() ? "inset" : "plain"}
      each={stocks}
      keyExtractor={(s) => String(s.id)}
      ListHeaderComponent={Header}
    >
      {(stock) => (
        <view style={{ flexDirection: "row", alignItems: "center" }}>
          <text style={{ flex: 1, fontWeight: "600" }}>{stock().symbol}</text>
          <text style={{ width: 80, textAlign: "right" }}>{stock().price}</text>
          <text style={{ width: 64, textAlign: "right", color: stock().change >= 0 ? "#1a7f37" : "#cf222e" }}>
            {(stock().change >= 0 ? "+" : "") + stock().change.toFixed(2)}%
          </text>
        </view>
      )}
    </ListView>
  );
}
