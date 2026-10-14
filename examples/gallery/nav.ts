import { createNavigator, type JSX } from "yf";

/** A demo of the index. */
export interface DemoEntry {
  name: string;
  detail: string;
  screen: () => JSX.Element;
}

/** A screen a demo pushes: a component rendering its own `<Screen>`. */
export type Route = { name: "home" } | { name: "demo"; demo: DemoEntry } | { name: "page"; page: () => JSX.Element };

export const nav = createNavigator<Route>({ name: "home" });
