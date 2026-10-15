// `nsh/list`: the virtualized, recycling list.

import { batch, createMemo, createRoot, createSignal, getOwner, onCleanup, untrack } from "solid-js";
import {
  command,
  createComponent,
  createElement,
  detachNode,
  effect,
  insert,
  insertNode,
  setProp,
  YNode,
} from "./_core";
import type { NativeValue } from "./animated";
import type { JSX, ScrollBehaviorProps, StyleProp, ViewStyle } from "./types";

// --- Virtualized list ----------------------------------------------------

type ListBinder = (flat: number[]) => void;
const lists = new Map<number, ListBinder>();

/** A component (`Header`) or an element (`<Header />`). */
export type ListSlot = (() => JSX.Element) | JSX.Element;

/** An item that became viewable or stopped being viewable. */
export interface ViewToken<T> {
  item: T;
  key: string;
  index: number;
  isViewable: boolean;
}

export interface ViewabilityConfig {
  /** Percent of the viewport an item must cover to count as viewable. */
  viewAreaCoveragePercentThreshold?: number;
  /** Percent of an item that must be visible. Without either threshold, any visible pixel counts. */
  itemVisiblePercentThreshold?: number;
  /** Milliseconds items must stay viewable before they are reported. */
  minimumViewTime?: number;
}

export interface ListProps<T> extends ScrollBehaviorProps {
  each: readonly T[];
  /** Size used for items not measured yet (points). */
  estimatedItemSize?: number;
  horizontal?: boolean;
  /** Items per row (vertical lists). */
  numColumns?: number;
  /** Laid out from the end, item 0 last (chat style); starts scrolled to the end and keeps its distance from the end as content changes. */
  inverted?: boolean;
  /** Items that stick to the top while scrolled past (one column, not inverted). */
  stickyHeaderIndices?: number[];
  ListHeaderComponent?: ListSlot;
  ListFooterComponent?: ListSlot;
  /** Shown instead of the items while `each` is empty. */
  ListEmptyComponent?: ListSlot;
  /** Rendered between items (one column). */
  ItemSeparatorComponent?: ListSlot;
  /** Padding (`padding`, `paddingHorizontal`, `paddingTop`, …) and `gap` / `rowGap` / `columnGap` of the content. */
  contentContainerStyle?: {
    padding?: number;
    paddingHorizontal?: number;
    paddingVertical?: number;
    paddingTop?: number;
    paddingRight?: number;
    paddingBottom?: number;
    paddingLeft?: number;
    gap?: number;
    rowGap?: number;
    columnGap?: number;
  };
  /** Scrolls to this item before the first frame. */
  initialScrollIndex?: number;
  /** Key of an item in `onViewableItemsChanged` tokens (default: `item.key`, `item.id`, or the index). */
  keyExtractor?: (item: T, index: number) => string;
  onViewableItemsChanged?: (info: { viewableItems: ViewToken<T>[]; changed: ViewToken<T>[] }) => void;
  viewabilityConfig?: ViewabilityConfig;
  style?: StyleProp<ViewStyle>;
  showsIndicator?: boolean;
  refreshing?: boolean;
  onRefresh?: () => void;
  onEndReached?: () => void;
  /** How close to the end (in viewports) `onEndReached` fires; default 1. */
  onEndReachedThreshold?: number;
  onScrollEnd?: (e: { x: number; y: number }) => void;
  /** Native value fed with the scroll offset, for scroll-linked effects. */
  scrollValue?: NativeValue;
  /** Whether dragging the list dismisses the keyboard. */
  keyboardDismissMode?: "none" | "on-drag" | "interactive";
  ref?: YNode | ((node: YNode) => void);
  /** Renders one row. Called once per recycled cell; `item` and `index`
   * change when the cell is reused for another item. */
  children: (item: () => T, index: () => number) => JSX.Element;
}

/** Renders a list slot prop: a component is called, an element used as is. */
function slot(value: ListSlot): any {
  return typeof value === "function" && !(value instanceof YNode) ? untrack(() => createComponent(value as unknown as (props: {}) => YNode, {})) : value;
}

/**
 * A virtualized, recycling list on a native scroll view. Only rows near the
 * viewport exist; the native side decides which, and cells are reused.
 * Positions, columns, sticky headers and viewability are computed natively.
 *
 * Rows are recycled: keep per-item state in your data (keyed by id), not in
 * signals created inside the row.
 */
export function List<T>(props: ListProps<T>): YNode {
  const list = createElement("list");
  const owner = getOwner();
  const cells: { node: YNode; setIndex: (i: number) => void; dispose: () => void }[] = [];

  // Tells the core how the items changed (by identity: the common start and
  // end stay), so measurements and cells follow their items and items added
  // above the viewport do not move what is on screen.
  let shown: readonly T[] = [];
  effect(() => {
    const next = props.each;
    const old = shown;
    shown = next;
    if (old.length > 0 && next !== old) {
      const max = Math.min(old.length, next.length);
      let start = 0;
      while (start < max && old[start] === next[start]) start++;
      let end = 0;
      while (end < max - start && old[old.length - 1 - end] === next[next.length - 1 - end]) end++;
      const removed = old.length - start - end;
      const inserted = next.length - start - end;
      if (removed > 0 || inserted > 0) setProp(list, "splice", [start, removed, inserted]);
    }
    setProp(list, "count", next.length);
  });
  effect(() => setProp(list, "estimatedItemSize", props.estimatedItemSize ?? 50));
  for (const name of [
    "style", "horizontal", "showsIndicator", "refreshing", "onRefresh", "onEndReached", "onScrollEnd", "scrollValue",
    "keyboardDismissMode", "numColumns", "inverted", "stickyHeaderIndices", "contentContainerStyle", "viewabilityConfig",
    "pagingEnabled", "snapToInterval", "snapToOffsets", "snapToAlignment", "decelerationRate", "scrollEnabled", "bounces",
    "contentInset", "contentOffset", "keyboardShouldPersistTaps", "scrollEventThrottle", "disableIntervalMomentum",
    "onScroll", "onScrollBeginDrag", "onScrollEndDrag", "onMomentumScrollBegin", "onMomentumScrollEnd",
  ] as const) {
    effect((prev: unknown) => {
      const value = (props as any)[name];
      if (value !== prev) setProp(list, name, value, prev);
      return value;
    });
  }
  // Not an event: `on*` props are handlers to the renderer.
  effect(() => setProp(list, "endReachedThreshold", props.onEndReachedThreshold ?? 1));

  // A new cell renders its first item right away: rows never see a
  // missing item.
  const createCell = (slotIndex: number, first: number) =>
    createRoot((dispose) => {
      const [index, setIndex] = createSignal(first);
      // Past the end (the data shrank before the core unbound the cell):
      // keep showing the last item; the core hides unbound cells.
      const item = createMemo((prev: T) => {
        const i = index();
        return i >= 0 && i < props.each.length ? props.each[i] : prev;
      }, first >= 0 && first < props.each.length ? props.each[first] : (undefined as T));
      const node = createElement("cell");
      setProp(node, "slot", slotIndex);
      insertNode(list, node);
      const content = untrack(() => props.children(item, index));
      if ("ItemSeparatorComponent" in props) {
        // Between items: after each one but the last (before it when
        // inverted, as the list runs upwards).
        let separator: any;
        const shown = createMemo(() => index() < props.each.length - 1 && props.ItemSeparatorComponent != null);
        insert(node, () => {
          if (!shown()) return content;
          separator ??= slot(props.ItemSeparatorComponent);
          return untrack(() => props.inverted) ? [separator, content] : [content, separator];
        });
      } else {
        insert(node, content);
      }
      return { node, setIndex, dispose };
    }, owner!);

  // Header and footer cells (always rendered), and the empty placeholder
  // while there are no items.
  const extraCell = (role: string, value: () => ListSlot) =>
    createRoot((dispose) => {
      const node = createElement("cell");
      setProp(node, "slot", role);
      insertNode(list, node);
      insert(node, () => slot(value()));
      return dispose;
    }, owner!);
  const extras: (() => void)[] = [];
  if ("ListHeaderComponent" in props) extras.push(extraCell("header", () => props.ListHeaderComponent));
  if ("ListFooterComponent" in props) extras.push(extraCell("footer", () => props.ListFooterComponent));
  if ("ListEmptyComponent" in props) {
    let disposeEmpty: (() => void) | null = null;
    let emptyNode: YNode | null = null;
    effect(() => {
      if (props.each.length === 0 && !disposeEmpty) {
        disposeEmpty = createRoot((dispose) => {
          emptyNode = createElement("cell");
          setProp(emptyNode, "slot", "empty");
          insertNode(list, emptyNode);
          insert(emptyNode, () => slot(props.ListEmptyComponent));
          return dispose;
        }, owner!);
      } else if (props.each.length > 0 && disposeEmpty) {
        disposeEmpty();
        detachNode(list, emptyNode!);
        disposeEmpty = emptyNode = null;
      }
    });
    extras.push(() => disposeEmpty?.());
  }

  if (props.onViewableItemsChanged) {
    let reported: [number, number] = [-1, -1];
    let timer: number | undefined;
    let latest: [number, number] = [-1, -1];
    const report = () => {
      timer = undefined;
      const [first, last] = latest;
      const [prevFirst, prevLast] = reported;
      if (first === prevFirst && last === prevLast) return;
      const each = props.each;
      const keyOf = props.keyExtractor;
      const token = (index: number, isViewable: boolean): ViewToken<T> => {
        const item = each[index];
        const key = keyOf ? keyOf(item, index) : String((item as any)?.key ?? (item as any)?.id ?? index);
        return { item, key, index, isViewable };
      };
      const viewableItems: ViewToken<T>[] = [];
      const changed: ViewToken<T>[] = [];
      if (first >= 0) for (let i = first; i <= last && i < each.length; i++) viewableItems.push(token(i, true));
      if (prevFirst >= 0) {
        for (let i = prevFirst; i <= prevLast && i < each.length; i++) {
          if (i < first || i > last) changed.push(token(i, false));
        }
      }
      for (const t of viewableItems) if (t.index < prevFirst || t.index > prevLast) changed.push(t);
      reported = latest;
      props.onViewableItemsChanged!({ viewableItems, changed });
    };
    setProp(list, "onViewableItemsChanged", (e: { first: number; last: number }) => {
      latest = [e.first, e.last];
      const wait = props.viewabilityConfig?.minimumViewTime ?? 0;
      if (wait <= 0) return report();
      if (timer !== undefined) clearTimeout(timer);
      timer = setTimeout(report, wait) as unknown as number;
    });
  }

  if (props.initialScrollIndex != null) {
    command(list.id, "scrollToIndex", { index: props.initialScrollIndex, animated: false });
  }
  if (typeof props.ref === "function") props.ref(list);

  lists.set(list.id, (flat) => {
    batch(() => {
      for (let i = 0; i < flat.length; i += 2) {
        const slotIndex = flat[i];
        const index = flat[i + 1];
        const cell = cells[slotIndex];
        if (cell) cell.setIndex(index);
        else cells[slotIndex] = createCell(slotIndex, index);
      }
    });
    // After the batch: the cells' new content is already in the op buffer.
    for (let i = 0; i < flat.length; i += 2) setProp(cells[flat[i]].node, "cellIndex", flat[i + 1]);
  });
  onCleanup(() => {
    lists.delete(list.id);
    for (const cell of cells) cell?.dispose();
    for (const dispose of extras) dispose();
  });
  return list;
}


(globalThis as any).__nsh_list = (id: number, flat: number[]) => lists.get(id)?.(flat);
