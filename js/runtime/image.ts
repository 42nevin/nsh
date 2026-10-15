// `nsh/image`: image information and loading ahead (`<image>` itself is an
// element).

import { command, root, setProp } from "./_core";

// --- Image information ----------------------------------------------------------

let nextImageInfo = 1;
const imageInfos = new Map<number, (e: { width?: number; height?: number; error?: string }) => void>();

function onImageInfo(e: { id: number; width?: number; height?: number; error?: string }) {
  const done = imageInfos.get(e.id);
  imageInfos.delete(e.id);
  if (imageInfos.size === 0) setProp(root, "onImageInfo", undefined);
  done?.(e);
}

function imageInfo(uri: string): Promise<{ width: number; height: number }> {
  return new Promise((resolve, reject) => {
    const id = nextImageInfo++;
    if (imageInfos.size === 0) setProp(root, "onImageInfo", onImageInfo);
    imageInfos.set(id, (e) => {
      if (e.error !== undefined || e.width === undefined || e.height === undefined) reject(new Error(e.error ?? "no size"));
      else resolve({ width: e.width, height: e.height });
    });
    command(1, "imageInfo", { id, uri });
  });
}

export interface ImageModule {
  /** The image's size in pixels; its bytes stay loaded for the views that show it next. */
  getSize(source: string | { uri: string }): Promise<{ width: number; height: number }>;
  /** Loads the image ahead of showing it: true once its bytes are in memory. */
  prefetch(source: string | { uri: string }): Promise<boolean>;
}

/** Image information and loading ahead. */
export const Image: ImageModule = {
  getSize: (source) => imageInfo(typeof source === "string" ? source : source.uri),
  prefetch: (source) =>
    imageInfo(typeof source === "string" ? source : source.uri).then(
      () => true,
      () => false,
    ),
};
