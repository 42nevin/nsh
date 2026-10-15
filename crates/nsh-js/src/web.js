// Web platform APIs missing from QuickJS: TextEncoder / TextDecoder, URL /
// URLSearchParams, EventTarget / Event, AbortController / AbortSignal,
// Headers, Request, Response and fetch.
//
// Runs as a classic script before the app bundle. The natives it builds on
// are captured here and removed from the global object. Heavy lifting
// (UTF-8, URL parsing, networking) is done in Rust.
(() => {
  "use strict";
  const g = globalThis;
  const utf8Encode = g.__nsh_utf8_encode;
  const utf8Decode = g.__nsh_utf8_decode;
  const urlNative = g.__nsh_url;
  const fetchNative = g.__nsh_fetch;
  const abortNative = g.__nsh_fetch_abort;
  for (const name of ["__nsh_utf8_encode", "__nsh_utf8_decode", "__nsh_url", "__nsh_fetch", "__nsh_fetch_abort"]) {
    delete g[name];
  }

  // Replaces the lazy accessor of `name`, unless the app assigned its own.
  const define = (name, value) => {
    const current = Object.getOwnPropertyDescriptor(g, name);
    if (current && !current.get) return;
    Object.defineProperty(g, name, { value, writable: true, configurable: true, enumerable: false });
  };
  const hidden = (obj, values) => {
    for (const key of Object.keys(values)) {
      Object.defineProperty(obj, key, { value: values[key], writable: true, configurable: true });
    }
  };

  // --- Encoding -----------------------------------------------------------

  const viewBytes = (input) => {
    if (input === undefined) return [new ArrayBuffer(0), 0, 0];
    if (input instanceof ArrayBuffer) return [input, 0, input.byteLength];
    if (ArrayBuffer.isView(input)) return [input.buffer, input.byteOffset, input.byteLength];
    throw new TypeError("The provided value is not an ArrayBuffer or ArrayBufferView");
  };

  class TextEncoder {
    get encoding() {
      return "utf-8";
    }
    encode(input = "") {
      return new Uint8Array(utf8Encode(String(input)));
    }
    encodeInto(source, destination) {
      const bytes = this.encode(source);
      const written = Math.min(bytes.length, destination.length);
      destination.set(bytes.subarray(0, written));
      return { read: written === bytes.length ? String(source).length : undefined, written };
    }
  }

  class TextDecoder {
    #fatal;
    #ignoreBOM;
    constructor(label = "utf-8", options = {}) {
      const l = String(label).trim().toLowerCase();
      if (l !== "utf-8" && l !== "utf8" && l !== "unicode-1-1-utf-8") {
        throw new RangeError(`The encoding "${label}" is not supported`);
      }
      this.#fatal = !!options.fatal;
      this.#ignoreBOM = !!options.ignoreBOM;
    }
    get encoding() {
      return "utf-8";
    }
    get fatal() {
      return this.#fatal;
    }
    get ignoreBOM() {
      return this.#ignoreBOM;
    }
    decode(input, _options) {
      const [buffer, offset, length] = viewBytes(input);
      return utf8Decode(buffer, offset, length, this.#fatal, this.#ignoreBOM);
    }
  }

  // --- URL ------------------------------------------------------------------

  // Encodes like application/x-www-form-urlencoded (WHATWG URL).
  const formEncode = (s) =>
    encodeURIComponent(s)
      .replace(/%20/g, "+")
      .replace(/[!'()~]/g, (c) => "%" + c.charCodeAt(0).toString(16).toUpperCase());
  const formDecode = (s) => {
    const t = s.replace(/\+/g, " ");
    try {
      return decodeURIComponent(t);
    } catch {
      return t;
    }
  };

  class URLSearchParams {
    #list = [];
    #url = null;
    constructor(init = "") {
      if (typeof init === "string") {
        this.#parse(init);
      } else if (init instanceof URLSearchParams) {
        this.#list = init.#list.map(([k, v]) => [k, v]);
      } else if (init != null && typeof init[Symbol.iterator] === "function") {
        for (const pair of init) {
          const p = [...pair];
          if (p.length !== 2) throw new TypeError("Each pair must have two items");
          this.#list.push([String(p[0]), String(p[1])]);
        }
      } else if (init != null) {
        for (const k of Object.keys(init)) this.#list.push([k, String(init[k])]);
      }
    }
    #parse(s) {
      this.#list = [];
      if (s.startsWith("?")) s = s.slice(1);
      for (const part of s.split("&")) {
        if (part === "") continue;
        const i = part.indexOf("=");
        this.#list.push(i < 0 ? [formDecode(part), ""] : [formDecode(part.slice(0, i)), formDecode(part.slice(i + 1))]);
      }
    }
    #update() {
      if (this.#url) this.#url._setSearch(this.toString());
    }
    static _attach(params, url, search) {
      params.#url = url;
      params.#parse(search);
    }
    static _reparse(params, search) {
      params.#parse(search);
    }
    get size() {
      return this.#list.length;
    }
    append(name, value) {
      this.#list.push([String(name), String(value)]);
      this.#update();
    }
    delete(name, value) {
      name = String(name);
      this.#list = this.#list.filter(([k, v]) => k !== name || (value !== undefined && v !== String(value)));
      this.#update();
    }
    get(name) {
      name = String(name);
      const hit = this.#list.find(([k]) => k === name);
      return hit ? hit[1] : null;
    }
    getAll(name) {
      name = String(name);
      return this.#list.filter(([k]) => k === name).map(([, v]) => v);
    }
    has(name, value) {
      name = String(name);
      return this.#list.some(([k, v]) => k === name && (value === undefined || v === String(value)));
    }
    set(name, value) {
      name = String(name);
      value = String(value);
      const i = this.#list.findIndex(([k]) => k === name);
      if (i < 0) {
        this.#list.push([name, value]);
      } else {
        this.#list[i][1] = value;
        this.#list = this.#list.filter(([k], j) => j <= i || k !== name);
      }
      this.#update();
    }
    sort() {
      this.#list.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
      this.#update();
    }
    forEach(callback, thisArg) {
      for (const [k, v] of this.#list) callback.call(thisArg, v, k, this);
    }
    *entries() {
      for (const [k, v] of this.#list) yield [k, v];
    }
    *keys() {
      for (const [k] of this.#list) yield k;
    }
    *values() {
      for (const [, v] of this.#list) yield v;
    }
    [Symbol.iterator]() {
      return this.entries();
    }
    toString() {
      return this.#list.map(([k, v]) => formEncode(k) + "=" + formEncode(v)).join("&");
    }
  }

  // Parts returned by the native parser, in this order: href, origin,
  // protocol, username, password, host, hostname, port, pathname, search, hash.
  class URL {
    #parts;
    #params;
    constructor(url, base) {
      const parts = urlNative(String(url), base === undefined ? undefined : String(base));
      if (parts == null) throw new TypeError(`Invalid URL: ${url}`);
      this.#parts = parts;
      this.#params = new URLSearchParams();
      URLSearchParams._attach(this.#params, this, this.search);
    }
    static canParse(url, base) {
      return urlNative(String(url), base === undefined ? undefined : String(base)) != null;
    }
    static parse(url, base) {
      try {
        return new URL(url, base);
      } catch {
        return null;
      }
    }
    #set(field, value) {
      const parts = urlNative(this.#parts[0], undefined, field, String(value));
      if (parts == null) {
        if (field === "href") throw new TypeError(`Invalid URL: ${value}`);
        return;
      }
      this.#parts = parts;
      if (field === "search" || field === "href") URLSearchParams._reparse(this.#params, this.search);
    }
    _setSearch(search) {
      const parts = urlNative(this.#parts[0], undefined, "search", search);
      if (parts != null) this.#parts = parts;
    }
    get href() { return this.#parts[0]; }
    set href(v) { this.#set("href", v); }
    get origin() { return this.#parts[1]; }
    get protocol() { return this.#parts[2]; }
    set protocol(v) { this.#set("protocol", v); }
    get username() { return this.#parts[3]; }
    set username(v) { this.#set("username", v); }
    get password() { return this.#parts[4]; }
    set password(v) { this.#set("password", v); }
    get host() { return this.#parts[5]; }
    set host(v) { this.#set("host", v); }
    get hostname() { return this.#parts[6]; }
    set hostname(v) { this.#set("hostname", v); }
    get port() { return this.#parts[7]; }
    set port(v) { this.#set("port", v); }
    get pathname() { return this.#parts[8]; }
    set pathname(v) { this.#set("pathname", v); }
    get search() { return this.#parts[9]; }
    set search(v) { this.#set("search", v); }
    get hash() { return this.#parts[10]; }
    set hash(v) { this.#set("hash", v); }
    get searchParams() {
      return this.#params;
    }
    toString() {
      return this.#parts[0];
    }
    toJSON() {
      return this.#parts[0];
    }
  }

  // --- Events -------------------------------------------------------------

  class Event {
    #type;
    #stopped = false;
    constructor(type, init = {}) {
      this.#type = String(type);
      this.bubbles = !!init.bubbles;
      this.cancelable = !!init.cancelable;
      this.defaultPrevented = false;
      this.target = null;
      this.currentTarget = null;
      this.timeStamp = g.performance ? g.performance.now() : Date.now();
    }
    get type() {
      return this.#type;
    }
    preventDefault() {
      if (this.cancelable) this.defaultPrevented = true;
    }
    stopImmediatePropagation() {
      this.#stopped = true;
    }
    stopPropagation() {}
    static _stopped(e) {
      return e.#stopped;
    }
  }

  class EventTarget {
    #listeners = new Map();
    addEventListener(type, listener, options) {
      if (listener == null) return;
      const once = typeof options === "object" && options !== null && !!options.once;
      const signal = typeof options === "object" && options !== null ? options.signal : undefined;
      if (signal && signal.aborted) return;
      let list = this.#listeners.get(type);
      if (!list) this.#listeners.set(type, (list = []));
      if (list.some((l) => l.listener === listener)) return;
      list.push({ listener, once });
      if (signal) signal.addEventListener("abort", () => this.removeEventListener(type, listener));
    }
    removeEventListener(type, listener) {
      const list = this.#listeners.get(type);
      if (!list) return;
      const i = list.findIndex((l) => l.listener === listener);
      if (i >= 0) list.splice(i, 1);
    }
    dispatchEvent(event) {
      event.target = this;
      event.currentTarget = this;
      const handler = this["on" + event.type];
      if (typeof handler === "function") handler.call(this, event);
      for (const entry of [...(this.#listeners.get(event.type) ?? [])]) {
        if (entry.once) this.removeEventListener(event.type, entry.listener);
        const l = entry.listener;
        if (typeof l === "function") l.call(this, event);
        else if (l && typeof l.handleEvent === "function") l.handleEvent(event);
        if (Event._stopped(event)) break;
      }
      event.currentTarget = null;
      return !event.defaultPrevented;
    }
  }

  // --- Abort --------------------------------------------------------------

  const abortError = (message = "This operation was aborted") => new DOMException(message, "AbortError");

  class AbortSignal extends EventTarget {
    #aborted = false;
    #reason = undefined;
    constructor(key) {
      super();
      if (key !== SIGNAL_KEY) throw new TypeError("Illegal constructor");
      this.onabort = null;
    }
    get aborted() {
      return this.#aborted;
    }
    get reason() {
      return this.#reason;
    }
    throwIfAborted() {
      if (this.#aborted) throw this.#reason;
    }
    static _abort(signal, reason) {
      if (signal.#aborted) return;
      signal.#aborted = true;
      signal.#reason = reason === undefined ? abortError() : reason;
      signal.dispatchEvent(new Event("abort"));
    }
    static abort(reason) {
      const signal = new AbortSignal(SIGNAL_KEY);
      AbortSignal._abort(signal, reason);
      return signal;
    }
    static timeout(ms) {
      const signal = new AbortSignal(SIGNAL_KEY);
      setTimeout(() => AbortSignal._abort(signal, new DOMException("The operation timed out", "TimeoutError")), ms);
      return signal;
    }
    static any(signals) {
      const signal = new AbortSignal(SIGNAL_KEY);
      for (const s of signals) {
        if (s.aborted) {
          AbortSignal._abort(signal, s.reason);
          return signal;
        }
      }
      for (const s of signals) s.addEventListener("abort", () => AbortSignal._abort(signal, s.reason), { once: true });
      return signal;
    }
  }
  const SIGNAL_KEY = Symbol("AbortSignal");

  class AbortController {
    #signal = new AbortSignal(SIGNAL_KEY);
    get signal() {
      return this.#signal;
    }
    abort(reason) {
      AbortSignal._abort(this.#signal, reason);
    }
  }

  // --- Headers ------------------------------------------------------------

  const TOKEN = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
  const normalizeName = (name) => {
    name = String(name);
    if (!TOKEN.test(name)) throw new TypeError(`Invalid header name: "${name}"`);
    return name.toLowerCase();
  };
  const normalizeValue = (value) => String(value).replace(/^[\t\n\r ]+|[\t\n\r ]+$/g, "");

  class Headers {
    // name (lower case) -> values, in insertion order.
    #map = new Map();
    constructor(init) {
      if (init == null) return;
      if (init instanceof Headers) {
        for (const [k, v] of init.#map) this.#map.set(k, [...v]);
      } else if (typeof init[Symbol.iterator] === "function") {
        for (const pair of init) {
          const p = [...pair];
          if (p.length !== 2) throw new TypeError("Each header pair must have two items");
          this.append(p[0], p[1]);
        }
      } else {
        for (const k of Object.keys(init)) this.append(k, init[k]);
      }
    }
    append(name, value) {
      const key = normalizeName(name);
      const list = this.#map.get(key);
      if (list) list.push(normalizeValue(value));
      else this.#map.set(key, [normalizeValue(value)]);
    }
    delete(name) {
      this.#map.delete(normalizeName(name));
    }
    get(name) {
      const list = this.#map.get(normalizeName(name));
      return list ? list.join(", ") : null;
    }
    getSetCookie() {
      return [...(this.#map.get("set-cookie") ?? [])];
    }
    has(name) {
      return this.#map.has(normalizeName(name));
    }
    set(name, value) {
      this.#map.set(normalizeName(name), [normalizeValue(value)]);
    }
    forEach(callback, thisArg) {
      for (const [k, v] of this.entries()) callback.call(thisArg, v, k, this);
    }
    *entries() {
      for (const k of [...this.#map.keys()].sort()) {
        if (k === "set-cookie") for (const v of this.#map.get(k)) yield [k, v];
        else yield [k, this.#map.get(k).join(", ")];
      }
    }
    *keys() {
      for (const [k] of this.entries()) yield k;
    }
    *values() {
      for (const [, v] of this.entries()) yield v;
    }
    [Symbol.iterator]() {
      return this.entries();
    }
    // Flat [name, value, ...] for the native side.
    static _flat(headers) {
      const out = [];
      for (const [k, list] of headers.#map) for (const v of list) out.push(k, v);
      return out;
    }
  }

  // --- Bodies -------------------------------------------------------------

  // Normalizes a body init to [bytes (ArrayBuffer) | null, content type | null].
  const extractBody = (body) => {
    if (body == null) return [null, null];
    if (typeof body === "string") return [utf8Encode(body), "text/plain;charset=UTF-8"];
    if (body instanceof URLSearchParams) {
      return [utf8Encode(body.toString()), "application/x-www-form-urlencoded;charset=UTF-8"];
    }
    if (body instanceof ArrayBuffer) return [body.slice(0), null];
    if (ArrayBuffer.isView(body)) {
      return [body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength), null];
    }
    return [utf8Encode(String(body)), "text/plain;charset=UTF-8"];
  };

  // Shared by Request and Response: one-shot reads of a buffered body.
  class Body {
    #bytes;
    #used = false;
    _initBody(bytes) {
      this.#bytes = bytes;
    }
    get body() {
      return null;
    }
    get bodyUsed() {
      return this.#used;
    }
    #consume() {
      if (this.#used) return Promise.reject(new TypeError("Body has already been consumed"));
      this.#used = true;
      return Promise.resolve(this.#bytes ?? new ArrayBuffer(0));
    }
    _peek() {
      return this.#bytes;
    }
    arrayBuffer() {
      return this.#consume();
    }
    bytes() {
      return this.#consume().then((b) => new Uint8Array(b));
    }
    text() {
      return this.#consume().then((b) => utf8Decode(b, 0, b.byteLength, false, false));
    }
    json() {
      return this.text().then(JSON.parse);
    }
  }

  // --- Request ------------------------------------------------------------

  const METHODS = ["DELETE", "GET", "HEAD", "OPTIONS", "POST", "PUT", "PATCH"];

  class Request extends Body {
    #method;
    #url;
    #headers;
    #signal;
    #redirect;
    constructor(input, init = {}) {
      super();
      let base = null;
      if (input instanceof Request) {
        base = input;
        this.#url = input.url;
      } else {
        this.#url = new URL(String(input)).href;
      }
      let method = init.method ?? base?.method ?? "GET";
      const upper = String(method).toUpperCase();
      this.#method = METHODS.includes(upper) ? upper : String(method);
      this.#headers = new Headers(init.headers ?? base?.headers);
      this.#signal = init.signal ?? base?.signal ?? new AbortController().signal;
      this.#redirect = init.redirect ?? base?.redirect ?? "follow";
      let bytes = null;
      if (init.body !== undefined) {
        const [b, type] = extractBody(init.body);
        bytes = b;
        if (type && !this.#headers.has("content-type")) this.#headers.set("content-type", type);
      } else if (base) {
        bytes = base._peek() ?? null;
      }
      if (bytes !== null && (this.#method === "GET" || this.#method === "HEAD")) {
        throw new TypeError("Request with GET/HEAD method cannot have body");
      }
      this._initBody(bytes);
    }
    get method() {
      return this.#method;
    }
    get url() {
      return this.#url;
    }
    get headers() {
      return this.#headers;
    }
    get signal() {
      return this.#signal;
    }
    get redirect() {
      return this.#redirect;
    }
    get credentials() {
      return "include";
    }
    get mode() {
      return "cors";
    }
    clone() {
      if (this.bodyUsed) throw new TypeError("Body has already been consumed");
      return new Request(this);
    }
  }

  // --- Response -----------------------------------------------------------

  class Response extends Body {
    #status;
    #statusText;
    #headers;
    #type = "default";
    #url = "";
    #redirected = false;
    constructor(body = null, init = {}) {
      super();
      const status = init.status ?? 200;
      if (status < 200 || status > 599) throw new RangeError(`Invalid status: ${status}`);
      this.#status = status;
      this.#statusText = init.statusText ?? "";
      this.#headers = new Headers(init.headers);
      const [bytes, type] = extractBody(body);
      if (type && !this.#headers.has("content-type")) this.#headers.set("content-type", type);
      this._initBody(bytes);
    }
    static _fromNetwork(status, statusText, url, redirected, flat, bytes) {
      const r = new Response(null, { status: status < 200 ? 200 : status });
      r.#status = status;
      r.#statusText = statusText;
      r.#url = url;
      r.#redirected = redirected;
      r.#type = "basic";
      for (let i = 0; i < flat.length; i += 2) r.#headers.append(flat[i], flat[i + 1]);
      r._initBody(bytes);
      return r;
    }
    static json(data, init = {}) {
      const headers = new Headers(init.headers);
      if (!headers.has("content-type")) headers.set("content-type", "application/json");
      return new Response(JSON.stringify(data), { ...init, headers });
    }
    static error() {
      const r = new Response(null, { status: 200 });
      r.#status = 0;
      r.#type = "error";
      return r;
    }
    static redirect(url, status = 302) {
      return new Response(null, { status, headers: { location: new URL(url).href } });
    }
    get type() {
      return this.#type;
    }
    get url() {
      return this.#url;
    }
    get redirected() {
      return this.#redirected;
    }
    get status() {
      return this.#status;
    }
    get ok() {
      return this.#status >= 200 && this.#status < 300;
    }
    get statusText() {
      return this.#statusText;
    }
    get headers() {
      return this.#headers;
    }
    clone() {
      if (this.bodyUsed) throw new TypeError("Body has already been consumed");
      const bytes = this._peek();
      return Response._fromNetwork(
        this.#status,
        this.#statusText,
        this.#url,
        this.#redirected,
        Headers._flat(this.#headers),
        bytes ? bytes.slice(0) : null,
      );
    }
  }

  // --- fetch --------------------------------------------------------------

  let nextRequest = 1;
  const pending = new Map();

  function fetch(input, init) {
    return new Promise((resolve, reject) => {
      const request = new Request(input, init);
      const signal = request.signal;
      if (signal.aborted) return reject(signal.reason);
      const id = nextRequest++;
      const onAbort = () => {
        if (!pending.delete(id)) return;
        abortNative(id);
        reject(signal.reason);
      };
      pending.set(id, { resolve, reject, signal, onAbort });
      signal.addEventListener("abort", onAbort);
      fetchNative(
        id,
        request.method,
        request.url,
        Headers._flat(request.headers),
        request._peek() ?? null,
        request.redirect !== "manual",
      );
    });
  }

  const settle = (id) => {
    const entry = pending.get(id);
    if (!entry) return null;
    pending.delete(id);
    entry.signal.removeEventListener("abort", entry.onAbort);
    return entry;
  };

  hidden(g, {
    __nsh_fetch_done(id, status, statusText, url, redirected, flat, bytes) {
      const entry = settle(id);
      if (entry) entry.resolve(Response._fromNetwork(status, statusText, url, redirected, flat, bytes));
    },
    __nsh_fetch_fail(id, message) {
      const entry = settle(id);
      if (entry) entry.reject(new TypeError(`Network request failed: ${message}`));
    },
  });

  define("TextEncoder", TextEncoder);
  define("TextDecoder", TextDecoder);
  define("URL", URL);
  define("URLSearchParams", URLSearchParams);
  define("Event", Event);
  define("EventTarget", EventTarget);
  define("AbortSignal", AbortSignal);
  define("AbortController", AbortController);
  define("Headers", Headers);
  define("Request", Request);
  define("Response", Response);
  define("fetch", fetch);
})();
