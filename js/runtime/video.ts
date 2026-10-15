// `nsh/video`: video playback.

import { hostComponent, type YNode } from "./_core";
import type { ViewProps } from "./types";

export interface VideoProps extends Omit<ViewProps, "children" | "onLoad" | "onError"> {
  source: string | { uri: string };
  paused?: boolean;
  muted?: boolean;
  /** 0..1. */
  volume?: number;
  rate?: number;
  repeat?: boolean;
  resizeMode?: "contain" | "cover" | "stretch";
  /** The platform's playback controls. */
  controls?: boolean;
  /** Milliseconds between `onProgress` events (250 by default). */
  progressUpdateInterval?: number;
  onLoadStart?: () => void;
  onLoad?: (e: { duration: number; width: number; height: number }) => void;
  onProgress?: (e: { currentTime: number; duration: number }) => void;
  onEnd?: () => void;
  onBuffer?: (e: { isBuffering: boolean }) => void;
  onError?: (e: { message: string }) => void;
}

/** A video player (AVPlayer / MediaPlayer); its `ref` has `seek(seconds)`. */
export const Video: (props: VideoProps) => YNode = hostComponent<VideoProps>("video");
