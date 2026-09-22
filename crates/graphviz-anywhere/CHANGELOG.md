# Changelog

All notable changes to this project are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.6] — 2026-09-22

### Fixed

- **`cargo install` of downstream binaries** — `build.rs` now downloads the
  matching GitHub release asset by default when no local library is found.
  The published crate carries no native library, so the previous opt-in
  (`GRAPHVIZ_ANYWHERE_ALLOW_DOWNLOAD=1`) made every plain crates.io install
  fail. Set `GRAPHVIZ_ANYWHERE_NO_DOWNLOAD=1` to keep builds offline;
  `GRAPHVIZ_ANYWHERE_ALLOW_DOWNLOAD` is still accepted and is now a no-op.
- **Linux linking without -dev packages** — the static archive's system
  dependencies are linked by SONAME (`libstdc++.so.6`, `libexpat.so.1`,
  `libz.so.1`), so hosts with only the runtime packages installed (no
  `libexpat1-dev` / `zlib1g-dev` / `g++`) no longer fail with
  `library not found: expat`. MSRV is now 1.67 for the `+verbatim` link
  modifier.

## [0.2.5] — 2026-07-13

### Fixed

- **Visual Studio image compatibility** — the Windows native build now selects
  the installed VS 2019, 2022, or 2026 CMake generator via `vswhere` instead of
  hard-coding VS 2022. This keeps release builds working as GitHub advances the
  `windows-latest` image while retaining explicit `CMAKE_GENERATOR` overrides.
- **Independent Windows diagnostics** — x64 and ARM64 release jobs no longer
  cancel each other on the first matrix failure.
- **Native ARM64 dependency boundary** — ARM64 builds explicitly ignore the
  Graphviz source tree's bundled x64 GD/Cairo/Pango libraries and build only
  the core layout/SVG surface used by the Rust wrapper.
- **Git Bash packaging** — release archives use a POSIX-converted workspace
  path so GNU tar does not interpret a Windows drive prefix as a remote host.

## [0.2.4] — 2026-07-12

### Fixed

- **Self-contained desktop executables** — Linux and macOS release downloads
  now link the static archive that was already shipped beside the shared
  library. Final Cargo binaries no longer depend on `libgraphviz_api.so` or
  `libgraphviz_api.dylib`, so downstream build-script rpaths cannot disappear
  and a same-named system library cannot satisfy the load with an incompatible
  ABI.
- **Windows env override** — source output contains both the DLL import library
  and `graphviz_api_static.lib`. The resolver now selects the merged static
  archive first and accepts canonical `graphviz_api.lib` as static only when no
  sibling DLL identifies it as an import library.
- **ABI-safe target resolution** — GNU-built Linux assets are no longer
  auto-selected for musl targets, and MSVC `.lib` assets are no longer selected
  for `windows-gnu`. These targets now require an explicit compatible native
  build instead of failing later with opaque linker or loader errors.
- **Runtime smoke coverage** — CI executes a real DOT-to-SVG example from the
  downloaded release assets on Linux, macOS, and Windows and asserts desktop
  executables have no Graphviz shared-library dependency.
- **Release isolation** — the native/Cargo release no longer attempts to
  publish independently-versioned web and React Native npm packages. A Rust
  patch release therefore cannot fail midway or accidentally consume an npm
  version before the crates.io publish step.

## [0.2.1] — 2026-05-11

### Fixed

- **Windows static library merge** — `graphviz_api_static.lib` previously
  contained only the C wrapper (~19 KB) because the CMake `STATIC` target's
  `target_link_options(... LINKER:/WHOLEARCHIVE)` is a link-time directive
  for executables/DLLs, silently ignored when producing a static archive.
  Downstream Rust crates that static-linked on Windows hit hundreds of
  unresolved Graphviz symbols. Now invokes `lib.exe` directly to merge all
  27 Graphviz component `.lib` archives + the wrapper `.obj` into a
  ~7 MB merged static library that actually contains the Graphviz code.
- **`publish-npm-cargo` gate** — was `if: startsWith(refs/tags/)` so
  `test*` tags accidentally published 0.2.0 to npm + crates.io. Tightened
  to `refs/tags/v` so only real release tags publish.

### Note on 0.2.0

The 0.2.0 npm packages and crates.io crate were released via a `test-*`
tag during pipeline verification. They are functionally identical to 0.2.1
on all platforms **except** Windows static-link consumers, who must use
0.2.1+. Shared-link consumers (DLL via import library) are unaffected
either way.

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
  auto-resolves; the bundled form remains for `@actrium/graphviz-anywhere-rn`'s
  postinstall script.
- **Windows ARM64**: skeleton (`scripts/build-windows.sh --arch arm64`, CI
  matrix entry with `continue-on-error: true`, asset name
  `graphviz-native-windows-arm64.zip`, `build.rs` env-override path). Needs
  real verification on a Windows ARM runner.
- **RN postinstall paths in `try_repo_output`**: `build.rs` now scans
  `packages/react-native/{ios,android}/...` when the Rust crate is built
  inside the same monorepo as `@actrium/graphviz-anywhere-rn`.
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

[Unreleased]: https://github.com/Actrium/supramark/compare/v0.2.6...HEAD
[0.2.6]: https://github.com/Actrium/supramark/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/Actrium/supramark/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/Actrium/supramark/compare/v0.2.3...v0.2.4
[0.2.1]: https://github.com/Actrium/graphviz-anywhere/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Actrium/graphviz-anywhere/compare/v0.1.8...v0.2.0
[0.1.8]: https://github.com/Actrium/graphviz-anywhere/releases/tag/v0.1.8
[0.1.7]: https://github.com/Actrium/graphviz-anywhere/releases/tag/v0.1.7
[0.1.6]: https://github.com/Actrium/graphviz-anywhere/releases/tag/v0.1.6
[0.1.5]: https://github.com/Actrium/graphviz-anywhere/releases/tag/v0.1.5
