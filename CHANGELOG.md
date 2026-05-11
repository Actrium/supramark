# Changelog

All notable changes to this project are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] — 2026-05-11

Target version: **0.2.0** (cross-target build.rs + asset coverage)

### Added

- **Rust `build.rs` cross-target coverage** — every Rust `--target` the project
  ships for now has a deterministic resolution path through `try_env_override`
  → `try_prebuilt` → `try_repo_output` → `try_github_release` (or a descriptive
  panic naming the expected paths + asset name + fix options).
- **iOS targets**: `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios`
  fully wired into `build.rs` (previously not recognized at all).
- **Linux aarch64**: `aarch64-unknown-linux-gnu` GitHub Release asset
  (`graphviz-native-linux-aarch64.tar.gz`) + CI matrix entry.
- **Android x86 (`i686-linux-android`)**: 4th Android ABI added to
  `scripts/build-android.sh`, CI matrix, and Release asset
  (`graphviz-native-android-x86.tar.gz`).
- **iOS simulator x86_64 slice**: third slice added to `scripts/build-ios.sh`
  for Intel Mac development. Per-slice install layout
  (`output/ios/<sdk>-<arch>/{lib,include}/`) lets CI package per-slice tarballs
  in addition to the bundled XCFramework.
- **Per-slice iOS Release assets**: `graphviz-native-ios-{device-arm64,
  sim-arm64,sim-x86_64}.tar.gz` alongside the legacy bundled
  `graphviz-native-ios.tar.gz`. The per-slice form is what `build.rs`
  auto-resolves; the bundled form remains for `@kookyleo/graphviz-anywhere-rn`'s
  postinstall script.
- **Windows ARM64**: skeleton (`scripts/build-windows.sh --arch arm64`, CI
  matrix entry with `continue-on-error: true`, asset name
  `graphviz-native-windows-arm64.zip`, `build.rs` env-override path). Needs
  real verification on a Windows ARM runner.
- **RN postinstall paths in `try_repo_output`**: `build.rs` now scans
  `packages/react-native/{ios,android}/...` when the Rust crate is built
  inside the same monorepo as `@kookyleo/graphviz-anywhere-rn`.
- **Target-triple `prebuilt/` layout**: new entries use
  `prebuilt/<rust-target-triple>/libgraphviz_api.{a,lib}`. Legacy per-host-OS
  paths (`prebuilt/{macos,linux,windows}/`) are kept as read-only fallback.
- **`docs/cross-compile.md`**: 14-row per-target guide covering toolchain,
  build command, output path, Release asset, env override, common errors,
  RN-vs-Rust postinstall divergence, airgapped builds.
- **22 new unit tests** in `packages/rust/tests/build_helpers_tests.rs`
  covering every target-triple → asset-name / prebuilt-subdir / output-dirs
  mapping. Combined with the existing 31 lib tests = 53 tests passing.

### Changed

- **iOS library name** unified across platforms: was `libGraphviz.a`,
  now `libgraphviz_api.a` (matches what `linux`/`macos`/`android` scripts
  produce, and what `cargo:rustc-link-lib=static=graphviz_api` looks for).
- **iOS deployment target** aligned at **15.1** across `scripts/build-ios.sh`
  (`IOS_MIN_VERSION`) and `packages/react-native/ios/GraphvizNative.podspec`
  (`s.ios.deployment_target`). Previously 12.0 vs 15.1 mismatch.
- **`try_prebuilt`** now reads `CARGO_CFG_TARGET_OS` / `TARGET` instead of
  `cfg!(target_os = ...)`, so cross-compile resolution is consistent with
  the rest of `build.rs`.
- **`build.rs` panic message** now prints detected `TARGET`, expected
  `prebuilt/...` path, expected `output/...` path, expected GitHub Release
  asset name, and four labeled fix options.
- **CI matrices** expanded: Linux `[x86_64, aarch64]`, Android
  `[arm64-v8a, armeabi-v7a, x86_64, x86]`, iOS three slices with per-slice
  tarball packaging, Windows `[x86_64, arm64]`.
- **CI prebuilt copy step**: desktop artifacts are now also copied to
  `prebuilt/<target-triple>/` (new layout) alongside legacy
  `prebuilt/{macos,linux,windows}/`.
- **`build-android.sh`**: default ABI list expanded from 3 to 4 (adds `x86`).
- **`README.md` Native Target Status section**: replaced the
  over-promising "Native builds target …" line with a 14-row matrix listing
  per-target coverage of scripts / CI / Release asset / `build.rs`
  auto-resolve.

### Known Limitations / Not Yet Verified

- Windows ARM64 cross-build path is a skeleton — needs a `windows-11-arm`
  runner or a confirmed `vcvarsall x64_arm64` setup.
- Linux aarch64 CI leg currently uses `ubuntu-latest` with
  `continue-on-error: true`; switch to `ubuntu-24.04-arm` once enabled at
  the org level.
- macOS Apple Silicon native host build not re-verified after this refactor.
  Convention is preserved (legacy `prebuilt/macos/` still read), but a clean
  reinstall on an arm64 Mac is worth running once.

## [0.1.8] — 2026-05-09

(Pre-existing entries, captured retroactively from `git log`.)

### Added

- `try_github_release` in `build.rs`: auto-download prebuilt library from the
  matching GitHub Release when no local artifact is found (#598dc33).
- Android target support in `try_repo_output` and `try_github_release` —
  3 ABIs auto-resolved (#8b9ef81).

## [0.1.7]

- `wasm`: enable `libexpat` for HTML labels (#7, #2c88260).

## [0.1.6]

- `build`: link Linux `libgraphviz_api.so` with `g++` (#6, #52e8141).

## [0.1.5]

- `build`: enable `WITH_EXPAT` + `WITH_ZLIB` on macos/linux (#5, #88b9535).

## [0.1.3] and earlier

- Initial wasm32 bridge; size_t overflow fix in capi; web embind TypeScript
  glue cleanups.

---

[Unreleased]: https://github.com/kookyleo/graphviz-anywhere/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/kookyleo/graphviz-anywhere/compare/v0.1.8...v0.2.0
[0.1.8]: https://github.com/kookyleo/graphviz-anywhere/releases/tag/v0.1.8
[0.1.7]: https://github.com/kookyleo/graphviz-anywhere/releases/tag/v0.1.7
[0.1.6]: https://github.com/kookyleo/graphviz-anywhere/releases/tag/v0.1.6
[0.1.5]: https://github.com/kookyleo/graphviz-anywhere/releases/tag/v0.1.5
