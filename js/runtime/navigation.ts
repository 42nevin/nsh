// `nsh/navigation`: native stacks of screens, and modals.

import {
  type Accessor,
  children as resolveChildren,
  createMemo,
  createSignal,
  mapArray,
  mergeProps as mergeSolidProps,
  splitProps,
  untrack,
} from "solid-js";
import {
  createComponent,
  createElement,
  effect,
  insert,
  InsetsContext,
  nativeInsets,
  safeArea,
  setProp,
  spread,
  YNode,
} from "./_core";
import type { JSX, StyleProp, ViewStyle } from "./types";

// --- Navigation -------------------------------------------------------------

/** A stack of routes for `<Stack>`: plain values, compared by identity. */
export interface Navigator<R> {
  /** The routes, bottom to top (reactive). */
  routes: Accessor<readonly R[]>;
  /** Pushes a screen. */
  push(route: R): void;
  /** Pops `count` screens (at least one stays). */
  pop(count?: number): void;
  /** Pops back to the first screen. */
  popToTop(): void;
  /** Replaces the top screen. */
  replace(route: R): void;
  /** Replaces all screens. */
  reset(routes: R[]): void;
  /** Removes a route wherever it is (the user popped its screen). */
  remove(route: R): void;
}

/** Creates the route stack of a `<Stack>`. */
export function createNavigator<R>(initial: R | R[]): Navigator<R> {
  const [routes, setRoutes] = createSignal<readonly R[]>(Array.isArray(initial) ? initial : [initial]);
  return {
    routes,
    push: (route) => setRoutes((r) => [...r, route]),
    pop: (count = 1) => setRoutes((r) => r.slice(0, Math.max(1, r.length - count))),
    popToTop: () => setRoutes((r) => r.slice(0, 1)),
    replace: (route) => setRoutes((r) => [...r.slice(0, -1), route]),
    reset: (rs) => setRoutes(rs),
    remove: (route) => setRoutes((r) => r.filter((x) => x !== route)),
  };
}

export interface StackProps<R> {
  navigator: Navigator<R>;
  style?: StyleProp<ViewStyle>;
  /** Renders a route; return a `<Screen>` for header options. */
  children: (route: R, index: Accessor<number>) => JSX.Element;
}

/**
 * A native navigation stack: UINavigationController on iOS (swipe back,
 * native header), a stack with a toolbar and predictive back on Android.
 * Screens stay mounted (with their state) while they are in the stack.
 */
export function Stack<R>(props: StackProps<R>): YNode {
  const stack = createElement("stack");
  effect((prev: unknown) => {
    const style = props.style;
    if (style !== prev) setProp(stack, "style", style, prev);
    return style;
  });
  const screens = mapArray(
    () => props.navigator.routes(),
    (route, index) => {
      const screen = (rendered: JSX.Element) => {
        const node = rendered instanceof YNode && rendered.tag === "screen" ? rendered : Screen({ children: rendered });
        // The user popped it natively: forget the route.
        node.popped = () => props.navigator.remove(route);
        return node;
      };
      const rendered = untrack(() => props.children(route, index));
      // An accessor (e.g. a component made hot in development) may render
      // another screen later.
      if (typeof rendered === "function") return createMemo(() => screen(rendered()));
      return screen(rendered);
    },
  );
  insert(stack, screens);
  return stack;
}

export interface ScreenProps {
  title?: string;
  headerShown?: boolean;
  /** The content extends under the header. */
  headerTransparent?: boolean;
  headerTintColor?: string;
  headerBackgroundColor?: string;
  headerTitleColor?: string;
  headerBackVisible?: boolean;
  headerLeft?: JSX.Element;
  headerRight?: JSX.Element;
  /** Replaces the title. */
  headerTitle?: JSX.Element;
  /** The user can pop the screen (swipe back, back button). */
  gestureEnabled?: boolean;
  animation?: "default" | "fade" | "none";
  style?: StyleProp<ViewStyle>;
  onDismiss?: () => void;
  children?: JSX.Element;
  ios?: {
    /** A large title; the content scrolls under the header. */
    headerLargeTitle?: boolean;
    /** Label of the back button leading back to this screen. */
    headerBackTitle?: string;
  };
}

/** One screen of a `<Stack>`, with its header options. */
export function Screen(props: ScreenProps): YNode {
  const node = createElement("screen");
  node.tag = "screen";
  const [local, others] = splitProps(props, ["children", "headerLeft", "headerRight", "headerTitle", "onDismiss"]);
  const { insets, onInsets } = nativeInsets(() => ({ ...safeArea(), top: 0 }));
  spread(
    node,
    mergeSolidProps(others, {
      onDismiss() {
        node.popped?.();
        local.onDismiss?.();
      },
      onInsets,
    }),
    true,
  );
  const slot = (name: string, content: Accessor<any>) =>
    createMemo(() => {
      const c = content();
      if (c == null || c === false) return null;
      const item = createElement("headeritem");
      setProp(item, "headerSlot", name);
      insert(item, c);
      return item;
    });
  const left = slot("left", resolveChildren(() => local.headerLeft));
  const right = slot("right", resolveChildren(() => local.headerRight));
  const title = slot("title", resolveChildren(() => local.headerTitle));
  insert(node, () => [
    left(),
    right(),
    title(),
    createComponent(InsetsContext.Provider, {
      value: insets,
      get children() {
        return local.children;
      },
    }),
  ]);
  return node;
}

export interface ModalProps {
  /** Shown while true (default). */
  visible?: boolean;
  animationType?: "slide" | "fade" | "none";
  /** The background shows what is behind. */
  transparent?: boolean;
  /** The user asked to close (Android back button, iOS sheet swipe): set
   * `visible` to false to close it. */
  onRequestClose?: () => void;
  onShow?: () => void;
  onDismiss?: () => void;
  style?: StyleProp<ViewStyle>;
  children?: JSX.Element;
  ios?: {
    /** `"pageSheet"` / `"formSheet"` keep what is behind visible. */
    presentationStyle?: "fullScreen" | "pageSheet" | "formSheet";
    /** The orientations it allows (default: the presenter's). */
    supportedOrientations?: ("portrait" | "portrait-upside-down" | "landscape" | "landscape-left" | "landscape-right")[];
  };
  android?: {
    /** The content reaches under the status bar (default: it starts below). */
    statusBarTranslucent?: boolean;
  };
}

/** Content presented over everything else, in its own native window layer. */
export function Modal(props: ModalProps): any {
  return createMemo(() => {
    if (props.visible === false) return null;
    return untrack(() => {
      const node = createElement("modal");
      const [local, others] = splitProps(props, ["children", "visible"]);
      const { insets, onInsets } = nativeInsets(() => safeArea());
      spread(node, mergeSolidProps(others, { onInsets }), true);
      insert(node, () =>
        createComponent(InsetsContext.Provider, {
          value: insets,
          get children() {
            return local.children;
          },
        }),
      );
      return node;
    });
  });
}
