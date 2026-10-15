// nsh hot module replacement runtime (development bundles only).
//
// A development bundle defines every module as a factory (rolldown CommonJS
// output with one chunk per module) and starts the entry. An update script
// redefines the modules that changed; `update` runs them again, together
// with the importers that cannot take the change on their own, in place:
// the JS context, the native views and every other module keep their state.
//
// Components (top-level functions returning JSX, wrapped by the bundler)
// are registered by module and name and rendered through a stable proxy.
// Running a module again swaps the implementation behind its proxies; the
// mounted instances whose code changed render again. Contexts keep their
// identity across updates, so providers and consumers stay connected.
//
// Written in plain JavaScript: it runs before any module, including the
// TypeScript runtime.

globalThis.__nsh_hot = (() => {
  /** id -> factory(module, exports, require) */
  const factories = new Map();
  /** Ids of modules that never run again (solid-js, the nsh runtime). */
  const fixed = new Set();
  /** id -> { id, exports, components, exportsComponents } */
  const modules = new Map();
  /** id -> Set of the ids of modules that required it */
  const importers = new Map();
  /** "module:name" -> component record */
  const components = new Map();
  /** "module:name" -> context */
  const contexts = new Map();
  /** module id -> render disposers */
  const renders = new Map();
  const proxies = new WeakSet();
  let solidId = null;
  let entry = null;
  /** The module running now. */
  let current = null;
  /** Running an update: components whose code did not change still render again. */
  let forced = false;
  /** Components to render again once the update ran. */
  let stale = [];

  const solid = () => load(solidId);

  function resolve(from, spec) {
    if (!spec.startsWith(".")) return spec;
    const parts = from.split("/");
    parts.pop();
    for (const part of spec.split("/")) {
      if (part === "..") parts.pop();
      else if (part !== ".") parts.push(part);
    }
    return parts.join("/");
  }

  function run(module) {
    const factory = factories.get(module.id);
    if (!factory) throw new Error(`nsh hot: no module ${module.id}`);
    const previous = current;
    current = module;
    module.components = 0;
    // Renders made by an earlier run of this module are replaced.
    for (const dispose of renders.get(module.id) ?? []) dispose();
    renders.delete(module.id);
    // Importers keep the module's first exports object: each run fills a
    // fresh one, which the kept object then forwards to (modules define
    // their exports as non-configurable getters).
    const exports = fixed.has(module.id) ? module.exports : {};
    try {
      factory(module, exports, (spec) => {
        const id = resolve(module.id, spec);
        let set = importers.get(id);
        if (!set) importers.set(id, (set = new Set()));
        set.add(module.id);
        return load(id);
      });
    } finally {
      current = previous;
    }
    if (exports !== module.exports) {
      const kept = module.exports;
      for (const key of Object.getOwnPropertyNames(kept)) {
        if (!Object.prototype.hasOwnProperty.call(exports, key)) delete kept[key];
      }
      for (const key of Object.getOwnPropertyNames(exports)) {
        Object.defineProperty(kept, key, {
          configurable: true,
          enumerable: Object.getOwnPropertyDescriptor(exports, key).enumerable,
          get: () => exports[key],
        });
      }
    }
    // Takes its own updates when everything it exports is a component.
    const values = Object.values(module.exports);
    module.accepts = module.components > 0 && values.every((v) => proxies.has(v));
  }

  function load(id) {
    let module = modules.get(id);
    if (module) return module.exports;
    module = { id, exports: {}, components: 0, accepts: false };
    modules.set(id, module);
    run(module);
    return module.exports;
  }

  return {
    /** Registers a module factory; `isFixed` modules never run again. */
    define(id, factory, isFixed) {
      factories.set(id, factory);
      if (isFixed) fixed.add(id);
    },

    /** Starts the app: `solid` is the id of solid-js, `main` the entry. */
    start(solidModule, main) {
      solidId = solidModule;
      entry = main;
      load(main);
    },

    /**
     * Applies an update: `defs` maps module ids to new factories. Returns
     * false when the update cannot be applied in place (the caller reloads).
     */
    update(defs) {
      const changed = [];
      for (const id in defs) {
        factories.set(id, defs[id]);
        if (modules.has(id)) changed.push(id);
      }
      // Modules to run again: the changed ones, and the importers of those
      // that do not accept their own updates (up to the entry).
      const rerun = new Set();
      const queue = [...changed];
      while (queue.length > 0) {
        const id = queue.pop();
        if (rerun.has(id)) continue;
        rerun.add(id);
        const module = modules.get(id);
        if (module.accepts || id === entry) continue;
        const from = importers.get(id);
        if (!from || from.size === 0) return false;
        for (const importer of from) queue.push(importer);
      }
      // Dependencies before their importers.
      const order = [];
      const seen = new Set();
      const visit = (id) => {
        if (seen.has(id)) return;
        seen.add(id);
        for (const [dep, from] of importers) {
          if (rerun.has(dep) && from.has(id)) visit(dep);
        }
        order.push(id);
      };
      for (const id of rerun) visit(id);
      const { batch } = solid();
      try {
        for (const id of order) {
          forced = !(id in defs);
          run(modules.get(id));
        }
      } finally {
        forced = false;
      }
      const renderAgain = stale;
      stale = [];
      batch(() => {
        for (const c of renderAgain) c.setVersion((v) => v + 1);
      });
      return true;
    },

    /**
     * A component of module `key`: returns its stable proxy. `sig` changes
     * when its code (or the module's other code) changed.
     */
    component(key, name, fn, sig) {
      const k = `${key}:${name}`;
      if (current) current.components++;
      let c = components.get(k);
      if (c) {
        if (c.sig !== sig || forced) stale.push(c);
        c.fn = fn;
        c.sig = sig;
        return c.proxy;
      }
      const { createSignal, createMemo, untrack } = solid();
      const [version, setVersion] = createSignal(0);
      c = { fn, sig, setVersion };
      c.proxy = (props) =>
        createMemo(() => {
          version();
          const f = c.fn;
          try {
            return untrack(() => f(props));
          } catch (error) {
            // A computation that throws stays stale in Solid and never runs
            // again: report the error on its own and stay ready for the fix.
            queueMicrotask(() => {
              throw error;
            });
            return undefined;
          }
        });
      Object.defineProperty(c.proxy, "name", { value: name });
      proxies.add(c.proxy);
      components.set(k, c);
      return c.proxy;
    },

    /** A context of module `key`: the first one created keeps its identity. */
    context(key, name, context) {
      const k = `${key}:${name}`;
      const kept = contexts.get(k);
      if (!kept) {
        contexts.set(k, context);
        return context;
      }
      kept.defaultValue = context.defaultValue;
      return kept;
    },

    /** The runtime's `render`: disposed when its module runs again. */
    rendered(dispose) {
      if (!current) return;
      let list = renders.get(current.id);
      if (!list) renders.set(current.id, (list = []));
      list.push(dispose);
    },
  };
})();
