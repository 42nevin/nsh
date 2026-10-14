# AGENTS.md

yf 是一个类 React Native 的跨平台 UI 框架：业务代码用 TSX + SolidJS 写，运行在 QuickJS 里；布局用 Taffy；渲染全部使用系统原生 View（UIKit / Android View），因此嵌入地图、WebView 等原生组件不会有层级问题。开发工具链是纯 Rust，不依赖 Node.js。

## 目录结构

```
crates/
  yf-core/            平台无关的核心：shadow tree、样式（RN 语义）、Taffy 布局、文本合并、list 虚拟化、mount 指令
  yf-anim/            原生动画图（纯 Rust）：value / interpolate / math 节点，timing / spring / decay 驱动
  yf-js/              QuickJS 宿主：console/定时器/指令桥、op buffer 解码、字节码编译与加载
  yf-engine/          引擎线程：驱动 yf-js + yf-core，按帧产出 MountBatch 交给平台
  yf-bundler/         打包器：rolldown + Oxc（擦除 TS）+ Solid universal JSX 编译器 + 内置 JS 包 + source map
  yf-dev/             开发工具：dev server 协议、服务端（CLI 用，feature `server`）、客户端（宿主用，feature `client`）
  yf-platform-apple/  UIKit 后端（iOS / 模拟器 / Mac Catalyst）：挂载原生 View、CoreText 文本引擎、输入、Surface、独立 App；
                      feature `dev`：dev server 客户端、日志回传、错误浮层（只有 yf-host 开）
  yf-host/            预编译的开发宿主 App（Catalyst），连接 dev server 或运行本地 bundle
  yf-cli/             `yf` 命令行：build / trace / dev / run
js/runtime/index.ts   Solid universal 渲染器（JS 侧），被打进 yf-bundler
js/vendor/solid-js/   内置的 solid-js（版本见 VERSION）
examples/             示例 App
```

依赖方向：`yf-core` ← `yf-js` ← `yf-engine` ← `yf-platform-*` ← `yf-host`；`yf-cli` 依赖 `yf-bundler`、`yf-js`、`yf-engine`。`yf-core` 不能依赖任何平台或 JS 相关的 crate。

## 数据流与线程模型

```
JS（引擎线程，QuickJS）  Solid 组件 -> js/runtime 把变更 push 进一个扁平数组 -> 微任务里调用 __yf_commit(ops)
yf-js                    在 Rust 侧解码 op buffer（标签、样式对象、事件名都在这里解析）-> Vec<Op>
yf-core（引擎线程）       apply(Op) 维护 shadow tree -> commit()：重建文本、Taffy 布局、diff frame -> MountBatch
平台（主线程）            每次 commit 一次 main queue 跳转，按顺序应用 MountItem 到原生 View
输入                     原生手势 -> EngineControl::dispatch_event -> 引擎线程 -> JS handler
```

- 平台主线程永远不等待 JS。
- **文本只排版一次**：`TextEngine`（core 里定义）在引擎线程上 `prepare`（Apple：CTFramesetter）→ 按不同宽度 `measure` →
  布局结束后按最终尺寸 `layout`（Apple：CTFrame），`MountItem::Text` 把不可变的排版结果交给主线程直接绘制
  （`TextView` 的 `drawRect` 里 `CTFrameDraw`），主线程不做任何文本排版。Android 对应 `StaticLayout`（RN 的 PreparedLayout 同理）。
  实现里不能碰只能在主线程用的 UI 对象。
- 测量缓存：每个引擎一个按内容寻址的 LRU（`MeasureCache`，两代哈希表，≤1024 条），key 是布局相关内容的哈希
  （`AttributedText::layout_hash`，不含颜色）+ 宽度约束；节点上另缓存 `Prepared` 和最终排版。只改颜色不触发布局，只重做最终排版。
- 一个 `Surface` 对应一个引擎（一个 QuickJS runtime），不使用全局单例，以支持嵌入已有 App、多实例共存。独立 App 只是「一个铺满窗口的 Surface」。

## 常用命令

```sh
cargo build -p yf-cli                                         # 构建 yf 命令行
cargo build -p yf-host --target aarch64-apple-ios-macabi      # 构建 Catalyst 开发宿主（yf run 需要）
./target/debug/yf trace examples/counter/App.tsx              # 无界面运行：打印 mount 指令和每次提交的耗时
./target/release/yf trace examples/gallery/App.tsx --scroll 3000 --taps 0   # 无界面滚动（每帧 10pt），看 list 每帧开销
./target/debug/yf run examples/counter/App.tsx --trace        # dev server + Catalyst 宿主（390×844），改代码自动热重载
./target/debug/yf dev examples/counter/App.tsx                # 只起 dev server（给模拟器/真机），打印本机和局域网地址
./target/debug/yf build examples/counter/App.tsx --release    # 输出 bundle.js / bundle.qbc

cargo test --workspace
cargo test --release -p yf-engine --test list_bench -- --ignored --nocapture   # gallery 滚动基准（JS + core）
cargo test --release -p yf-core bench_ -- --ignored --nocapture                # 行/cell 重排基准
cargo +nightly fmt --all                                      # .rustfmt.toml 用了 nightly 选项
cargo clippy --workspace --all-targets
cargo clippy -p yf-platform-apple -p yf-host --target aarch64-apple-ios-macabi
```

`yf-platform-apple` 只在 `target_os = "ios"` 下编译（iOS 真机、iOS 模拟器、Catalyst 都满足）；在 macOS 主机上它是空 crate，所以平台代码一定要用 Catalyst target 跑一遍 clippy。

## 元素与能力

| JSX | 原生（Apple） | 说明 |
|---|---|---|
| `<view>` | UIView | 支持 `onPress` / `onLongPress` / `pan={{x, y}}` + `onPanStart` / `onPanEnd` |
| `<text>` | `TextView`（CTFrame） | 嵌套 `<text>` 是 span |
| `<image src>` | UIImageView | `src` 是 URL 或 `import img from "./a.png"`（带尺寸的资源描述）；`resizeMode`；`onLoad` / `onError` |
| `<scroll>` | UIScrollView | `horizontal`、`refreshing` + `onRefresh`、`scrollValue`、`onScrollEnd` |
| `List`（运行时组件） | UIScrollView + 复用 cell | `each` / `estimatedItemSize` / `onEndReached` / 下拉刷新 / `scrollValue` |

- **List**：窗口计算在 Rust（`core/list.rs`，Fenwick 前缀和），可视区上下各多渲染一屏。Rust 通过 `__yf_list(listId, [slot, index, …])` 让 JS 绑定 cell；
  JS 先在 `batch` 里更新 cell 内容，再设置 `cellIndex` 确认，core 只测量/定位已确认的 cell。新 cell 创建时直接带首个 index。
  引擎每帧最多 8 轮「绑定 → JS → 布局」并合并成一个 batch。行内状态会随复用串行，要按 id 存在外部。
- **动画**：`createNativeValue()` → `timing` / `spring` / `decay`（返回 Promise）、`interpolate`（数值或颜色）、`add/sub/mul/div`。
  可绑定 `opacity`、`backgroundColor`、`transform` 的 translate/scale/rotate。主线程 `CADisplayLink` 驱动，无动画时暂停。
  `scrollValue` 让滚动偏移同帧写入原生值；`pan` 让拖动直接驱动原生值，结束时 JS 收到速度。
  JS 侧原生值被 view 的 style 引用时挂在 view 节点上（`YNode.anims`），派生节点持有输入，避免被 GC 后绑定失效。
- **图片**：后台 NSURLSession / 文件读取 → ImageIO 按 view 像素尺寸降采样解码 → `NSCache`（内存紧张自动回收）；同 key 合并请求，无人等待时取消下载。
  `asset:/path` 在开发时走 dev server 的 `/assets/`，`yf build` 会把资源复制到 `out/assets/`。

## 开发服务器（yf dev / yf run）

- CLI 用 notify 监听源码，重新打包后通过 WebSocket（`/ws`，默认端口 8790，被占用顺延）推给所有已连接的宿主；
  新连接的客户端先订阅再收当前最新状态，不会漏事件。协议见 `crates/yf-dev/src/protocol.rs`（JSON 文本帧 + 二进制帧）。
- 字节码版本协商：客户端握手时报 `BYTECODE_VERSION`，不一致时服务端自动改推 JS 源码。
- 宿主的 `log`（含 JS `console.*`，target `js`）连上后回传到 CLI 终端（带设备名），断开时写 stderr。
- 构建失败、bundle 执行失败、未捕获异常都会在宿主上显示红色浮层（点按关闭），修好后自动重载并消失。
- 堆栈经 source map 符号化：打包时生成 hidden source map（TSX→JSX→JS 两步 map 用 `rolldown_sourcemap::collapse_sourcemaps` 合成），
  随 bundle 推送，引擎在报告错误前把 `bundle.js:L:C` 换成 `App.tsx:L:C`。

## 代码规范

- 格式：`cargo +nightly fmt --all`，配置见 `.rustfmt.toml`。
- workspace lints：`missing_docs = warn`、`unreachable_pub = deny`、`private_interfaces = deny`。新 crate 必须写 `[lints] workspace = true`。
  - 公开 API 都要有文档注释。
  - 私有模块里的条目用 `pub(crate)`，不要写 `pub`。
- 注释、文档、日志用英文。注释说明「为什么」，不要复述代码。
- 库 crate 只用 `log` 门面输出（不要 `eprintln!`）；性能日志用 target `yf::perf`，JS 日志用 target `js`。日志器由二进制安装。
- 依赖统一在根 `Cargo.toml` 的 `[workspace.dependencies]` 里声明，crate 里只写 `.workspace = true`。
- `unsafe` 块必须带 `// SAFETY:` 注释。

## 性能规则（本项目的首要指标）

1. **QuickJS 是最慢的一环，JS 侧只做最少的工作。** `js/runtime` 只往同一个复用数组里 push 原始值。解析、diff、字符串处理一律放在 Rust 侧。不要在 JS 侧做 JSON 序列化。
2. **跨线程按批次。** 一次 JS 回合 → 一次 `__yf_commit` → 一次 `Core::commit` → 一个 `MountBatch` → 一次主线程跳转。不要逐条指令跨 FFI 或跨线程。
3. **热路径不做多余分配。** node id 稠密递增，直接用 `Vec` 索引（`slots`），不要用哈希表。内部链接用 slab key。缓冲区复用（`take_ops`、`queue`）。
4. **能不发的 mount 指令就不发。** 新建 view 不发默认样式和空事件；样式没变就不发；frame 没变就不发。用 `yf trace` 看指令条数。
5. **文本**：测量结果按约束缓存（`TextContext.cache`）；平台侧字体对象要缓存（`FontCache`）。
6. **改动热路径时，要附上 `yf trace` 前后的耗时和指令条数对比。** 当前基线（counter 示例，Apple Silicon）：
   - 无界面（debug）：首帧约 0.5ms（57 ops / 48 mount items），点击一次约 0.15ms（12 ops / 8 items）。
   - Catalyst（release）：首帧约 1.05–1.15ms（其中 CoreText 冷排版 10 段文本约 0.7ms），点击一次约 0.12ms。
   - 热重载：保存文件到设备加载新版本约 120ms（含文件事件延迟和约 30ms 打包）。
   - gallery 示例（debug，Catalyst）：首帧 22 行图文列表 598 ops / 205 节点，约 3.7ms（含 list 绑定轮次）。
   - gallery 滚动（release）：换一行的帧 4 ops / 4 mount items；不滚出新行的帧不调 JS、不产出 batch。
     `list_bench` 连续跑每帧约 20–30µs（JS 约 20µs），每帧间冲掉缓存约 40µs，但线程睡 10ms 后约 150µs ——
     差异来自线程睡眠时 CPU 降频（Apple CLPC），不是代码路径；QoS（引擎线程已设 USER_INTERACTIVE）和 Mach time-constraint 策略都改变不了。
     评估优化效果要看「连续」和「冲缓存」两列，`yf trace` 的单帧耗时受降频影响很大。
7. dev profile 对 `rquickjs-sys` 和 `taffy` 开了 `opt-level = 3`，debug 构建的耗时也有参考价值；release 用 fat LTO。

## 已验证的约束和踩过的坑

- **rquickjs 不能开 `full` feature。** 它会带上 `dyn-load`（dlopen2），在 iOS/Catalyst 上编译不过。iOS 系列 target 没有预置 bindings，需要 `bindgen` feature（以及 libclang）。
- **QuickJS 的微任务需要宿主手动执行。** 每次从 Rust 调进 JS 后都要 `Host::run_jobs()`。`setTimeout`/`console` 是我们自己实现的，`queueMicrotask`、`WeakRef`、`FinalizationRegistry` 是 QuickJS-NG 内置的。
- **JS 对象图必须从全局可达。** `js/runtime` 用 `globalThis.__yf` 挂住 root、registry 和 listening。没有这一行，渲染完成后整个 App（节点、handler、signal、registry）就是一个孤立的环，空闲 GC 会把它整体回收，表现为点击无反应。`yf trace` 默认空闲 1.2 秒后再点击，`crates/yf-engine/tests/counter.rs` 是这个问题的回归测试。
- **节点释放由 GC 驱动，不是由 Remove 驱动。** Solid 可能把移除的节点留在 memo 里以后再插回去，所以 `Remove` 只是脱离父节点，`Release`（来自 FinalizationRegistry）才会真正释放节点和原生 view。
- **`yf_js::BUNDLE_NAME`（`bundle.js`）** 是所有 bundle 的模块名，符号化靠它匹配堆栈；`BYTECODE_VERSION` 有测试和链接的 QuickJS 版本对齐，升级 rquickjs 时一起改。
- **`oxc_sourcemap` 用 9.x**（和 rolldown / oxc 0.153 一致）；Solid 编译器内部的 oxc 0.118 带的是 6.x，它只输出 JSON 字符串，不要混用类型。
- **我们插件自己的编译错误**记在 `YfPlugin.error` 里直接展示（干净的代码框 + 相对路径），不用 rolldown 包装后的报告。
- **UIFont 和 CTFont 是 toll-free bridged 的**，文本引擎用 `UIFont::systemFontOfSize_weight` 拿系统字体再转成 CTFont（已渲染验证）。CoreText 属性必须用 CT 的 key（颜色是 CGColor），不能用 UIKit 的 NSForegroundColor。
- **op buffer 的 opcode** 定义在 `js/runtime/index.ts` 和 `crates/yf-js/src/protocol.rs` 两处，必须保持一致。
- **不在 `<text>` 里的文本节点**（比如 `<Show>`、`<For>` 的空占位）不创建原生 view，也不参与布局。`<text>` 里嵌套的 `<text>` 是 span，没有原生 view，整块合并成一个 `AttributedText`。
- **`TextSpan` 的偏移单位是 UTF-16**，和 NSAttributedString / Android Spannable 一致。
- **Taffy 0.15 的自定义 measure 回调必须委托 `taffy::compute_leaf_layout`**，回调里只算内容尺寸。直接返回内容尺寸会跳过叶子自身样式的
  解析（width/height/padding/min/max），表现为 `<image style={{width: 48}}>` 这类叶子尺寸为 0。
- **List 的 cell 是独立的 Taffy 根**（不挂在 list 的 Taffy 节点下，宽度跟随 list 交叉轴）：滚动时只重排 dirty 的 cell，只给重排过的
  cell 子树发 frame；主树不 dirty 时跳过整棵主树的 `compute_layout` 和 frame 遍历（Taffy 对干净的根也会做一遍完整的取整遍历）。
- **Taffy 默认值是 CSS 语义**，`Style::default()` 已经改成 RN 语义：column、flexShrink 0、alignContent flex-start、position relative。
- **打包平台不能设为 Node。** solid-js 的 `node` 导出条件指向没有响应式的 SSR 版本。
- **`dom-expressions-compiler` 锁定在 main 分支的某个 commit。** 已发布的 tag 早于现在用的 `compile` API。这个项目很新，升级前要跑完整测试。
- **`MountItem::Insert` 用兄弟节点（`before`）而不是下标定位**：平台会往容器里插自己的子视图（`UIRefreshControl` 插在最底层、滚动条），下标会错位。
- **Catalyst 的 Mac idiom 不支持 `UIRefreshControl`**（一挂上就抛 `_UICatalystUnsupportedMacIdiomBehavior`）：Mac idiom 下用 scroll delegate 检测下拉 + 菊花 + contentInset 自己实现，iOS 用原生控件。
- **Mac idiom 自定义下拉刷新**：松手时系统的回弹动画目标已按旧 inset 算好（回到 0），只改 `contentInset` 菊花会被藏住；
  开始刷新后在下一个主队列任务里把偏移动画到 `-REFRESH_INSET`。合成测试要用带 phase 的连续滚动事件（began/changed/ended），普通滚轮事件不会越界。
- **`yf run` 安装宿主要「复制到临时文件再 rename」**：原地覆盖正在被另一个会话运行的可执行文件，新启动的进程会因代码签名不匹配被内核 SIGKILL。
- **UIKit 回调可能在 Mounter 应用 batch 时同步触发**（例如设置 `contentOffset` 触发 `scrollViewDidScroll`）：`EventTarget::with_mounter` 拿不到借用就推迟到下一个主队列任务，不要直接 `borrow_mut`。
- **有 transform 的 view 不能设 `frame`**，统一用 `bounds` + `center`；设 `bounds` 时保留原 origin（scroll view 的 bounds.origin 就是滚动偏移）。
- **Catalyst**：`UIDeviceFamily` 必须包含 6（Mac idiom），否则界面会被缩放到 77%。Catalyst 下 container 延伸到标题栏下面，叠加 UI 要约束到 `safeAreaLayoutGuide`。UIDevice 在 Catalyst 里报告 "iPad"。使用 UIScene 生命周期（`UIScreen.mainScreen`、`UIWindow initWithFrame` 已弃用）。窗口尺寸通过 `sizeRestrictions` 固定为 390×844。
- 在 `define_class!` 中返回 `Retained<T>` 的方法要用 `#[unsafe(method_id(...))]`。由 UIKit 按类名创建的类（AppDelegate、SceneDelegate）不能带 ivars，需要的状态放在 static 或 thread_local 里。

## 改动后的验证要求

1. `cargo test --workspace`，以及上面列出的 fmt 和 clippy（含 Catalyst target）。
2. 改了 JS 运行时、引擎或 core：跑 `yf trace`，检查指令和耗时。
3. 改了平台层：`yf run` 实际启动，截图确认画面，必要时模拟点击（需要终端有「录屏」和「辅助功能」权限；锁屏时前台是 `loginwindow`，点击会失败）。
