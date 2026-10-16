---
title: nsh/image
description: Image.getSize and Image.prefetch, image information and loading ahead.
order: 8
---

# nsh/image

`nsh/image` asks the native image loader about images: their size, and loading them before they are shown. Images themselves are shown with the [`<image>`](../elements/image.md) element.

```ts
import { Image, type ImageSize } from "nsh/image";
```

> [!NOTE]
> Importing `Image` from `nsh/image` does not change the `<image>` element: `Image` is an object of functions, not a component, so `<image src=…>` stays the host element.

See [Images and assets](../docs/images-and-assets.md) for the guide.

## Image.getSize

```ts
getSize(source: string | { uri: string }): Promise<ImageSize>;

interface ImageSize {
  width: number;   // pixels
  height: number;
}
```

Loads the image and resolves with its size in pixels (of the source image, not a downsampled copy). The loaded bytes stay in memory, so the `<image>` that shows it next does not download it again. The promise rejects if the image cannot be loaded.

```tsx
import { signal } from "nsh";
import { Image } from "nsh/image";

const url = "https://picsum.photos/seed/nsh/1200/800";
const ratio = signal(1);

Image.getSize(url).then(({ width, height }) => ratio.set(width / height));

<image src={url} style={{ width: "100%", aspectRatio: ratio() }} />;
```

## Image.prefetch

```ts
prefetch(source: string | { uri: string }): Promise<boolean>;
```

Loads an image ahead of showing it. Resolves with `true` once its bytes are in memory (`false` when it could not be loaded). Use it for the next page of a gallery or the image behind a tap.

```ts
const next = photos.slice(index + 1, index + 4);
next.forEach((p) => Image.prefetch(p.url));
```

## Related

- [`<image>`](../elements/image.md): `src`, `resizeMode`, `defaultSource`, `onLoad`, `onProgress`, `onError` and the other props.
- Importing an image file (`import logo from "./logo.png"`) gives an `ImageAsset` (`{ uri, width, height }`, sized in points) that `<image src>` and canvas `drawImage` accept.

> [!NOTE]
> When your app declares `capabilities` in `nsh.json`, `getSize` and `prefetch` of remote URLs must be allowed under `network`, like `fetch`. See [Sandbox and capabilities](../docs/sandbox-and-capabilities.md).
