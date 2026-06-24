# Native FFI 跨平台构建卡点记录

> 起始 commit `69278d8e`（2026-05-11）。记录 iOS / Android 跨编当前状态、所需工具链、阻塞点 + 复现命令。
> **2026-05-11 更新**：换到 Mac (macOS 26.3.1 / Xcode 26.4.1) 后补完了 iOS d2 + mermaid 三个 target 的 staticlib 与 xcframework；plantuml iOS 被同样的 graphviz-anywhere prebuilt 问题阻塞。详见下方"阻塞 #4"。

## 现状一览

| 组件 | Linux x86_64 | Android arm64-v8a | Android armeabi-v7a | Android x86 | Android x86_64 | iOS device (aarch64-apple-ios) | iOS sim arm64 | iOS sim x86_64 | macOS (aarch64-apple-darwin) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `font-metrics` (with `metrics-ffi-callback`) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ |
| `d2-little` 主 crate | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ |
| `mermaid-little` 主 crate | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ |
| `plantuml-little` 主 crate | ✅ | ❌ (graphviz prebuilt) | ❌ | ❌ | ❌ | ❌ (同上) | ❌ | ❌ | ⏳ |
| `supramark-d2-native` (staticlib + cdylib) | ✅ 9.7 MB | ✅ 8.8 MiB | ✅ 10.2 MiB | ✅ 9.1 MiB | ✅ 9.3 MiB | ✅ 33 MiB `.a` / 7.4 MiB `.dylib` | ✅ 33 MiB | ✅ 33 MiB | ⏳ |
| `supramark-mermaid-native` (staticlib + cdylib) | ✅ 9.5 MB | ✅ 8.3 MiB | ✅ 10.3 MiB | ✅ 10.4 MiB | ✅ 9.7 MiB | ✅ 33 MiB `.a` / 7.2 MiB `.dylib` | ✅ 33 MiB | ✅ 33 MiB | ⏳ |
| `supramark-plantuml-native` (staticlib + cdylib) | ✅ 8.3 MB | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ⏳ |
| `SupramarkD2.xcframework` (device + sim universal) | — | — | — | — | — | ✅ 98 MiB（见 `target/ios-xcframeworks/`） | | | — |
| `SupramarkMermaid.xcframework` (device + sim universal) | — | — | — | — | — | ✅ 98 MiB | | | — |

- ✅ 已编译通过且产物可用
- ❌ 已确认阻塞，原因见下文
- ⏳ 工具链尚未在本机配置

## 当前 dev 机环境

### Linux（首发跨编机，前一阶段）

- Ubuntu 24.04.4 LTS (x86_64)
- Rust toolchain 1.93.0
- 已装 Android Rust targets：`aarch64-linux-android`、`armv7-linux-androideabi`、`x86_64-linux-android`、`i686-linux-android`
- Android NDK r27.2.12479018 在 `/opt/android/android-ndk-r27c/`
- `cargo-ndk` 0.18 在 `~/.cargo/bin/`
- 环境变量需要：`export ANDROID_NDK_HOME=/opt/android/android-ndk-r27c`

### macOS（当前跨编机，2026-05-11 起）

- macOS 26.3.1 (Darwin 25.3.0)
- Xcode 26.4.1 / Build 17E202 — `xcrun --show-sdk-path -sdk iphoneos` 指向 `iPhoneOS26.4.sdk`
- Rust toolchain 1.95.0
- 已装 iOS Rust targets：`aarch64-apple-ios`、`aarch64-apple-ios-sim`、`x86_64-apple-ios`（外加宿主 `aarch64-apple-darwin`）
- Android NDK 尚未配置 — Android 跨编要么重装 NDK、要么换回 Linux 机
- ⚠️ **Xcode 26.4.1 系统插件断裂**：`xcodebuild -create-xcframework` 启动时报 `IDESimulatorFoundation` 找不到 `DVTDownloads.developerDocumentation` 符号，无法运行。本机改用 `scripts/build-ios-xcframework.sh` 手工拼装 xcframework 绕过；根因需要 `sudo xcodebuild -runFirstLaunch` 重新安装系统组件后再确认。

## 阻塞 #1 — plantuml Android 编译卡 graphviz-anywhere prebuilt

### 错误现象

```
thread 'main' panicked at crates/graphviz-anywhere/packages/rust/build.rs:232:5:
Unable to locate graphviz_api. Tried in order: GRAPHVIZ_ANYWHERE_DIR / GRAPHVIZ_NATIVE_DIR
env override; packages/rust/prebuilt/<os>/libgraphviz_api.{a,lib}; sibling output/<platform>
/lib/; GitHub release download for v$CARGO_PKG_VERSION.
```

### 根因

- `plantuml-little` 硬依赖 `graphviz-anywhere = "0.1.8"`（plantuml-little/Cargo.toml:128）
- `graphviz-anywhere` 的 build.rs 期望 `libgraphviz_api.a` 在以下位置之一：
  1. `GRAPHVIZ_ANYWHERE_DIR` / `GRAPHVIZ_NATIVE_DIR` 环境变量指定的路径
  2. `crates/graphviz-anywhere/packages/rust/prebuilt/<os>/libgraphviz_api.{a,lib}`
  3. 同级 `output/<platform>/lib/`
  4. GitHub release 下载 `v$CARGO_PKG_VERSION`
- 仓库自带的 Android tarball 是 LFS 占位空文件（9 字节 ASCII，不是真二进制）：
  ```
  crates/graphviz-anywhere/packages/react-native/graphviz-native-android-arm64-v8a.tar.gz: 9 bytes
  ```
- GitHub release 没有 `linux→android` 的 prebuilt（404）

### 出路（择一）

**A. 自行用 NDK 跨编 `libgraphviz_api`**（推荐，长期正解）

`crates/graphviz-anywhere/capi/CMakeLists.txt` 是 C 源。用 NDK 工具链跨编 4 个 ABI，产物放 `prebuilt/android-<abi>/libgraphviz_api.a` 或设 `GRAPHVIZ_ANYWHERE_DIR` 环境变量。

```bash
# 大致命令（细节看 graphviz-anywhere README）
export ANDROID_NDK_HOME=/opt/android/android-ndk-r27c
cd crates/graphviz-anywhere
for abi in arm64-v8a armeabi-v7a x86 x86_64; do
  cmake -B build-android-$abi \
    -DCMAKE_TOOLCHAIN_FILE=$ANDROID_NDK_HOME/build/cmake/android.toolchain.cmake \
    -DANDROID_ABI=$abi \
    -DANDROID_PLATFORM=android-24 \
    -DCMAKE_BUILD_TYPE=Release \
    capi
  cmake --build build-android-$abi
done
```

预估 1-2 小时（含调试 graphviz 自身依赖：libexpat、libz、libltdl 等可能也要跨编）。

**B. plantuml-little 把 graphviz-anywhere 改为 optional feature**

```toml
[dependencies]
graphviz-anywhere = { version = "0.1.8", optional = true }

[features]
default = ["metrics-ttf-parser", "graphviz-dot"]
graphviz-dot = ["dep:graphviz-anywhere"]
```

然后在 plantuml-little 源代码里把所有 `crate::graphviz_anywhere::*` 调用用 `#[cfg(feature = "graphviz-dot")]` 包起来，关掉后 `!include` / `dot` layout 不可用，svek / smetana / 其它 layout 继续工作。

Android build 时 `--no-default-features --features metrics-ffi-callback`。这是 plantuml-little 主 crate 的源码 refactor，30-60 分钟工作量。

**C. 接受 plantuml Android 暂不支持**

把 plantuml 从 RN 支持列表里暂时去掉，文档说明。d2 + mermaid 先打通完整链路。

### 复现命令

```bash
cd /ext/Actrium/supramark
export ANDROID_NDK_HOME=/opt/android/android-ndk-r27c
cargo ndk -t arm64-v8a build --release -p plantuml-little \
  --no-default-features --features metrics-ffi-callback
# → graphviz-anywhere build.rs panic
```

## 阻塞 #2 — iOS 跨编 (d2 + mermaid) ✅ 已通

> **状态变更（2026-05-11）**：在 Mac 上跑通了 d2 + mermaid 三个 iOS target，并手工拼装出可发布的 xcframework。下面保留原 Linux 上的阻塞描述与 Mac 上的实际操作，留作复盘参考。plantuml 仍卡在 graphviz-anywhere（见阻塞 #4）。

### 原阻塞条件

iOS 跨编需要 macOS + Xcode：

- Xcode Command Line Tools（`xcrun` / `lipo` / `xcodebuild`）
- iOS SDK（`xcrun --show-sdk-path -sdk iphoneos`）
- Rust targets：`aarch64-apple-ios`（真机）+ `aarch64-apple-ios-sim`（M 系列 Mac 模拟器）+ `x86_64-apple-ios`（Intel Mac 模拟器）

Linux 本机**完全做不了**这步（缺 Apple SDK，且 Apple ToS 不允许 Linux 跨编 iOS target）。

### Mac 上实际跑通的步骤（2026-05-11）

```bash
# 1. 安装 iOS Rust targets
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

# 2. 三个 target 各编一次（d2 + mermaid 同一条命令完成）
for target in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
  cargo build --release --target "$target" \
    -p supramark-d2-native -p supramark-mermaid-native
done

# 3. 模拟器两片用 lipo 合并 universal staticlib
mkdir -p target/ios-sim-universal/release
for lib in libsupramark_d2_native.a libsupramark_mermaid_native.a; do
  lipo -create \
    "target/aarch64-apple-ios-sim/release/$lib" \
    "target/x86_64-apple-ios/release/$lib" \
    -output "target/ios-sim-universal/release/$lib"
done

# 4. 拼装 xcframework
#    本机 xcodebuild 插件挂掉，改用本仓库的手工脚本（产物结构与 xcodebuild 一致）
scripts/build-ios-xcframework.sh supramark-d2-native \
  crates/d2-little/packages/native/include libsupramark_d2_native.a \
  target/ios-xcframeworks/SupramarkD2.xcframework
scripts/build-ios-xcframework.sh supramark-mermaid-native \
  crates/mermaid-little/packages/native/include libsupramark_mermaid_native.a \
  target/ios-xcframeworks/SupramarkMermaid.xcframework
```

构出物（`target/ios-xcframeworks/`）：

```
Supramark{D2,Mermaid}.xcframework/
├── Info.plist                            # AvailableLibraries 描述两个切片
├── ios-arm64/                            # 真机切片（aarch64-apple-ios）
│   ├── Headers/supramark_{d2,mermaid}.h
│   └── libsupramark_{d2,mermaid}_native.a
└── ios-arm64_x86_64-simulator/           # 模拟器切片（lipo arm64 + x86_64）
    ├── Headers/supramark_{d2,mermaid}.h
    └── libsupramark_{d2,mermaid}_native.a
```

- `plutil -lint Info.plist` 通过
- `nm -gU` 验证导出符号：`_supramark_d2_render` / `_supramark_d2_free` / `_supramark_d2_version` / `_supramark_install_metrics_callback`（mermaid 同名前缀）

### Xcode 26.4.1 上 `xcodebuild -create-xcframework` 失败原因（备忘）

```
A required plugin failed to load. Please ensure system content is up-to-date — try running 'xcodebuild -runFirstLaunch'.
dlopen(/Applications/Xcode.app/.../IDESimulatorFoundation, 0x0109):
  Symbol not found: _$s12DVTDownloads21DownloadableAssetTypeO22developerDocumentationyA2CmFWC
  Referenced from: IDESimulatorFoundation
  Expected in:     /Library/Developer/PrivateFrameworks/DVTDownloads.framework
```

`xcodebuild -version` 能跑，其它子命令都因插件无法加载而早退。修复方式：`sudo xcodebuild -runFirstLaunch`（需输密码）。在没跑这一步前 xcframework 全部走 `scripts/build-ios-xcframework.sh` 手工生成。

### 老命令（保留作为 `xcodebuild` 修复后的参考）

```bash
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libsupramark_d2_native.a \
  -headers crates/d2-little/packages/native/include \
  -library target/ios-sim-universal/release/libsupramark_d2_native.a \
  -headers crates/d2-little/packages/native/include \
  -output target/ios-xcframeworks/SupramarkD2.xcframework
```

## 阻塞 #4 — plantuml iOS 卡 graphviz-anywhere（与 #1 同根因）

### 实测复现（2026-05-11，Mac）

```bash
cargo build --release --target aarch64-apple-ios -p plantuml-little \
  --no-default-features --features metrics-ffi-callback
# → graphviz-anywhere v0.1.8 build.rs panic：
#   Unable to locate graphviz_api. Tried in order:
#   GRAPHVIZ_ANYWHERE_DIR / GRAPHVIZ_NATIVE_DIR env override;
#   packages/rust/prebuilt/<os>/libgraphviz_api.{a,lib};
#   sibling output/<platform>/lib/;
#   GitHub release download for v$CARGO_PKG_VERSION.
```

### 根因

`crates/graphviz-anywhere/packages/rust/build.rs` 的搜索逻辑只覆盖 host OS：

| 路径 | 覆盖目标 |
| --- | --- |
| `try_prebuilt` | 仅 `cfg!(target_os = "macos"/"linux"/"windows")` — iOS 直接 `return false` |
| `try_github_release` | 仅 `(linux, x86_64)` 与 `(macos, *)` — iOS / Android 直接 `return false` |
| `try_env_override` | 需要事先 `export GRAPHVIZ_ANYWHERE_DIR=...` 指向预编译好的 iOS 版 `libgraphviz_api.a` |

仓库里 `crates/graphviz-anywhere/packages/rust/prebuilt/` 只有 `.gitkeep`，`packages/react-native/` 下没有 ios/android 的 tarball（之前在 Linux 上看到的 9 字节 LFS 占位文件，在 Mac 上根本不存在 — 它从来没真的被 push 过）。

### 出路

与阻塞 #1 完全相同 — 自行用 NDK / Xcode 跨编 `libgraphviz_api`（方案 A），或在 plantuml-little 主 crate 把 graphviz-anywhere 改成 optional feature（方案 B，推荐），或暂时把 plantuml 从 native 支持列表里去掉（方案 C）。

iOS 这个 case 的具体方案 A 命令：

```bash
cd crates/graphviz-anywhere
SDK=$(xcrun --show-sdk-path -sdk iphoneos)
SIM_SDK=$(xcrun --show-sdk-path -sdk iphonesimulator)
# 真机 arm64
cmake -B build-ios-arm64 -DCMAKE_SYSTEM_NAME=iOS \
  -DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_SYSROOT="$SDK" \
  -DCMAKE_BUILD_TYPE=Release capi
cmake --build build-ios-arm64
# 模拟器 arm64
cmake -B build-iossim-arm64 -DCMAKE_SYSTEM_NAME=iOS \
  -DCMAKE_OSX_ARCHITECTURES=arm64 -DCMAKE_OSX_SYSROOT="$SIM_SDK" \
  -DCMAKE_BUILD_TYPE=Release capi
cmake --build build-iossim-arm64
# 模拟器 x86_64
cmake -B build-iossim-x64 -DCMAKE_SYSTEM_NAME=iOS \
  -DCMAKE_OSX_ARCHITECTURES=x86_64 -DCMAKE_OSX_SYSROOT="$SIM_SDK" \
  -DCMAKE_BUILD_TYPE=Release capi
cmake --build build-iossim-x64
```

> ⚠️ graphviz 自身依赖（libexpat、libz、libltdl 等）在 iOS / Android 跨编下都会再触发一轮，要么用 system framework 替换，要么也一并跨编 — 实测预计 2-4 小时调通，比 Android NDK 那套更费事。**推荐先走方案 B**，省下时间投入 RN wrapper。

## 阻塞 #3 — RN turbomodule wrapper npm 包不存在

每个 engine 还需要一个 RN native 包，例如 `@actrium/supramark-d2-native-rn`，承担：

1. 把 `libsupramark_d2_native.a` / `.so` 打进 iOS .xcframework + Android jniLibs 各 ABI
2. RN TurboModule / Old NativeModule wrapper：
   - iOS：Swift / ObjC 调 `supramark_d2_render` C 函数 → 转 JS Promise
   - Android：JNI 调同上 → 转 Promise
3. JS entry 调 `@supramark/engines/rn` 的 `registerNativeEngineAdapter({ engine: 'd2', render })` 在 module 加载时自动注册

参考已有的 `@actrium/graphviz-anywhere-rn`（`crates/graphviz-anywhere/packages/react-native/`）作模板。每个 engine 大概 200-400 LOC 包装 + per-platform 二进制。

## 待办优先级（2026-05-11 当前 Mac 上更新后）

剩下的事按收益从高到低：

1. **RN TurboModule wrapper（首个 engine，d2 推荐）** — 把 `target/ios-xcframeworks/SupramarkD2.xcframework` + Android jniLibs 各 ABI 的 `.so` 打成一个 npm 包，写 Swift / JNI bridge → JS Promise。是阻塞 RN 端落地的最后一步，预计 1-2 天。
2. **plantuml 解阻塞** — 推荐方案 B（graphviz-anywhere 改 optional feature）一刀切；如果产品同意 plantuml 暂不支持 native，方案 C 写进 RN 文档即可。
3. **修 `xcodebuild`** — `sudo xcodebuild -runFirstLaunch`，让官方 `-create-xcframework` 在 CI 上也能跑（手工脚本只是绕过，长期还是要走官方路径）。
4. **macOS 宿主跑通** — `aarch64-apple-darwin` 在当前 Mac 上跨编一次，留作 desktop 桥；目前只需要 `cargo build --release -p supramark-d2-native -p supramark-mermaid-native` 验证一次，不会有新阻塞。
5. **回 Linux 机或在 Mac 上装 NDK** — 重跑 Android d2/mermaid，验证 reproducibility；同时把 plantuml Android 4 个 ABI 在方案 B 落地后跑出来。

## 已 push 的 commit 链（origin/main）

```
21adaa10 docs: record iOS/Android native FFI cross-compile blockers
69278d8e feat(engines): wire RN native engine adapter registry for d2/mermaid/plantuml
57844d99 feat(mermaid-little): add native FFI wrapper for iOS / Android / RN
c8beb66e feat(plantuml-little): add native FFI wrapper for iOS / Android / RN
83ee6d5a feat(d2-little): add native FFI wrapper for iOS / Android / RN
8ac5a028 feat(font-metrics): implement metrics-ffi-callback for native (RN) host bridge
349bab63 refactor(web): drop SSR entry — focus on client-side rendering only
```

> 2026-05-11 Mac 上跑出的 iOS xcframework + lipo 产物**尚未提交** — 它们体积大（每个 .xcframework 约 98 MiB），按惯例不入 git，作为 release 工件随 RN wrapper 一起分发。

## 已生成产物（不在 git 里，需重新生成）

### Android（前一阶段 Linux 上）

`target/aarch64-linux-android/release/`、`target/armv7-linux-androideabi/release/`、`target/i686-linux-android/release/`、`target/x86_64-linux-android/release/` 下：
- `libsupramark_d2_native.so` ×4
- `libsupramark_mermaid_native.so` ×4
- 对应的 `.a` 文件 ×8

每次重跑：
```bash
export ANDROID_NDK_HOME=/opt/android/android-ndk-r27c
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 build --release \
  -p supramark-d2-native -p supramark-mermaid-native
```

### iOS（2026-05-11 本机 Mac 上）

各 target 单独 staticlib + dylib：

- `target/aarch64-apple-ios/release/libsupramark_{d2,mermaid}_native.{a,dylib}` — 真机
- `target/aarch64-apple-ios-sim/release/libsupramark_{d2,mermaid}_native.{a,dylib}` — Apple Silicon 模拟器
- `target/x86_64-apple-ios/release/libsupramark_{d2,mermaid}_native.{a,dylib}` — Intel 模拟器
- `target/ios-sim-universal/release/libsupramark_{d2,mermaid}_native.a` — lipo 后的模拟器 universal `.a`

可消费的 xcframework：

- `target/ios-xcframeworks/SupramarkD2.xcframework/`
- `target/ios-xcframeworks/SupramarkMermaid.xcframework/`

每次重跑：

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
for target in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
  cargo build --release --target "$target" \
    -p supramark-d2-native -p supramark-mermaid-native
done
mkdir -p target/ios-sim-universal/release
for lib in libsupramark_d2_native.a libsupramark_mermaid_native.a; do
  lipo -create \
    "target/aarch64-apple-ios-sim/release/$lib" \
    "target/x86_64-apple-ios/release/$lib" \
    -output "target/ios-sim-universal/release/$lib"
done
scripts/build-ios-xcframework.sh supramark-d2-native \
  crates/d2-little/packages/native/include libsupramark_d2_native.a \
  target/ios-xcframeworks/SupramarkD2.xcframework
scripts/build-ios-xcframework.sh supramark-mermaid-native \
  crates/mermaid-little/packages/native/include libsupramark_mermaid_native.a \
  target/ios-xcframeworks/SupramarkMermaid.xcframework
```
