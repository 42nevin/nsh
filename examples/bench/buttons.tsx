// The two benchmark screens, parameterized by the Button under test, so the
// Rust component (`nsh/ui/mobile`) and a TSX composition (`TsButton.tsx`) run
// the same app.
import { signal, type Component } from "nsh";
import { List } from "nsh/list";

/** The props both Buttons take. */
export interface BenchButtonProps {
  title: string;
  variant?: "primary" | "secondary" | "plain";
  onPress?: () => void;
}

const rows = Array.from({ length: 200 }, (_, i) => i);

/** 200 Buttons in a scroll view. The first press target bumps a counter
 * that every title shows: one tap updates all 200 titles. */
export function Grid(props: { Button: Component<BenchButtonProps> }) {
  const count = signal(0);
  const Button = props.Button;
  return (
    <scroll style={{ flex: 1 }}>
      <view style={{ padding: 16, gap: 8 }}>
        <view onPress={() => count.update((c) => c + 1)} style={{ padding: 12 }}>
          <text>Bump</text>
        </view>
        <each of={rows}>
          {(i) => <Button title={`Row ${i()} · ${count()}`} variant={i() % 3 === 0 ? "primary" : i() % 3 === 1 ? "secondary" : "plain"} />}
        </each>
      </view>
    </scroll>
  );
}

const items = Array.from({ length: 1000 }, (_, i) => i);

/** A recycled list of 1000 rows with a Button each: scrolling rebinds rows. */
export function Rows(props: { Button: Component<BenchButtonProps> }) {
  const Button = props.Button;
  return (
    <List each={items} estimatedItemSize={64} style={{ flex: 1 }}>
      {(i) => (
        <view style={{ flexDirection: "row", alignItems: "center", paddingHorizontal: 16, paddingVertical: 8, gap: 12 }}>
          <text style={{ flex: 1 }}>{`Item ${i()}`}</text>
          <Button title={`Open ${i()}`} variant={i() % 2 === 0 ? "primary" : "secondary"} />
        </view>
      )}
    </List>
  );
}
