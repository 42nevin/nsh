import { createSignal, For, Match, Switch } from "solid-js";
import { keyboard, KeyboardAvoidingView, List, useSafeAreaInsets, type YNode } from "yf";
import { colors } from "../ui";

const tints = ["#fde2e4", "#e2ece9", "#dfe7fd", "#fff1e6", "#e8e8e4", "#cddafd"];

/** Three columns, a header and a footer, padding and gaps, viewability. */
function Grid() {
  const items = Array.from({ length: 90 }, (_, i) => ({ id: i, height: 70 + (i % 4) * 20 }));
  const [visible, setVisible] = createSignal("");
  let list!: YNode;
  return (
    <List
      ref={list}
      each={items}
      numColumns={3}
      estimatedItemSize={90}
      style={{ flex: 1 }}
      contentContainerStyle={{ padding: 12, gap: 8 }}
      viewabilityConfig={{ itemVisiblePercentThreshold: 50 }}
      onViewableItemsChanged={({ viewableItems }) =>
        setVisible(viewableItems.length ? `${viewableItems[0].index}–${viewableItems[viewableItems.length - 1].index}` : "")
      }
      ListHeaderComponent={
        <view style={{ flexDirection: "row", alignItems: "center", gap: 12, paddingBottom: 4 }}>
          <text style={{ flex: 1, color: colors.muted }}>Visible {visible()}</text>
          <text onPress={() => list.scrollToIndex({ index: 60, viewPosition: 0.5 })} style={{ color: colors.accent }}>
            Item 60
          </text>
          <text onPress={() => list.scrollToEnd()} style={{ color: colors.accent }}>
            End
          </text>
        </view>
      }
      ListFooterComponent={<text style={{ textAlign: "center", color: colors.muted, padding: 12 }}>— the end —</text>}
    >
      {(item) => (
        <view
          style={{
            height: item().height,
            borderRadius: 10,
            backgroundColor: tints[item().id % tints.length],
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <text style={{ fontSize: 17, fontWeight: "600" }}>{item().id}</text>
        </view>
      )}
    </List>
  );
}

/** Section headers stick to the top until the next one pushes them out. */
function Contacts() {
  const names = ["Ada", "Alan", "Barbara", "Brian", "Claude", "Dennis", "Donald", "Edsger", "Frances", "Grace", "Guido", "Ken", "Linus", "Margaret", "Niklaus"];
  // One list of header and name rows; the headers' indices stick.
  type Row = { header: string } | { name: string; last: boolean };
  const rows: Row[] = [];
  const headers: number[] = [];
  for (const letter of [...new Set(names.map((n) => n[0]))]) {
    headers.push(rows.length);
    rows.push({ header: letter });
    const group = names.filter((n) => n[0] === letter);
    group.forEach((name, i) => rows.push({ name, last: i === group.length - 1 }));
  }
  return (
    <List each={rows} stickyHeaderIndices={headers} estimatedItemSize={44} style={{ flex: 1, backgroundColor: colors.surface }}>
      {(row) => (
        <Switch>
          <Match when={"header" in row() && (row() as { header: string })}>
            {(r) => (
              <view style={{ backgroundColor: "#eaeef2", paddingHorizontal: 16, paddingVertical: 6 }}>
                <text style={{ fontWeight: "700", color: colors.muted }}>{r().header}</text>
              </view>
            )}
          </Match>
          <Match when={"name" in row() && (row() as { name: string; last: boolean })}>
            {(r) => (
              <view
                style={{
                  paddingHorizontal: 16,
                  paddingVertical: 12,
                  borderBottomWidth: r().last ? 0 : 1,
                  borderBottomColor: "#eaeef2",
                }}
              >
                <text style={{ fontSize: 17 }}>{r().name}</text>
              </view>
            )}
          </Match>
        </Switch>
      )}
    </List>
  );
}

type Message = { id: number; text: string; mine: boolean };

/** An inverted list: starts at the newest message, keeps its place as older ones load. */
function Chat() {
  const insets = useSafeAreaInsets();
  let next = 0;
  const message = (text: string, mine: boolean): Message => ({ id: next++, text, mine });
  const [messages, setMessages] = createSignal(
    Array.from({ length: 30 }, (_, i) => message(`Message ${30 - i}`, i % 3 === 0)),
  );
  const [draft, setDraft] = createSignal("");
  const send = () => {
    if (!draft().trim()) return;
    setMessages((m) => [message(draft(), true), ...m]);
    setDraft("");
  };
  const loadOlder = () => {
    const count = messages().length;
    setMessages((m) => [...m, ...Array.from({ length: 20 }, (_, i) => message(`Older ${count + i + 1}`, i % 4 === 0))]);
  };
  return (
    <KeyboardAvoidingView style={{ flex: 1 }}>
      <List
        each={messages()}
        inverted
        estimatedItemSize={44}
        style={{ flex: 1 }}
        contentContainerStyle={{ padding: 12, gap: 6 }}
        keyboardDismissMode="interactive"
        onEndReached={loadOlder}
        onEndReachedThreshold={0.5}
      >
        {(m) => (
          <view
            style={{
              alignSelf: m().mine ? "flex-end" : "flex-start",
              maxWidth: "75%",
              paddingHorizontal: 12,
              paddingVertical: 8,
              borderRadius: 14,
              backgroundColor: m().mine ? colors.accent : colors.surface,
            }}
          >
            <text style={{ color: m().mine ? "#ffffff" : colors.text, fontSize: 16 }}>{m().text}</text>
          </view>
        )}
      </List>
      <view
        style={{
          flexDirection: "row",
          gap: 8,
          padding: 8,
          paddingBottom: 8 + (keyboard().height > 0 ? 0 : insets().bottom),
          backgroundColor: colors.surface,
        }}
      >
        <input
          value={draft()}
          onChangeText={setDraft}
          onSubmitEditing={send}
          blurOnSubmit={false}
          placeholder="Message"
          returnKeyType="send"
          style={{ flex: 1, fontSize: 16, borderWidth: 1, borderColor: colors.border, borderRadius: 18, paddingHorizontal: 12, paddingVertical: 8 }}
        />
        <text onPress={send} style={{ alignSelf: "center", color: colors.accent, fontSize: 16, fontWeight: "600" }}>
          Send
        </text>
      </view>
    </KeyboardAvoidingView>
  );
}

/** Horizontal scrolling, pull to refresh on a plain scroll view, and an empty list. */
function More() {
  const [refreshing, setRefreshing] = createSignal(false);
  const [refreshed, setRefreshed] = createSignal(0);
  const [items, setItems] = createSignal<number[]>([]);
  const refresh = () => {
    setRefreshing(true);
    setTimeout(() => {
      setRefreshed(refreshed() + 1);
      setRefreshing(false);
    }, 1000);
  };
  const cards = Array.from({ length: 1000 }, (_, i) => i);
  return (
    <scroll style={{ flex: 1 }} refreshing={refreshing()} onRefresh={refresh}>
      <view style={{ paddingVertical: 12, gap: 16 }}>
        <text style={{ marginHorizontal: 12, color: colors.muted }}>Pull down to refresh (refreshed {refreshed()} times)</text>
        <text style={{ marginHorizontal: 12, fontWeight: "600" }}>A horizontal scroll view</text>
        <scroll horizontal showsIndicator={false}>
          <view style={{ flexDirection: "row", gap: 8, paddingHorizontal: 12 }}>
            <For each={["Today", "Yesterday", "This week", "This month", "This year", "All time", "Custom"]}>
              {(label) => (
                <text style={{ paddingHorizontal: 14, paddingVertical: 8, borderRadius: 16, overflow: "hidden", backgroundColor: colors.surface }}>
                  {label}
                </text>
              )}
            </For>
          </view>
        </scroll>
        <text style={{ marginHorizontal: 12, fontWeight: "600" }}>A horizontal list of 1000 recycled cards</text>
        <List each={cards} horizontal estimatedItemSize={130} style={{ height: 120 }} contentContainerStyle={{ paddingHorizontal: 12, gap: 10 }}>
          {(i) => (
            <view style={{ width: 120, height: 120, borderRadius: 12, backgroundColor: tints[i() % tints.length], alignItems: "center", justifyContent: "center" }}>
              <text style={{ fontSize: 20, fontWeight: "700" }}>{i()}</text>
            </view>
          )}
        </List>
        <view style={{ flexDirection: "row", alignItems: "center", marginHorizontal: 12 }}>
          <text style={{ flex: 1, fontWeight: "600" }}>An empty list ({items().length} items)</text>
          <text onPress={() => setItems([...items(), items().length + 1])} style={{ color: colors.accent, marginRight: 16 }}>
            Add
          </text>
          <text onPress={() => setItems([])} style={{ color: colors.accent }}>
            Clear
          </text>
        </view>
        <List
          each={items()}
          estimatedItemSize={44}
          style={{ height: 180, marginHorizontal: 12, borderRadius: 10, backgroundColor: colors.surface }}
          ListEmptyComponent={<text style={{ padding: 24, textAlign: "center", color: colors.muted }}>Nothing here yet: tap Add</text>}
          ItemSeparatorComponent={() => <view style={{ height: 1, backgroundColor: "#eaeef2" }} />}
        >
          {(n) => <text style={{ padding: 12 }}>Item {n()}</text>}
        </List>
      </view>
    </scroll>
  );
}

type Tab = "grid" | "sections" | "chat" | "more";

export function Lists() {
  const [tab, setTab] = createSignal<Tab>("grid");
  const tabButton = (id: Tab, label: string) => (
    <text
      onPress={() => setTab(id)}
      style={{
        flex: 1,
        textAlign: "center",
        paddingVertical: 6,
        borderRadius: 8,
        overflow: "hidden",
        backgroundColor: tab() === id ? colors.surface : "transparent",
        fontWeight: "600",
      }}
    >
      {label}
    </text>
  );
  return (
    <view style={{ flex: 1, backgroundColor: colors.background }}>
      <view style={{ flexDirection: "row", margin: 12, padding: 3, borderRadius: 10, backgroundColor: "#e1e4e8" }}>
        {tabButton("grid", "Grid")}
        {tabButton("sections", "Sections")}
        {tabButton("chat", "Chat")}
        {tabButton("more", "More")}
      </view>
      <Switch>
        <Match when={tab() === "grid"}>
          <Grid />
        </Match>
        <Match when={tab() === "sections"}>
          <Contacts />
        </Match>
        <Match when={tab() === "chat"}>
          <Chat />
        </Match>
        <Match when={tab() === "more"}>
          <More />
        </Match>
      </Switch>
    </view>
  );
}
