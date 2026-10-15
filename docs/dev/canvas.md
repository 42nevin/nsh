# Canvas

- **Canvas** (`Canvas` component, `<canvas>` element): an HTML Canvas 2D subset recorded in JS and replayed natively
  (`nsh_core::canvas`: `Drawing { ops, slots }`). `draw={(ctx, { width, height }) => ...}` runs again when the signals it reads
  change (sizes come from `onLayout`). `CanvasRenderingContext2D`: state (`save` / `restore`, `globalAlpha`,
  `globalCompositeOperation`, line styles and dashes, shadows, `font` (CSS shorthand), `textAlign` / `textBaseline`),
  transforms, paths (`arc`, `arcTo`, `ellipse`, curves, `rect`, `roundRect`), `fill` / `stroke` / `clip` (also of a `Path2D`,
  which takes SVG path data; arcs become cubics), `fillRect` / `strokeRect` / `clearRect`, `fillText` / `strokeText` /
  `measureText` (synchronous, through the shared text engine: `__nsh_measure_text`; only `width`), linear / radial / conic
  gradients, `drawImage` (all three forms, images by URI or asset). Any number argument and colors may be native values
  (`createNativeValue`, `interpolate`, ...): those become slots bound like style values (`AnimProp::CanvasArg`) and the
  platform redraws on its own while they animate, without JS. Drawings travel in their own op (`OP_DRAW`), recorded into a
  separate array: creating native values while drawing pushes ops of its own. iOS replays into the `drawRect:` context,
  keeping its own transform and path in view space; Android `CanvasBox.java` interprets an `int[]` (`encode.rs`). Not
  supported: `getImageData` / `putImageData`, filters, `isPointInPath`, SVG images.

The drawing format is documented in `crates/nsh-core/src/canvas.rs`.
