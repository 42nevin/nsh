---
title: nsh/image
description: Image.getSize 和 Image.prefetch：获取图片信息和预加载图片。
order: 8
---

# nsh/image

`nsh/image` 向原生图片加载器查询图片信息：尺寸，以及在显示之前预先加载。图片本身用 [`<image>`](../elements/image.md) 元素显示。

```ts
import { Image, type ImageSize } from "nsh/image";
```

> [!NOTE]
> 从 `nsh/image` 导入 `Image` 不会改变 `<image>` 元素：`Image` 是一个函数集合对象，不是组件，所以 `<image src=…>` 仍然是宿主元素。

教程见[图片与资源](../docs/images-and-assets.md)。

## Image.getSize

```ts
getSize(source: string | { uri: string }): Promise<ImageSize>;

interface ImageSize {
  width: number;   // 像素
  height: number;
}
```

加载图片，并 resolve 为它的像素尺寸（源图片的尺寸，而不是降采样后的副本）。加载的数据会保留在内存中，接下来显示它的 `<image>` 不会再次下载。图片无法加载时 Promise 会 reject。

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

在显示之前预先加载图片。数据进入内存后 resolve 为 `true`（无法加载时为 `false`）。适合用于相册的下一页，或点击后才会显示的图片。

```ts
const next = photos.slice(index + 1, index + 4);
next.forEach((p) => Image.prefetch(p.url));
```

## 相关内容

- [`<image>`](../elements/image.md)：`src`、`resizeMode`、`defaultSource`、`onLoad`、`onProgress`、`onError` 等属性。
- 导入图片文件（`import logo from "./logo.png"`）会得到一个 `ImageAsset`（`{ uri, width, height }`，尺寸单位为点），`<image src>` 和画布的 `drawImage` 都接受它。

> [!NOTE]
> 如果应用在 `nsh.json` 中声明了 `capabilities`，对远程 URL 调用 `getSize` 和 `prefetch` 需要像 `fetch` 一样在 `network` 中允许。见[沙箱与权限](../docs/sandbox-and-capabilities.md)。
