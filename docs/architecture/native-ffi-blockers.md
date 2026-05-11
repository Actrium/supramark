# Native FFI 跨平台构建卡点记录

> 截至 commit `69278d8e`（2026-05-11）。记录 iOS / Android 跨编当前状态、所需工具链、阻塞点 + 复现命令。换机器后按此清单继续。

## 现状一览

| 组件 | Linux x86_64 | Android arm64-v8a | Android armeabi-v7a | Android x86 | Android x86_64 | iOS (aarch64-apple-ios) | macOS (aarch64-apple-darwin) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `font-metrics` (with `metrics-ffi-callback`) | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ 未跑 | ⏳ 未跑 |
| `d2-little` 主 crate | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ | ⏳ |
| `mermaid-little` 主 crate | ✅ | ✅ | ✅ | ✅ | ✅ | ⏳ | ⏳ |
| `plantuml-little` 主 crate | ✅ | ❌ (graphviz prebuilt) | ❌ | ❌ | ❌ | ⏳ | ⏳ |
| `supramark-d2-native` (cdylib) | ✅ 9.7 MB | ✅ 8.8 MiB | ✅ 10.2 MiB | ✅ 9.1 MiB | ✅ 9.3 MiB | ⏳ | ⏳ |
| `supramark-mermaid-native` (cdylib) | ✅ 9.5 MB | ✅ 8.3 MiB | ✅ 10.3 MiB | ✅ 10.4 MiB | ✅ 9.7 MiB | ⏳ | ⏳ |
| `supramark-plantuml-native` (cdylib) | ✅ 8.3 MB | ❌ | ❌ | ❌ | ❌ | ⏳ | ⏳ |

- ✅ 已编译通过且产物可用
- ❌ 已确认阻塞，原因见下文
- ⏳ 工具链尚未在本机配置（需要 Mac 或对应 NDK / Xcode）

## 当前 Linux dev 机环境

- Ubuntu 24.04.4 LTS (x86_64)
- Rust toolchain 1.93.0
- 已装 Android Rust targets：`aarch64-linux-android`、`armv7-linux-androideabi`、`x86_64-linux-android`、`i686-linux-android`
- Android NDK r27.2.12479018 在 `/opt/android/android-ndk-r27c/`
- `cargo-ndk` 0.18 在 `~/.cargo/bin/`
- 环境变量需要：`export ANDROID_NDK_HOME=/opt/android/android-ndk-r27c`

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
cd /ext/kookyleo/supramark
export ANDROID_NDK_HOME=/opt/android/android-ndk-r27c
cargo ndk -t arm64-v8a build --release -p plantuml-little \
  --no-default-features --features metrics-ffi-callback
# → graphviz-anywhere build.rs panic
```

## 阻塞 #2 — iOS 跨编 (任意 engine) 完全未跑

### 阻塞条件

iOS 跨编需要 macOS + Xcode：

- Xcode Command Line Tools（`xcrun` / `lipo` / `xcodebuild`）
- iOS SDK（`xcrun --show-sdk-path -sdk iphoneos`）
- Rust targets：`aarch64-apple-ios`（真机）+ `aarch64-apple-ios-sim`（M 系列 Mac 模拟器）+ `x86_64-apple-ios`（Intel Mac 模拟器）

Linux 本机**完全做不了**这步（缺 Apple SDK，且 Apple ToS 不允许 Linux 跨编 iOS target）。

### Mac 上的步骤

```bash
# 1. 装 Rust iOS targets
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

# 2. 验证 Xcode 在位
xcodebuild -version  # 需要 ≥ 15.0
xcrun --show-sdk-path -sdk iphoneos

# 3. 对每个 native crate 跨编 3 target
cd /path/to/supramark
for target in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
  cargo build --release --target $target \
    -p supramark-d2-native -p supramark-mermaid-native
done

# 4. 用 lipo 合并 simulator 的 arm64 + x86_64 成 fat lib
mkdir -p target/ios-sim-universal/release
lipo -create \
  target/aarch64-apple-ios-sim/release/libsupramark_d2_native.a \
  target/x86_64-apple-ios/release/libsupramark_d2_native.a \
  -output target/ios-sim-universal/release/libsupramark_d2_native.a

# 5. 用 xcodebuild -create-xcframework 组装 .xcframework
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libsupramark_d2_native.a \
  -headers crates/d2-little/packages/native/include \
  -library target/ios-sim-universal/release/libsupramark_d2_native.a \
  -headers crates/d2-little/packages/native/include \
  -output target/ios/SupramarkD2.xcframework

# 6. 同样流程跑 mermaid / plantuml (plantuml 同样卡 graphviz-anywhere iOS prebuilt，
# graphviz-anywhere repo 自带 ios.tar.gz 是否真二进制需 Mac 上验证)
```

### 注意：plantuml iOS 也可能有 graphviz-anywhere prebuilt 问题

`crates/graphviz-anywhere/packages/react-native/graphviz-native-ios.tar.gz` 同样要先确认是不是 LFS 占位（Linux 上看是空 ASCII）。Mac 上如果 LFS 拉得下来真二进制就 OK；否则要自己用 Xcode 跨编 libgraphviz_api iOS 版。

## 阻塞 #3 — RN turbomodule wrapper npm 包不存在

每个 engine 还需要一个 RN native 包，例如 `@kookyleo/supramark-d2-native-rn`，承担：

1. 把 `libsupramark_d2_native.a` / `.so` 打进 iOS .xcframework + Android jniLibs 各 ABI
2. RN TurboModule / Old NativeModule wrapper：
   - iOS：Swift / ObjC 调 `supramark_d2_render` C 函数 → 转 JS Promise
   - Android：JNI 调同上 → 转 Promise
3. JS entry 调 `@supramark/engines/rn` 的 `registerNativeEngineAdapter({ engine: 'd2', render })` 在 module 加载时自动注册

参考已有的 `@kookyleo/graphviz-anywhere-rn`（`crates/graphviz-anywhere/packages/react-native/`）作模板。每个 engine 大概 200-400 LOC 包装 + per-platform 二进制。

## 待办优先级（换机器后建议路径）

如果换的是 **Mac**：
1. iOS 跨编 d2 + mermaid + 装 .xcframework （1-2 小时）
2. 顺便复用同机器装 Android NDK + 再跑一遍 Android（验证 reproducibility）
3. 开始 RN turbomodule wrapper 第一个（d2 或 mermaid）

如果换的是另一台 **Linux**（NDK 更新或者更强机器）：
1. 解阻塞 #1（推荐方案 B — plantuml graphviz optional feature，30-60 分钟）
2. 重跑 plantuml 4 个 Android ABI
3. iOS 部分仍要等 Mac

## 已 push 的 commit 链（origin/main）

```
69278d8e feat(engines): wire RN native engine adapter registry for d2/mermaid/plantuml
57844d99 feat(mermaid-little): add native FFI wrapper for iOS / Android / RN
c8beb66e feat(plantuml-little): add native FFI wrapper for iOS / Android / RN
83ee6d5a feat(d2-little): add native FFI wrapper for iOS / Android / RN
8ac5a028 feat(font-metrics): implement metrics-ffi-callback for native (RN) host bridge
349bab63 refactor(web): drop SSR entry — focus on client-side rendering only
```

## 已生成产物（不在 git 里，需重新生成）

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
