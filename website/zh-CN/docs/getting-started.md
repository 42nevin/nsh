---
title: 快速开始
description: 用 nsh init 创建应用，逐行阅读它的 App.tsx，在 Mac Catalyst、iOS 模拟器或 Android 上运行，并借助热重载编辑。
order: 3
demo: guide-getting-started
---

# 快速开始

本页创建一个新应用，讲解 `nsh init` 生成的代码，并在热重载下运行它。这里假设 `nsh` 已安装并位于 `PATH` 中（[安装](./installation.md)）；如果使用源码仓库，请改用 `./target/debug/nsh`。

## 创建应用

```sh
nsh init my-app
```

`nsh init` 会在目录不存在时创建它，并写入：

| 文件 | 作用 |
|---|---|
| `App.tsx` | 入口模块：用 `mount` 渲染应用 |
| `nsh.json` | 应用的名称、id、版本和构建号（[项目结构](./project-structure.md#nshjson)） |
| `tsconfig.json` | TypeScript 设置，把 `nsh` 解析到 `.nsh/types` 中的声明 |
| `.gitignore` | 忽略 `.nsh/` 和 `out/` |
| `.nsh/types/` | 生成的所有 nsh 模块的声明，供编辑器使用 |

显示名称默认是目录名；可以用 `--name "My App"` 指定其他名称。id 会是 `com.example.<name>`：发布前请在 `nsh.json` 中修改。已存在的文件会被保留，所以你可以在已经有部分文件的目录里运行 `nsh init`。

## 入口模块

```tsx
import { mount, signal } from "nsh";
import { SafeAreaView } from "nsh/ui/mobile";

function App() {
  const count = signal(0);
  return (
    <SafeAreaView style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 16 }}>
      <text style={{ fontSize: 28, fontWeight: "600" }}>{count()}</text>
      <view
        onPress={() => count.update((c) => c + 1)}
        activeOpacity={0.6}
        accessibilityRole="button"
        style={{ paddingHorizontal: 20, paddingVertical: 12, borderRadius: 10, backgroundColor: "#0969da" }}
      >
        <text style={{ color: "white", fontSize: 17 }}>Tap</text>
      </view>
    </SafeAreaView>
  );
}

mount(App);
```

逐行来看：

- **`import { mount, signal } from "nsh"`**：`nsh` 是核心模块，提供 [signals](./reactivity.md)、`mount` 和其他响应式原语。它由原生运行时提供，不会被打包。
- **`import { SafeAreaView } from "nsh/ui/mobile"`**：移动端 UI 组件集。[`SafeAreaView`](../components/safe-area-view.md) 会为内容加上内边距，使其避开刘海、状态栏和 Home 指示条。
- **`function App()`** 是一个组件：返回 JSX 的函数。它只运行**一次**。`signal(0)` 创建一个会随时间变化的值。
- **`{count()}`** 在 JSX 中读取 signal。编译器看到这是一次调用，就让这段文本跟随该 signal：`count` 变化时，运行时只更新这一段文本，其他什么都不动。
- **`<view onPress>`** 是一个响应按压的原生视图；`activeOpacity` 让它在按下时以原生方式变淡。小写标签（`view`、`text`、`image`、`scroll`、`input` 等）是 [宿主元素](../elements/index.md)。
- **`style={{ … }}`** 接受 React Native 风格的属性。布局是 flexbox，采用 React Native 的默认值（子元素按列排列）（[布局](./layout.md)）。
- **`mount(App)`** 把组件渲染到应用的界面上。

> [!IMPORTANT]
> 文本必须放在 `<text>` 中。直接写在 `<view>` 里的字符串（`<view>Hello</view>`）不会显示。

## 运行

`nsh run` 会启动开发服务器、构建应用、启动加载该应用的开发宿主，并在你编辑时持续重新构建。应用的日志（`console.log`、错误）会显示在终端中。

### Mac Catalyst（默认）

```sh
nsh run my-app/App.tsx
```

应用会在 Mac 上一个 iPhone 尺寸（390×844 点）的窗口中打开。这是最快的开发循环：不需要启动模拟器。它和 iOS 一样使用 UIKit，所以看到的效果就是 iPhone 上的效果，只有 Mac 特有的行为除外，例如用鼠标滚轮滚动。

### iOS 模拟器

```sh
nsh run my-app/App.tsx --target ios
nsh run my-app/App.tsx --target ios --device "iPhone 16 Pro"
```

`nsh` 会使用已启动的模拟器；如果没有正在运行的模拟器，就启动最新的 iPhone，并把开发宿主安装进去。`--device` 按名称或 UDID 选择模拟器。

### Android

```sh
nsh run my-app/App.tsx --target android
```

先启动模拟器或连接设备（`adb devices` 会列出它们；nsh 使用第一个）。`nsh` 会安装一个开发宿主 APK（只在运行时变化时重新构建），并通过 `adb reverse` 把开发服务器的端口转发到设备上，因此通过 USB 即可工作，无需共享网络。

## 用热重载编辑

保持 `nsh run` 运行，修改 `App.tsx` 中的按钮文字：

```tsx
<text style={{ color: "white", fontSize: 17 }}>Add one</text>
```

保存。CLI 重新构建并发送补丁；宿主重新运行变化的模块，并只重新渲染代码有变化的组件。这里变化的是 `App`，它持有 `count` signal，所以计数会归零。当你编辑的组件并不持有状态时，状态会被保留。把按钮移到单独的组件里：

```tsx
function TapButton(props: { onPress: () => void }) {
  return (
    <view onPress={props.onPress} activeOpacity={0.6} style={{ padding: 12, borderRadius: 10, backgroundColor: "#0969da" }}>
      <text style={{ color: "white", fontSize: 17 }}>Tap</text>
    </view>
  );
}
```

在 `App` 中使用 `<TapButton onPress={() => count.update((c) => c + 1)} />`，点几下，然后修改 `TapButton` 的颜色：按钮会更新，计数保持不变。

热替换以顶层组件为单位：名称首字母大写、函数体中包含 JSX 的函数。组件在自身代码变化时，或模块中组件之外的代码（它用到的常量、辅助函数）变化时重新渲染。如果某次编辑无法原地应用（例如导入了运行中的应用尚未导入的 nsh 模块，或修改了 `nsh.json` 中的 capabilities），宿主会重新加载整个应用。

### 错误

构建错误会打印在终端中，并在设备上以红色浮层显示文件、行号和列号；应用继续运行上一次成功的构建。下次构建成功后浮层会消失（也可以点击提前关闭）。运行时抛出的异常以同样方式显示，调用栈会映射回你的 `.tsx` 源码。

## 在终端中试运行

`nsh trace` 以无界面方式（不需要设备）运行应用，打印它产生的原生视图操作及耗时，然后点击第一个可按压的视图两次：

```sh
nsh trace my-app/App.tsx
```

这是检查应用能否正常启动、查看一次交互引起多少工作量的快捷方式（[测试与 trace](./testing-and-trace.md)）。

## 下一步

- [项目结构](./project-structure.md)：`nsh.json` 里写什么、资源、模块。
- [JSX 与组件](./jsx-and-components.md)：props、children、控制流。
- [响应式](./reactivity.md)：signals、计算值和 effect。
- [打包](./packaging.md)：构建可安装的 `.app`、`.ipa` 或 `.apk`。
