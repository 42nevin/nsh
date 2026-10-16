---
title: 简介
description: native-shell 是什么、应用如何运行，以及它与其他跨平台框架的比较。
order: 1
---

# 简介

native-shell（简称 nsh）是一个用 TypeScript 和 JSX 构建 iOS 与 Android 应用的框架。应用代码运行在内嵌的 JavaScript 引擎里，除此之外的一切都是原生的：响应式图、渲染器、布局、文本以及应用导入的每个模块都用 Rust 编写，界面由平台自己的视图组成（iOS 与 Mac Catalyst 上是 UIKit，Android 上是 Android `View`）。没有 WebView，也没有重绘整个界面的画布。

```tsx
import { mount, signal } from "nsh";

function Counter() {
  const count = signal(0);
  return (
    <view style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 12 }}>
      <text style={{ fontSize: 48, fontWeight: "700" }}>{count()}</text>
      <view
        onPress={() => count.update((n) => n + 1)}
        activeOpacity={0.6}
        class="rounded-lg bg-blue-600 px-5 py-3"
      >
        <text class="text-white text-lg">Tap</text>
      </view>
    </view>
  );
}

mount(Counter);
```

`<view>` 会成为一个 `UIView` 或 Android `ViewGroup`，`<text>` 成为原生文本视图，按钮的按下反馈（`activeOpacity`）在主线程上运行，无需往返 JavaScript。点击时，`count.update` 改变一个 signal，只有读取 `count()` 的那段文本会更新：组件函数不会再次运行。

## 应用如何运行

一个 nsh 应用是一组带入口的 `.tsx` / `.ts` 模块，入口通常是 `App.tsx`。工具链只有一个 `nsh` 可执行文件：

1. **构建。** `nsh` 打包你的模块（借助 rolldown 和 Oxc），并用自己的编译器编译 JSX。对 `nsh` 与 `nsh/*` 的导入保留为导入：运行时以原生方式提供它们，因此 bundle 里只有你的代码。bundle 会被编译为 QuickJS 字节码。
2. **运行。** 在设备上，引擎线程用 [QuickJS](https://github.com/quickjs-ng/quickjs) 执行字节码。编译后的 JSX 直接调用原生渲染器；一次 JavaScript 执行轮次中的变更被收集起来，由 [Taffy](https://github.com/DioxusLabs/taffy)（flexbox 与 CSS grid）完成布局，再以每帧一批视图操作的形式交给主线程。
3. **渲染。** 主线程把这批操作应用到原生视图上。文本在引擎线程上测量和排版一次（Apple 平台用 CoreText，Android 用 `StaticLayout`），主线程只负责绘制。

由于 JavaScript 只描述界面、处理事件，解释器里的工作量很小。解析样式、解析 Tailwind 类名、差异比较、布局和文本处理都在 Rust 中完成。

> [!NOTE]
> 主线程从不等待 JavaScript。系统同步提出的问题（这个输入框还能再接受一个字符吗、用户可以侧滑返回吗、这个面板可以关闭吗）由原生层根据你事先设置的 props 直接回答，例如 `maxLength`、`preventBack` 或 `dismissible`。事件随后才会到达你的代码。

## 你能得到什么

- **原生视图。** 嵌入原生地图、网页视图或视频播放器不存在层级问题，无障碍使用平台自己的树，文本输入就是系统的输入。
- **细粒度响应式。** 类似 Solid 的 [signals](./reactivity.md)，以原生方式实现。组件只运行一次；只有读取了已变化 signal 的表达式会重新运行。
- **React Native 风格的样式**，并补充了 CSS 能力：采用 React Native 默认值的 flexbox、CSS grid、盒阴影、渐变、滤镜、过渡（[样式](./styling.md)）。
- **Tailwind CSS**，原生解析：`class="flex-row gap-2 p-4 dark:bg-zinc-900 active:opacity-80"`（[Tailwind](./tailwind.md)）。
- **移动端 UI 组件集**（`nsh/ui/mobile`），组件用 Rust 编写，在 iOS 和 Android 上外观一致：[Button](../components/button.md)、[Card](../components/card.md)、[ListView](../components/list-view.md)、[导航](../components/navigation.md) 等。
- **单一可执行文件的工具链。** 不需要 Node.js，不需要 `node_modules`，应用开发者也不需要 Rust 工具链。`nsh init` 会把 TypeScript 声明写入项目，让编辑器对应用做类型检查。
- **模块热替换。** `nsh run` 在保存时重新构建并为运行中的应用打补丁；只有你编辑过的组件会重新渲染。
- **签名包。** 嵌入 nsh 的应用可以下载并校验用你的发布密钥签名的应用 bundle（[签名包](./signed-packages.md)）。

## 运行平台

| 平台 | 最低版本 | 说明 |
|---|---|---|
| iOS | iOS 15.0 | 真机与模拟器 |
| Mac Catalyst | – | 在 Mac 上作为 iPhone 尺寸的开发窗口使用（`nsh run`） |
| Android | API 28（Android 9） | `arm64-v8a` 与 `x86_64` |

nsh 应用既可以打包为独立应用（`nsh package`），也可以作为一个视图嵌入现有原生应用（[iOS](./embedding-ios.md)、[Android](./embedding-android.md)）。

## 与其他框架的比较

React Native 同样渲染原生视图，但它的渲染器和 React 的协调器运行在 JavaScript 中，工具链构建在 Node.js 和各平台的构建系统之上。nsh 只把你的应用代码留在 JavaScript 中：渲染器、响应式图和各模块都是原生的，更新是细粒度的，而不是重新渲染组件再比较虚拟树。Flutter 用自己的组件集自绘每一个像素，因此它的控件是在模仿平台控件，而不是平台控件本身；nsh 使用系统的视图、文本引擎和输入。基于 WebView 的框架（Capacitor、Cordova）在浏览器引擎里渲染 HTML；nsh 没有 DOM，也没有浏览器引擎。API 的详细对照见 [与 React Native 的比较](./react-native-comparison.md)。

## 下一步

- [安装](./installation.md)：获取 `nsh` 命令和各平台工具。
- [快速开始](./getting-started.md)：创建一个应用并在模拟器上运行。
- [JSX 与组件](./jsx-and-components.md) 和 [响应式](./reactivity.md)：其他一切都建立在这两个概念之上。
