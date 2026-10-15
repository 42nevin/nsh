// `nsh/webview`: web pages.

import { hostComponent, type YNode } from "./_core";
import type { ViewProps } from "./types";

export interface WebViewNavigation {
  url: string;
  title: string;
  loading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
}

export interface WebViewProps extends Omit<ViewProps, "children" | "onLoad" | "onError"> {
  source: string | { uri: string } | { html: string; baseUrl?: string };
  /** Runs after each page loads. */
  injectedJavaScript?: string;
  /** Runs before each page's own scripts. */
  injectedJavaScriptBeforeContentLoaded?: string;
  userAgent?: string;
  scrollEnabled?: boolean;
  javaScriptEnabled?: boolean;
  onLoadStart?: (e: WebViewNavigation) => void;
  onLoad?: (e: WebViewNavigation) => void;
  onError?: (e: { message: string }) => void;
  /** The page called `window.ReactNativeWebView.postMessage(data)`. */
  onMessage?: (e: { data: string }) => void;
  onNavigationStateChange?: (e: WebViewNavigation) => void;
  ios?: {
    /** Swipe back and forward. */
    allowsBackForwardNavigationGestures?: boolean;
  };
}

/** A web page (WKWebView / android.webkit.WebView); its `ref` has
 * `reload()`, `goBack()`, `goForward()`, `stopLoading()`,
 * `injectJavaScript(script)` and `postMessage(data)`. */
export const WebView: (props: WebViewProps) => YNode = hostComponent<WebViewProps>("webview");
