// `nsh/device`: the surface and the device: sizes, appearance,
// text size, the platform, pixel density and right-to-left layout.

import { untrack } from "solid-js";
import { environment, initialEnvironment, isRTL, setAllowRTL, setForceRTL } from "./_core";

export { colorScheme, fontScale, windowSize } from "./_core";

export interface PlatformSelectSpec<T> {
  ios?: T;
  android?: T;
  /** Both platforms (nsh only runs natively). */
  native?: T;
  default?: T;
}

export interface PlatformStatic {
  readonly OS: "ios" | "android";
  /** iOS: the version string (`"17.5"`); Android: the API level. */
  readonly Version: string | number;
  readonly isPad: boolean;
  readonly isTV: boolean;
  /** Mac Catalyst in the Mac idiom. */
  readonly isMacCatalyst: boolean;
  readonly constants: {
    readonly osVersion: string | number;
    readonly interfaceIdiom: "phone" | "pad" | "mac" | "tv";
  };
  /** The value for this platform: `ios` / `android`, else `native`, else `default`. */
  select<T>(spec: PlatformSelectSpec<T>): T | undefined;
}

/** The platform the app runs on (React Native's `Platform`). */
export const Platform: PlatformStatic = {
  OS: initialEnvironment.os,
  Version: initialEnvironment.osVersion,
  isPad: initialEnvironment.idiom === "pad",
  isTV: initialEnvironment.idiom === "tv",
  isMacCatalyst: initialEnvironment.idiom === "mac",
  constants: { osVersion: initialEnvironment.osVersion, interfaceIdiom: initialEnvironment.idiom },
  select<T>(spec: PlatformSelectSpec<T>): T | undefined {
    const own = spec[initialEnvironment.os];
    if (own !== undefined) return own;
    return spec.native !== undefined ? spec.native : spec.default;
  },
};

/** Device pixels per point (React Native's `PixelRatio`). Not reactive:
 * read in components that also read `windowSize()` to follow a window
 * moving to another screen. */
export interface PixelRatioStatic {
  get(): number;
  getFontScale(): number;
  getPixelSizeForLayoutSize(size: number): number;
  roundToNearestPixel(size: number): number;
}

export const PixelRatio: PixelRatioStatic = {
  get(): number {
    return untrack(environment).pixelRatio;
  },
  /** The user's text size preference (also `fontScale()`). */
  getFontScale(): number {
    return untrack(environment).fontScale;
  },
  /** `size` points in whole device pixels. */
  getPixelSizeForLayoutSize(size: number): number {
    return Math.round(size * PixelRatio.get());
  },
  /** `size` rounded to the nearest device pixel, in points. */
  roundToNearestPixel(size: number): number {
    const ratio = PixelRatio.get();
    return Math.round(size * ratio) / ratio;
  },
};

/** Right-to-left layout (React Native's `I18nManager`). Right to left,
 * rows run from the right and `left` / `right` style properties mean start
 * and end. Changes apply at once (no restart). */
export const I18nManager = {
  /** Whether the app lays out right to left (reactive). */
  get isRTL(): boolean {
    return isRTL();
  },
  /** `left` and `right` swap right to left (always, as React Native's default). */
  doLeftAndRightSwapInRTL: true as const,
  /** Follow right-to-left languages (default true). */
  allowRTL(allow: boolean): void {
    setAllowRTL(allow);
  },
  /** Lay out right to left whatever the language. */
  forceRTL(force: boolean): void {
    setForceRTL(force);
  },
  getConstants(): { isRTL: boolean; doLeftAndRightSwapInRTL: boolean } {
    return { isRTL: isRTL(), doLeftAndRightSwapInRTL: true };
  },
};
