# Developer documentation

For people working on native-shell itself (`AGENTS.md` at the root is the short version agents read first).

| Page | What |
|---|---|
| [architecture.md](architecture.md) | Threads, the data flow from JS to native views, crates |
| [js-modules.md](js-modules.md) | The JS layers (`nsh/*`, `nsh/ui/*`), the module table, platform modules and props, where new things go |
| [elements.md](elements.md) | Host elements and runtime components, touch, accessibility, the environment, why the main thread never waits for JS |
| [text.md](text.md) | Text blocks, spans, inline attachments |
| [styles.md](styles.md) | Tailwind, effects, view styles, Android differences |
| [scrolling-and-lists.md](scrolling-and-lists.md) | Scroll views, zooming, `List` |
| [animation.md](animation.md) | Native values, composites, layout animation |
| [canvas.md](canvas.md) | `Canvas` |
| [images.md](images.md) | Loading, decoding, caching, animated images |
| [sandbox.md](sandbox.md) | VM limits, hardening, capabilities |
| [signed-packages.md](signed-packages.md) | Keys, packages, verification, revocation |
| [embedding.md](embedding.md) | `nsh package`, the iOS / Android SDKs, native components |
| [tooling.md](tooling.md) | `.nsh/types`, the dev server, hot reload, symbolication |
| [performance.md](performance.md) | Baselines |
| [pitfalls.md](pitfalls.md) | Verified constraints and pitfalls, by area |

How a module works inside is documented in the module itself (`//!` docs, Javadoc); these pages describe behavior
and decisions and point there.
