# Cross-target work — progress + handoff notes

This file is the long-form companion to `CHANGELOG.md`. It captures
**how** the work was done, **what is verified locally vs. what is
deferred to CI**, and the open work for the next session.

The goal is that another contributor (or a fresh AI session) can pick up
mid-stream without losing context.

---

## Phase 1 — Rust `build.rs` cross-target coverage ✅ shipped

**State**: merged to `main` (commit `9856093`), pushed to `origin/main`
on 2026-05-11.

The four parallel work-streams below all landed in `main` in a single
session and were reconciled against each other. Each is on a merge
commit; the individual feature commits are preserved underneath.

| Branch (merged) | Files | Commit |
| --- | --- | --- |
| `feat/build-rs-coverage` | `packages/rust/build.rs`, `packages/rust/src/build_helpers.rs` (new), `packages/rust/tests/build_helpers_tests.rs` (new), `packages/rust/Cargo.toml` (include list) | `8a4fba4` |
| `feat/native-build-coverage` | `scripts/build-ios.sh`, `scripts/build-android.sh`, `scripts/build-linux.sh`, `scripts/build-windows.sh` | `3002563` + `f077c4b` (libname fix) |
| `ci/expand-target-matrix` | `.github/workflows/build.yml` | `9733214` + `dcb5899` (iOS dir-name fix) |
| `docs/honest-status-and-cross-compile` | `README.md`, `docs/cross-compile.md` (new), `packages/react-native/ios/GraphvizNative.podspec`, `packages/react-native/android/build.gradle` | `5462760` + `515d49d` (iOS asset-name fix) |

### Reconciliation notes

Three cross-stream inconsistencies were caught and fixed before merging:

1. **iOS library filename**: `build-ios.sh` historically produced
   `libGraphviz.a` (capital G). Every other platform produces
   `libgraphviz_api.{a,so,dylib}`, and `cargo:rustc-link-lib=static=graphviz_api`
   expects exactly that. Fixed in `f077c4b` — renamed iOS internally.
2. **iOS install layout**: the script only produced an XCFramework, no
   per-slice `output/ios/<slice>/{lib,include}/` install. CI's per-slice
   tarball packaging needed this. Fixed in `f077c4b` — each `build_ios_arch`
   call now also installs to `${INSTALL_DIR}/${sdk}-${arch}/{lib,include}/`.
3. **iOS slice directory naming**: CI step had assumed `device-arm64` /
   `sim-arm64` / `sim-x86_64`; actual `build-ios.sh` produces
   `iphoneos-arm64` / `iphonesimulator-arm64` / `iphonesimulator-x86_64`.
   Fixed in `dcb5899` — CI `cd` paths use the SDK-keyed dir names; the
   consumer-facing tarball names stay as `-device-arm64` / `-sim-arm64` /
   `-sim-x86_64` for legibility. Same convention is documented in
   `docs/cross-compile.md`.

### Verified in this environment (macOS 26.3 + Xcode 26.4 + Rust 1.95)

- ✅ `cargo test -p graphviz-anywhere --tests` — 31 lib tests + 22 new
  helper tests = **53 passing**
- ✅ `GRAPHVIZ_ANYWHERE_NO_DOWNLOAD=1 cargo check --target X` for
  `X ∈ { aarch64-apple-darwin, aarch64-apple-ios, aarch64-apple-ios-sim,
  x86_64-apple-ios }` — each panics with the new descriptive message
  naming the correct `prebuilt/<triple>/` path, `output/...` path, and
  GitHub Release asset name. Cross-stream consistency confirmed
  (B's install dirs ↔ A's resolver ↔ C's CI packaging ↔ D's docs all
  agree).

### Not yet verified locally (deferred to CI or next session)

| Target | Why it's deferred | When to retest |
| --- | --- | --- |
| `aarch64-linux-android` and 3 siblings | Android NDK not installed locally yet | After `brew install --cask android-ndk` |
| `aarch64-unknown-linux-gnu` | Needs Docker + `cross` for cross-build from macOS | After `brew install colima docker && cargo install cross && colima start` |
| `x86_64-unknown-linux-gnu` | Same as above (use ubuntu image in Docker) | After Docker setup |
| `x86_64-pc-windows-msvc` and ARM64 | Need Windows host | CI only (`windows-latest` + `continue-on-error` for ARM64) |
| `wasm32-unknown-unknown` JS bridge | Doesn't need native lib; emsdk presence not re-checked | Quick `cargo build --target wasm32-unknown-unknown` once |

### Open follow-ups (post-0.2.0)

- Linux aarch64 CI leg currently uses `ubuntu-latest` (x86) with
  `continue-on-error: true`. Switch to `ubuntu-24.04-arm` once the org
  enables it. See `.github/workflows/build.yml` line ~30.
- Windows ARM64 cross-build (`scripts/build-windows.sh --arch arm64` +
  CI leg) is a skeleton — `# TODO(verify-in-ci)` markers in place.
- macOS Apple Silicon native host build was not re-verified after the
  `try_prebuilt` refactor. Convention is preserved (legacy
  `prebuilt/macos/` still read), but a clean run is worth doing once.

---

## Phase 2 — Local verification of NDK + Docker targets (in progress)

Plan: bring the "deferred to CI" rows in the table above into "verified
locally" by installing tools the macOS host can run.

### Tools to install

```bash
brew install --cask android-ndk    # ~1.5 GB, ~5 min
brew install colima docker docker-buildx
cargo install cargo-ndk cross
```

### After install

```bash
# Android NDK env
export ANDROID_NDK_HOME=$(brew --prefix android-ndk)/share/android-ndk
# (path may differ — `brew info --cask android-ndk` shows the symlink)

# Docker daemon (Colima, no GUI)
colima start --cpu 4 --memory 8

# Init graphviz submodule (needed for any actual native build)
cd /Users/leo/Starbucks/graphviz-anywhere
git submodule update --init --recursive graphviz
```

### Verification recipe (one target at a time)

**Android x86 (the new ABI added in 0.2.0)**:

```bash
cd /Users/leo/Starbucks/graphviz-anywhere
scripts/build-android.sh --abi x86
ls -la output/android/x86/lib/libgraphviz_api.so

# Then Rust crate resolves it via try_repo_output
cargo ndk -t x86 build --release -p graphviz-anywhere
```

**Linux aarch64**:

```bash
# scripts/build-linux.sh inside ubuntu-arm container,
# OR use cross which handles the container automatically:
cross build --release --target aarch64-unknown-linux-gnu -p graphviz-anywhere
# cross will need GRAPHVIZ_ANYWHERE_DIR pointing at an aarch64 .so —
# easiest path: run scripts/build-linux.sh --arch aarch64 inside the
# cross container first to produce output/linux-aarch64/lib/libgraphviz_api.so
```

### Skipped on macOS (intentionally — handled by CI)

- `x86_64-pc-windows-msvc` and `aarch64-pc-windows-msvc` — Windows host
  required. CI uses `windows-latest`; ARM64 leg has `continue-on-error:
  true` until a real Windows ARM runner is wired in.

---

## Phase 3 — Wave 2 candidates (not started)

Captured here so they don't get forgotten:

1. **Crate split** — separate `graphviz-anywhere-sys` (FFI + `build.rs`)
   from `graphviz-anywhere` (safe wrapper). Cleaner for other-language
   bindings to depend on `-sys` directly.
2. **`vendored-prebuilt` Cargo feature** — for downstream crates that
   want zero-network-setup builds. The feature pulls a small set of
   committed `prebuilt/<triple>/libgraphviz_api.a` blobs (Mac universal
   + linux x86_64 + linux aarch64; ~30 MB total — within crates.io
   limits if compressed).
3. **Matrix expansion (musl + WASI)**:
   - `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl` — Alpine
     / scratch containers
   - `wasm32-wasip1` — WASI runtimes (Wasmtime, Wasmer, Spin)
   - Both feasible locally via Docker / wasi-sdk

---

## Phase 4 — supramark subtree sync (not started)

After 0.2.0 ships on `kookyleo/graphviz-anywhere`:

1. Pull updated subtree into supramark: `git subtree pull --prefix
   crates/graphviz-anywhere https://github.com/kookyleo/graphviz-anywhere
   main`
2. Update `crates/graphviz-anywhere/UPSTREAM.md` pin (currently
   `436fe2f00bf099416a3e6eea6d1012911d4f7435` / `v0.1.7` — bump to
   v0.2.0 SHA after release).
3. Verify `plantuml-little` cross-compiles end-to-end for iOS and
   Android: `cargo build --target aarch64-apple-ios -p plantuml-little`
   should now succeed instead of panicking on `graphviz-anywhere`'s
   missing iOS branch.
4. If `plantuml-little`'s graphviz dependency is still hard-required
   (no `optional` feature), consider Wave 2 task 1 from
   `supramark/docs/architecture/native-ffi-blockers.md` — refactor
   `plantuml-little`'s Cargo.toml to make `graphviz-anywhere` an
   optional feature. That's a 30-60min change in the supramark repo,
   separate from this work.
