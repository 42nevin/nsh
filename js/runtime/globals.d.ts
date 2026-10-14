// Globals of the yf JS environment (QuickJS-NG plus yf's own): what an app
// can use besides the ECMAScript library. There is no DOM.
//
// Keep in sync with `crates/yf-js/src/console.rs`, `host.rs` (timers,
// frames) and `web.js` (fetch and friends).

declare var console: {
  log(...data: unknown[]): void;
  info(...data: unknown[]): void;
  debug(...data: unknown[]): void;
  warn(...data: unknown[]): void;
  error(...data: unknown[]): void;
};

declare var performance: {
  /** Milliseconds since the JS context started, with sub-millisecond precision. */
  now(): number;
};

declare function setTimeout(handler: (...args: any[]) => void, timeout?: number, ...args: any[]): number;
declare function setInterval(handler: (...args: any[]) => void, timeout?: number, ...args: any[]): number;
declare function clearTimeout(id: number | undefined): void;
declare function clearInterval(id: number | undefined): void;
declare function queueMicrotask(callback: () => void): void;
/** Runs `callback` at the next display frame with its time in milliseconds. */
declare function requestAnimationFrame(callback: (time: number) => void): number;
declare function cancelAnimationFrame(id: number): void;

declare class TextEncoder {
  readonly encoding: "utf-8";
  encode(input?: string): Uint8Array;
  encodeInto(source: string, destination: Uint8Array): { read: number; written: number };
}

declare class TextDecoder {
  constructor(label?: string, options?: { fatal?: boolean; ignoreBOM?: boolean });
  readonly encoding: string;
  readonly fatal: boolean;
  readonly ignoreBOM: boolean;
  decode(input?: ArrayBuffer | ArrayBufferView, options?: { stream?: boolean }): string;
}

declare class URLSearchParams {
  constructor(init?: string | Record<string, string> | [string, string][] | URLSearchParams);
  readonly size: number;
  append(name: string, value: string): void;
  delete(name: string, value?: string): void;
  get(name: string): string | null;
  getAll(name: string): string[];
  has(name: string, value?: string): boolean;
  set(name: string, value: string): void;
  sort(): void;
  forEach(callback: (value: string, key: string, parent: URLSearchParams) => void, thisArg?: unknown): void;
  entries(): IterableIterator<[string, string]>;
  keys(): IterableIterator<string>;
  values(): IterableIterator<string>;
  [Symbol.iterator](): IterableIterator<[string, string]>;
  toString(): string;
}

declare class URL {
  constructor(url: string | URL, base?: string | URL);
  static canParse(url: string | URL, base?: string | URL): boolean;
  static parse(url: string | URL, base?: string | URL): URL | null;
  href: string;
  readonly origin: string;
  protocol: string;
  username: string;
  password: string;
  host: string;
  hostname: string;
  port: string;
  pathname: string;
  search: string;
  hash: string;
  readonly searchParams: URLSearchParams;
  toString(): string;
  toJSON(): string;
}

declare class Event {
  constructor(type: string, init?: { cancelable?: boolean });
  readonly type: string;
  readonly cancelable: boolean;
  readonly defaultPrevented: boolean;
  preventDefault(): void;
  stopPropagation(): void;
  stopImmediatePropagation(): void;
}

declare class EventTarget {
  addEventListener(type: string, listener: ((event: Event) => void) | null, options?: { once?: boolean } | boolean): void;
  removeEventListener(type: string, listener: ((event: Event) => void) | null): void;
  dispatchEvent(event: Event): boolean;
}

declare class AbortSignal extends EventTarget {
  readonly aborted: boolean;
  readonly reason: any;
  onabort: ((event: Event) => void) | null;
  throwIfAborted(): void;
  static abort(reason?: any): AbortSignal;
  static timeout(milliseconds: number): AbortSignal;
  static any(signals: AbortSignal[]): AbortSignal;
}

declare class AbortController {
  readonly signal: AbortSignal;
  abort(reason?: any): void;
}

type HeadersInit = Headers | Record<string, string> | [string, string][];

declare class Headers {
  constructor(init?: HeadersInit);
  append(name: string, value: string): void;
  delete(name: string): void;
  get(name: string): string | null;
  getSetCookie(): string[];
  has(name: string): boolean;
  set(name: string, value: string): void;
  forEach(callback: (value: string, key: string, parent: Headers) => void, thisArg?: unknown): void;
  entries(): IterableIterator<[string, string]>;
  keys(): IterableIterator<string>;
  values(): IterableIterator<string>;
  [Symbol.iterator](): IterableIterator<[string, string]>;
}

/** Bodies are buffered: no streams, no Blob, no FormData. */
type BodyInit = string | ArrayBuffer | ArrayBufferView | URLSearchParams;

interface Body {
  /** Always null: bodies are read whole. */
  readonly body: null;
  readonly bodyUsed: boolean;
  arrayBuffer(): Promise<ArrayBuffer>;
  bytes(): Promise<Uint8Array>;
  text(): Promise<string>;
  json(): Promise<any>;
}

interface RequestInit {
  method?: string;
  headers?: HeadersInit;
  body?: BodyInit | null;
  signal?: AbortSignal | null;
  redirect?: "follow" | "manual" | "error";
}

declare class Request implements Body {
  constructor(input: string | URL | Request, init?: RequestInit);
  readonly method: string;
  readonly url: string;
  readonly headers: Headers;
  readonly signal: AbortSignal;
  readonly redirect: "follow" | "manual" | "error";
  readonly credentials: string;
  readonly mode: string;
  readonly body: null;
  readonly bodyUsed: boolean;
  arrayBuffer(): Promise<ArrayBuffer>;
  bytes(): Promise<Uint8Array>;
  text(): Promise<string>;
  json(): Promise<any>;
  clone(): Request;
}

interface ResponseInit {
  status?: number;
  statusText?: string;
  headers?: HeadersInit;
}

declare class Response implements Body {
  constructor(body?: BodyInit | null, init?: ResponseInit);
  static json(data: unknown, init?: ResponseInit): Response;
  static error(): Response;
  static redirect(url: string | URL, status?: number): Response;
  readonly type: "basic" | "default" | "error";
  readonly url: string;
  readonly redirected: boolean;
  readonly status: number;
  readonly ok: boolean;
  readonly statusText: string;
  readonly headers: Headers;
  readonly body: null;
  readonly bodyUsed: boolean;
  arrayBuffer(): Promise<ArrayBuffer>;
  bytes(): Promise<Uint8Array>;
  text(): Promise<string>;
  json(): Promise<any>;
  clone(): Response;
}

/** HTTP through the platform's network stack (NSURLSession, HttpURLConnection). */
declare function fetch(input: string | URL | Request, init?: RequestInit): Promise<Response>;

// Imported files: images are asset descriptors (sized in points, `@2x`
// files at half their pixel size), fonts the name of their family.
declare module "*.png" {
  const asset: { uri: string; width: number; height: number };
  export default asset;
}
declare module "*.jpg" {
  const asset: { uri: string; width: number; height: number };
  export default asset;
}
declare module "*.jpeg" {
  const asset: { uri: string; width: number; height: number };
  export default asset;
}
declare module "*.gif" {
  const asset: { uri: string; width: number; height: number };
  export default asset;
}
declare module "*.webp" {
  const asset: { uri: string; width: number; height: number };
  export default asset;
}
declare module "*.heic" {
  const asset: { uri: string; width: number; height: number };
  export default asset;
}
declare module "*.ttf" {
  const family: string;
  export default family;
}
declare module "*.otf" {
  const family: string;
  export default family;
}
