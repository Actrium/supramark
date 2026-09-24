# Cross-Compilation Guide

This guide covers building or consuming `graphviz-anywhere` native libraries
for each supported target triple.

## How resolution works

`packages/rust/build.rs` tries these steps in order:

1. `GRAPHVIZ_ANYWHERE_DIR` / `GRAPHVIZ_NATIVE_DIR` env override
2. `packages/rust/prebuilt/<os>/` static lib (populated by CI)
3. Sibling `output/<platform>/lib/` (local build tree)
4. Auto-download from GitHub Release (`curl` + `tar`)

Linux (glibc and musl), macOS, Windows MSVC, and iOS resolve a static archive;
Android resolves its package-staged shared library. Each ABI gets its own
asset: musl has separate `graphviz-native-linux-musl-*` archives rather than
reusing the glibc ones, and Windows GNU has no asset at all, because those ABIs
are not interchangeable.

Set `GRAPHVIZ_ANYWHERE_NO_DOWNLOAD=1` to make step 4 a hard error (useful in
CI or airgapped environments). Override the release tag with
`GRAPHVIZ_ANYWHERE_RELEASE_VERSION=<version>`.

---

## x86_64-unknown-linux-gnu

- **Toolchain**: `gcc` / `clang`, CMake 3.16+, `bison`, `flex`, `pkg-config`
- **Build**: `./scripts/build-linux.sh --arch x86_64`
- **Output**: `output/linux-x86_64/lib/libgraphviz_api.a`
- **Prebuilt path**: `packages/rust/prebuilt/linux/libgraphviz_api.a`
- **Release asset**: `graphviz-native-linux-x86_64.tar.gz`
- **Override**: `GRAPHVIZ_ANYWHERE_DIR=output/linux-x86_64 cargo build`
- **Common errors**: missing `libexpat-dev` / `libpango1.0-dev` → `apt-get install libexpat1-dev libpango1.0-dev libharfbuzz-dev`

## aarch64-unknown-linux-gnu

- **Toolchain**: `aarch64-linux-gnu-gcc`, CMake cross-file or `ARCH=aarch64`
- **Build**: `./scripts/build-linux.sh --arch aarch64`
- **Output**: `output/linux-aarch64/lib/libgraphviz_api.a`
- **Release asset**: `graphviz-native-linux-aarch64.tar.gz` (CI uses the native `ubuntu-22.04-arm` runner)
- **Override**: `GRAPHVIZ_ANYWHERE_DIR=output/linux-aarch64 cargo build --target aarch64-unknown-linux-gnu`
- **build.rs auto-resolve**: ✅
- **Common errors**: cross-linker not on PATH → `apt-get install gcc-aarch64-linux-gnu`

## x86_64-unknown-linux-musl / aarch64-unknown-linux-musl

- **Toolchain**: a musl toolchain, i.e. build inside Alpine —
  `apk add build-base cmake bison flex python3 pkgconf expat-dev expat-static zlib-dev zlib-static`
  (python3 is Graphviz's own build requirement, preinstalled on the glibc CI
  image but absent from a minimal Alpine).
  The script checks `cc -dumpmachine` and refuses to run on a glibc toolchain,
  so a glibc archive cannot be packaged as a musl asset by mistake.
- **Build**: `./scripts/build-linux.sh --arch x86_64 --libc musl`
- **Output**: `output/linux-musl-<arch>/lib/libgraphviz_api.a`
- **Release asset**: `graphviz-native-linux-musl-<arch>.tar.gz`
- **Override**: `GRAPHVIZ_ANYWHERE_DIR=output/linux-musl-x86_64 cargo build --target x86_64-unknown-linux-musl`
- **build.rs auto-resolve**: ✅, and matched by triple *suffix*. Alpine's distro
  rustc reports its host as `x86_64-alpine-linux-musl`, not rustup's
  `x86_64-unknown-linux-musl`, so both spellings (and any other vendor field)
  resolve to the same asset and to the canonical
  `prebuilt/x86_64-unknown-linux-musl/` directory.
- **Self-contained archive**: unlike the glibc asset, the musl archive has
  `libstdc++`, `libexpat` and `libz` merged into it, and `build.rs` emits no
  system-library link flags for musl targets. A musl host has neither the
  runtime SONAMEs the glibc path names nor the static packages, and rustc links
  these targets `crt-static`; anything left unresolved in the archive would
  break every downstream `cargo install`. Both link modes work (Alpine's own
  rust links musl dynamically, an upstream rustup toolchain links static).
- **Common errors**: `--libc musl needs a musl toolchain` → you are on glibc;
  run it in Alpine. `musl build needs a static libstdc++.a` → `apk add g++
  expat-static zlib-static`.

## aarch64-apple-darwin / x86_64-apple-darwin (macOS universal)

- **Toolchain**: Xcode 14+, `bison`/`flex` from Homebrew
- **Build**: `./scripts/build-macos.sh --arch universal`
- **Output**: `output/macos-universal/lib/libgraphviz_api.a`
- **Prebuilt path**: `packages/rust/prebuilt/macos/libgraphviz_api.a`
- **Release asset**: `graphviz-native-macos-universal.tar.gz`
- **Override**: `GRAPHVIZ_ANYWHERE_DIR=output/macos-universal cargo build`
- **Common errors**: system `bison` too old → `export PATH="$(brew --prefix bison)/bin:$PATH"`

## aarch64-apple-ios

- **Toolchain**: Xcode 15+, iOS SDK ≥ 15.1
- **Build**: `./scripts/build-ios.sh` (produces XCFramework + per-slice `.a`)
- **Output**: `output/ios/iphoneos-arm64/lib/libgraphviz_api.a` (and `include/graphviz_api.h`)
- **Release asset**: `graphviz-native-ios-device-arm64.tar.gz`
- **Override**: `GRAPHVIZ_ANYWHERE_DIR=output/ios/iphoneos-arm64 cargo build --target aarch64-apple-ios`
- **build.rs auto-resolve**: ✅
- **Common errors**: `ld: file not found` if pointing at the full XCFramework bundle — use the per-slice directory instead

## aarch64-apple-ios-sim

- **Toolchain**: Xcode 15+, iOS Simulator SDK ≥ 15.1
- **Build**: `./scripts/build-ios.sh` (simulator slice built automatically)
- **Output**: `output/ios/iphonesimulator-arm64/lib/libgraphviz_api.a`
- **Release asset**: `graphviz-native-ios-sim-arm64.tar.gz`
- **Override**: `GRAPHVIZ_ANYWHERE_DIR=output/ios/iphonesimulator-arm64 cargo build --target aarch64-apple-ios-sim`
- **build.rs auto-resolve**: ✅
- **Common errors**: wrong slice — simulator and device slices are separate; don't mix them

## x86_64-apple-ios (Intel simulator)

- **Toolchain**: Xcode 15+, iOS Simulator SDK ≥ 15.1 (any Mac host)
- **Build**: `./scripts/build-ios.sh` (third slice built automatically)
- **Output**: `output/ios/iphonesimulator-x86_64/lib/libgraphviz_api.a`
- **Release asset**: `graphviz-native-ios-sim-x86_64.tar.gz`
- **Override**: `GRAPHVIZ_ANYWHERE_DIR=output/ios/iphonesimulator-x86_64 cargo build --target x86_64-apple-ios`
- **build.rs auto-resolve**: ✅
- **Common errors**: `xcrun` cannot find SDK on arm64 Mac → set `-sdk iphonesimulator` explicitly in CMake flags

## aarch64-linux-android

- **Toolchain**: Android NDK r26+, CMake 3.22+
- **Build**: `./scripts/build-android.sh --abi arm64-v8a`
- **Output**: `output/android/arm64-v8a/lib/libgraphviz_api.so`
- **Release asset**: `graphviz-native-android-arm64-v8a.tar.gz`
- **Override**: `GRAPHVIZ_ANYWHERE_DIR=output/android/arm64-v8a cargo build --target aarch64-linux-android`
- **Common errors**: `ANDROID_NDK_HOME` not set → export it before running the script

## armv7-linux-androideabi

- **Toolchain**: Android NDK r26+
- **Build**: `./scripts/build-android.sh --abi armeabi-v7a`
- **Output**: `output/android/armeabi-v7a/lib/libgraphviz_api.so`
- **Release asset**: `graphviz-native-android-armeabi-v7a.tar.gz`
- **Override**: `GRAPHVIZ_ANYWHERE_DIR=output/android/armeabi-v7a cargo build --target armv7-linux-androideabi`

## x86_64-linux-android

- **Toolchain**: Android NDK r26+
- **Build**: `./scripts/build-android.sh --abi x86_64`
- **Output**: `output/android/x86_64/lib/libgraphviz_api.so`
- **Release asset**: `graphviz-native-android-x86_64.tar.gz`
- **Override**: `GRAPHVIZ_ANYWHERE_DIR=output/android/x86_64 cargo build --target x86_64-linux-android`

## i686-linux-android (x86 emulator)

- **Toolchain**: Android NDK r26+
- **Build**: `./scripts/build-android.sh --abi x86`
- **Output**: `output/android/x86/lib/libgraphviz_api.so`
- **Release asset**: `graphviz-native-android-x86.tar.gz`
- **Override**: `GRAPHVIZ_ANYWHERE_DIR=output/android/x86 cargo build --target i686-linux-android`
- **build.rs auto-resolve**: ✅
- **Common errors**: emulator only; do not ship to production without `arm64-v8a` as primary ABI

## x86_64-pc-windows-msvc

- **Toolchain**: MSVC 2019 / 2022 / 2026, CMake, `bison`/`flex`
  (winflexbison). The build selects the installed Visual Studio generator via
  `vswhere`; set `CMAKE_GENERATOR` only when an explicit override is required.
- **Build**: `./scripts/build-windows.sh`
- **Output**: `output/windows-x86_64/lib/graphviz_api_static.lib`; the release
  renames the merged static archive to canonical `lib/graphviz_api.lib`
- **Release asset**: `graphviz-native-windows-x86_64.tar.gz`
- **Override**: `$env:GRAPHVIZ_ANYWHERE_DIR = "output\windows-x86_64"; cargo build`
- **build.rs**: env override works; auto-download extracts the `.tar.gz` release asset
- **Common errors**: do not point the override at the DLL import library; use
  `graphviz_api_static.lib`, or a release directory whose canonical
  `graphviz_api.lib` is the merged static archive

## aarch64-pc-windows-msvc

- **Toolchain**: native Windows ARM64 runner with the MSVC ARM64 build tools
- **Build**: `./scripts/build-windows.sh --arch arm64`
- **Output**: `output/windows-arm64/lib/graphviz_api_static.lib`; the release
  renames it to canonical `lib/graphviz_api.lib`
- **Release asset**: `graphviz-native-windows-arm64.tar.gz`
- **Override**: `$env:GRAPHVIZ_ANYWHERE_DIR = "output\windows-arm64"; cargo build --target aarch64-pc-windows-msvc`
- **build.rs**: env override and release-asset fallback are both supported
- **Common errors**: the Visual Studio installation must include the ARM64 C++
  build tools; x64-only installations cannot produce this target

## wasm32-unknown-unknown

- **No native linking needed.** The Rust crate detects `target_arch == "wasm32"` in build.rs and exits early.
- **Delivery**: bundled in the `@actrium/graphviz-anywhere-web` npm package
- **Build**: `./scripts/build-wasm.sh` → `packages/web/dist/{viz.js,viz.wasm}`

---

## RN integration

The `@actrium/graphviz-anywhere-rn` postinstall script downloads a prebuilt
native library into `packages/react-native/ios/Frameworks/` and the Android
JNI libs. This is **separate** from what `build.rs` does.

`build.rs` scans the Android RN JNI staging paths when the Rust crate is built
inside the same monorepo. Desktop Rust binaries deliberately do not consume RN
framework shared libraries; they keep the static, self-contained contract.

If you want to point at a custom location, the env override always wins:

```bash
GRAPHVIZ_ANYWHERE_DIR=/path/to/lib-dir cargo build --target <triple>
```

---

## Sandboxed / airgapped builds

1. Download the required asset from the [GitHub Release](https://github.com/Actrium/supramark/releases) on a connected machine.
2. Extract and place the library where `build.rs` can find it — either:
   - Set `GRAPHVIZ_ANYWHERE_DIR=/path/to/extracted/dir`, or
   - Place `libgraphviz_api.a` (or `.lib` on Windows) under `packages/rust/prebuilt/<os>/`.
3. Set `GRAPHVIZ_ANYWHERE_NO_DOWNLOAD=1` to prevent any outbound network attempt.

```bash
export GRAPHVIZ_ANYWHERE_NO_DOWNLOAD=1
export GRAPHVIZ_ANYWHERE_DIR=/mnt/prebuilts/graphviz-linux-x86_64
cargo build
```

If the build still tries to reach the network, check that no earlier
`GRAPHVIZ_ANYWHERE_RELEASE_VERSION` in your environment is triggering a
version mismatch path.
