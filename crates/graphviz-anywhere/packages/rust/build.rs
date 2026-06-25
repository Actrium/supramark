use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

// Pull in the pure mapping helpers so they are available both here (at build
// time) and in `src/build_helpers.rs` (for unit-tests compiled as part of the
// crate).  We use `include!` rather than a re-export so that build.rs does not
// need to declare a module hierarchy.
include!("src/build_helpers.rs");

fn emit_search_path(dir: &Path, dynamic: bool) {
    println!("cargo:rustc-link-search=native={}", dir.display());

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if dynamic && target_os != "windows" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
    }
}

/// System libraries a *static* `graphviz_api` archive depends on.
///
/// The unified `.so`/`.dylib` embeds these via `--whole-archive`, but a static
/// link must pull them in at the final binary. Emitting them as
/// `rustc-link-lib` (which Cargo *does* propagate to transitive dependents,
/// unlike an `-rpath` link-arg) lets downstream test binaries link a static
/// graphviz with no runtime dependency on — and no rpath to — a shared lib in
/// the build-output directory.
/// Link a static archive, isolating it in `OUT_DIR` first.
///
/// On macOS the Apple linker has no `-Bstatic`, so rust cannot force a
/// name-based `-l` to pick the `.a` over a sibling `.dylib` with the same name
/// in the same search directory — `ld` then prefers the `.dylib` and the binary
/// ends up with an `@rpath` load command that fails at runtime. Copying the
/// archive into `OUT_DIR` (which holds no competing dylib) and pointing the
/// search path there forces a genuine static link. The emitted
/// `rustc-link-search` / `rustc-link-lib` propagate to downstream binaries
/// (unlike an `-rpath` link-arg), so transitive test binaries link it too.
fn emit_static_link(static_lib: &Path, link_name: &str) {
    if let (Some(out_dir), Some(file_name)) = (
        env::var_os("OUT_DIR").map(PathBuf::from),
        static_lib.file_name(),
    ) {
        let staged = out_dir.join(file_name);
        if std::fs::copy(static_lib, &staged).is_ok() {
            println!("cargo:rustc-link-search=native={}", out_dir.display());
            println!("cargo:rustc-link-lib=static={link_name}");
            return;
        }
    }
    // Fallback (e.g. OUT_DIR unset): link straight from the source directory.
    if let Some(dir) = static_lib.parent() {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
    println!("cargo:rustc-link-lib=static={link_name}");
}

fn emit_static_sys_libs(target_os: &str) {
    let libs: &[&str] = match target_os {
        // Graphviz 14.x ships C++ libraries (libstdc++), plus expat (HTML
        // labels), zlib and libm.
        "linux" => &["stdc++", "expat", "z", "m"],
        // Apple: libc++ for the C++ libs; expat + zlib live in the SDK. libm is
        // part of libSystem, so it needs no explicit flag.
        "macos" => &["c++", "expat", "z"],
        // build-windows.sh builds Graphviz with expat disabled and the MSVC C++
        // runtime is linked automatically; zlib is merged into the static lib.
        "windows" => &[],
        _ => &[],
    };
    for lib in libs {
        println!("cargo:rustc-link-lib=dylib={lib}");
    }
}

fn try_env_override() -> bool {
    let Some(dir) = ["GRAPHVIZ_ANYWHERE_DIR", "GRAPHVIZ_NATIVE_DIR"]
        .iter()
        .find_map(|name| env::var(name).ok())
    else {
        return false;
    };

    let base = PathBuf::from(dir);
    let mut found = false;

    for sub in ["lib", "lib64", "build", "."] {
        let lib_dir = base.join(sub);
        if lib_dir.exists() {
            emit_search_path(&lib_dir, true);
            found = true;
        }
    }

    if found {
        println!("cargo:rustc-link-lib=dylib=graphviz_api");
    }

    found
}

/// Try to link against a static lib already present in `packages/rust/prebuilt/`.
///
/// Search order:
///  1. `prebuilt/<rust-target-triple>/libgraphviz_api.{a,lib}` — the new,
///     triple-keyed layout that works for any cross-compile target.
///  2. `prebuilt/{macos,linux,windows}/libgraphviz_api.{a,lib}` — the legacy
///     per-host-OS layout, kept for backwards compatibility.  Only tried when
///     the **host** OS matches the **target** OS (i.e. a native compile), so
///     we never accidentally link a host library into a cross-compiled binary.
fn try_prebuilt(manifest_dir: &Path) -> bool {
    let target = env::var("TARGET").unwrap_or_default();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // 1. New triple-keyed layout.
    if let Some((subdir, lib_name)) = target_triple_to_prebuilt_subdir(&target) {
        let dir = manifest_dir.join("prebuilt").join(subdir);
        let lib = dir.join(lib_name);
        if lib.exists() {
            emit_static_link(&lib, "graphviz_api");
            emit_static_sys_libs(&target_os);
            return true;
        }
    }

    // 2. Legacy layout — only when host OS == target OS.
    let host_os = env::var("CARGO_CFG_TARGET_OS")
        .or_else(|_| env::var("HOST"))
        .unwrap_or_default();
    // HOST env is something like "aarch64-apple-darwin"; derive the OS word.
    let host_os_word = if host_os.contains("darwin") || host_os == "macos" {
        "macos"
    } else if host_os.contains("linux") || host_os == "linux" {
        "linux"
    } else if host_os.contains("windows") || host_os == "windows" {
        "windows"
    } else {
        ""
    };
    let target_os_word = target_os.as_str();

    // Match legacy folder name to target OS.
    let legacy_folder = match target_os_word {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        _ => return false,
    };

    // Only use the legacy path when host OS word matches target OS.
    let host_matches = host_os_word == legacy_folder
        || host_os_word.is_empty() /* conservative: allow if we can't detect */;

    if host_matches {
        let dir = manifest_dir.join("prebuilt").join(legacy_folder);
        let lib_name = if target_os_word == "windows" {
            "graphviz_api.lib"
        } else {
            "libgraphviz_api.a"
        };
        let lib = dir.join(lib_name);
        if lib.exists() {
            emit_static_link(&lib, "graphviz_api");
            emit_static_sys_libs(&target_os);
            return true;
        }
    }

    false
}

fn try_repo_output(manifest_dir: &Path) -> bool {
    let Some(repo_root) = manifest_dir.parent().and_then(Path::parent) else {
        return false;
    };

    let target = env::var("TARGET").unwrap_or_default();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    // Desktop targets (Linux/macOS/Windows) and iOS link the *static* archive,
    // pulling its system-library dependencies in via `emit_static_sys_libs`.
    // This keeps downstream test binaries free of an rpath to the output dir
    // (Cargo does not propagate `-rpath` link-args to dependents), which
    // otherwise makes `cargo test --workspace` fail to load the shared lib on
    // macOS. Android keeps the shared `.so`, loaded by the React-Native runtime.
    let prefer_static =
        is_ios_target(&target) || matches!(target_os.as_str(), "linux" | "macos" | "windows");

    // ── Collect candidate directories ────────────────────────────────────────

    let mut candidates: Vec<(PathBuf, bool /* is_static */)> = Vec::new();

    // Standard output/ dirs produced by CI/scripts.
    for rel in target_triple_to_output_dirs(&target) {
        candidates.push((repo_root.join(rel), prefer_static));
    }

    // RN staged paths — scripts/prepare-native.js copies the native libs under
    // packages/react-native/. Android is single-sourced into src/main/jniLibs/
    // (no separate libs/ copy); the .so carries no SONAME so a directory search
    // path resolves the `-lgraphviz_api` link.
    let rn_root = repo_root.join("packages").join("react-native");
    if is_ios_target(&target) {
        // iOS xcframework: device slice holds the static archive + header.
        candidates.push((
            rn_root
                .join("ios")
                .join("Frameworks")
                .join("Graphviz.xcframework")
                .join("ios-arm64"),
            true, // iOS always static
        ));
    } else if target_os == "android" {
        let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        let abi = match target_arch.as_str() {
            "aarch64" => "arm64-v8a",
            "arm" => "armeabi-v7a",
            "x86_64" => "x86_64",
            "x86" => "x86",
            _ => "",
        };
        if !abi.is_empty() {
            candidates.push((
                rn_root
                    .join("android")
                    .join("src")
                    .join("main")
                    .join("jniLibs")
                    .join(abi),
                false, // Android: .so preferred
            ));
        }
    } else if target_os == "macos" {
        candidates.push((rn_root.join("macos").join("Frameworks").join("lib"), false));
    }

    // ── Walk candidates ──────────────────────────────────────────────────────

    for (dir, want_static) in candidates {
        if !dir.exists() {
            continue;
        }

        if want_static {
            // Unix uses `libgraphviz_api.a`; Windows uses the lib.exe-merged
            // `graphviz_api_static.lib` (plain `graphviz_api.lib` is the import
            // library for the DLL).
            let (static_lib, link_name) = if target_os == "windows" {
                (dir.join("graphviz_api_static.lib"), "graphviz_api_static")
            } else {
                (dir.join("libgraphviz_api.a"), "graphviz_api")
            };
            if static_lib.exists() {
                emit_static_link(&static_lib, link_name);
                emit_static_sys_libs(&target_os);
                return true;
            }
        } else {
            // Dynamic desktop/mobile targets: link the self-contained shared
            // library. A static `.a` is intentionally NOT preferred here — it
            // cannot embed the C++ runtime (libstdc++) or expat, which the
            // unified .so already pulls in via --whole-archive; linking the .a
            // statically would leave those symbols undefined. (Static linking
            // is only used for iOS, handled by the want_static branch above.)
            let dylib = if target_os == "macos" {
                dir.join("libgraphviz_api.dylib")
            } else {
                dir.join("libgraphviz_api.so")
            };
            if dylib.exists() {
                emit_search_path(&dir, true);
                println!("cargo:rustc-link-lib=dylib=graphviz_api");
                return true;
            }
            // Also check the directory itself as a search path even without
            // inspecting individual files — keeps compatibility with the
            // prior behaviour for Windows .lib files.
            if target_os == "windows" {
                let lib_file = dir.join("graphviz_api.lib");
                if lib_file.exists() {
                    emit_search_path(&dir, false);
                    println!("cargo:rustc-link-lib=dylib=graphviz_api");
                    return true;
                }
            }
        }
    }

    false
}

/// Last-resort, opt-in fallback: download the prebuilt library matching this
/// crate's version from a GitHub release and link against it.
///
/// graphviz-anywhere lives inside the `Actrium/supramark` monorepo and has no
/// standalone release feed yet, so this path is **disabled by default** — the
/// supported way to obtain the native library is a source build via
/// `scripts/build-<platform>.sh` (picked up by `try_repo_output`) or a prebuilt
/// drop-in under `packages/rust/prebuilt/`. Enable the download explicitly with
/// `GRAPHVIZ_ANYWHERE_ALLOW_DOWNLOAD=1` once a matching release is published.
///
/// Configuration:
///   * `GRAPHVIZ_ANYWHERE_ALLOW_DOWNLOAD=1` — opt in to the network fallback.
///   * `GRAPHVIZ_ANYWHERE_NO_DOWNLOAD=1`    — force it off (wins over allow).
///   * `GRAPHVIZ_ANYWHERE_RELEASE_BASE_URL` — override the release base URL
///       (default `https://github.com/Actrium/supramark/releases/download`).
///   * `GRAPHVIZ_ANYWHERE_RELEASE_VERSION`  — override the tag version
///       (defaults to CARGO_PKG_VERSION).
fn try_github_release() -> bool {
    // Hard-off always wins; otherwise require explicit opt-in because no
    // standalone release feed exists for this crate yet.
    if env::var_os("GRAPHVIZ_ANYWHERE_NO_DOWNLOAD").is_some() {
        return false;
    }
    if env::var_os("GRAPHVIZ_ANYWHERE_ALLOW_DOWNLOAD").is_none() {
        return false;
    }

    let target = env::var("TARGET").unwrap_or_default();

    let Some(asset) = target_triple_to_asset_name(&target) else {
        eprintln!(
            "graphviz-anywhere: no GitHub release asset known for target {target:?}; \
             set GRAPHVIZ_ANYWHERE_DIR to point to the library directory"
        );
        return false;
    };

    // Windows .zip assets are not auto-extracted yet.
    // TODO: windows zip extraction — implement curl+unzip (or PowerShell
    // Expand-Archive) to handle .zip assets for Windows targets.
    if asset.ends_with(".zip") {
        eprintln!(
            "graphviz-anywhere: automatic download of Windows .zip assets is not yet \
             implemented.  Set GRAPHVIZ_ANYWHERE_DIR to point at the directory \
             containing graphviz_api.lib, or drop the prebuilt lib under \
             packages/rust/prebuilt/{target}/"
        );
        return false;
    }

    let release_version = env::var("GRAPHVIZ_ANYWHERE_RELEASE_VERSION")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").unwrap_or_default());

    if release_version.is_empty() {
        return false;
    }

    // graphviz-anywhere ships from the supramark monorepo; allow overriding the
    // base URL for forks / mirrors that host the prebuilt assets elsewhere.
    let base_url = env::var("GRAPHVIZ_ANYWHERE_RELEASE_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://github.com/Actrium/supramark/releases/download".to_string());
    let url = format!("{base_url}/v{release_version}/{asset}");

    let Some(out_dir) = env::var_os("OUT_DIR").map(PathBuf::from) else {
        return false;
    };

    // Keyed by version so cargo's OUT_DIR cache invalidates cleanly when the
    // crate version changes.
    let staging = out_dir.join(format!("graphviz-anywhere-prebuilt-v{release_version}"));
    let lib_subdir = staging.join("lib");
    let lib_file = lib_subdir.join(asset_lib_filename(&target));

    if !lib_file.exists() {
        if let Err(e) = std::fs::create_dir_all(&staging) {
            eprintln!(
                "graphviz-anywhere: cannot create {}: {e}",
                staging.display()
            );
            return false;
        }
        let archive = staging.join(asset);

        let curl = Command::new("curl")
            .args(["-sSfL", "--retry", "3", "--retry-delay", "2", "-o"])
            .arg(&archive)
            .arg(&url)
            .status();
        let downloaded = matches!(curl, Ok(s) if s.success());
        if !downloaded {
            eprintln!(
                "graphviz-anywhere: failed to download {url} (curl status: {curl:?}); \
                 falling through to manual setup paths"
            );
            return false;
        }

        let untar = Command::new("tar")
            .args(["-xzf"])
            .arg(&archive)
            .arg("-C")
            .arg(&staging)
            .status();
        if !matches!(untar, Ok(s) if s.success()) {
            eprintln!(
                "graphviz-anywhere: failed to extract {} (tar status: {untar:?})",
                archive.display()
            );
            return false;
        }

        if !lib_file.exists() {
            eprintln!(
                "graphviz-anywhere: archive {} did not contain expected {}",
                archive.display(),
                lib_file.display()
            );
            return false;
        }
    }

    let is_static = asset_is_static(&target);
    emit_search_path(&lib_subdir, !is_static);
    if is_static {
        println!("cargo:rustc-link-lib=static=graphviz_api");
    } else {
        println!("cargo:rustc-link-lib=dylib=graphviz_api");
    }
    println!("cargo:rerun-if-env-changed=GRAPHVIZ_ANYWHERE_RELEASE_VERSION");
    println!("cargo:rerun-if-env-changed=GRAPHVIZ_ANYWHERE_RELEASE_BASE_URL");
    println!("cargo:rerun-if-env-changed=GRAPHVIZ_ANYWHERE_ALLOW_DOWNLOAD");
    println!("cargo:rerun-if-env-changed=GRAPHVIZ_ANYWHERE_NO_DOWNLOAD");
    true
}

fn main() {
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rerun-if-env-changed=GRAPHVIZ_ANYWHERE_DIR");
    println!("cargo:rerun-if-env-changed=GRAPHVIZ_NATIVE_DIR");
    println!("cargo:rerun-if-changed=prebuilt/");

    // On wasm32, the Rust crate delegates to a host-provided JavaScript
    // function — no native linking required.
    if target_arch == "wasm32" {
        return;
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    if try_env_override()
        || try_prebuilt(&manifest_dir)
        || try_repo_output(&manifest_dir)
        || try_github_release()
    {
        return;
    }

    // ── Descriptive panic message ─────────────────────────────────────────────
    let expected_asset = target_triple_to_asset_name(&target)
        .map(|a| format!("  GitHub release asset : {a}"))
        .unwrap_or_else(|| format!("  GitHub release asset : (none known for target {target:?})"));

    let expected_prebuilt = target_triple_to_prebuilt_subdir(&target)
        .map(|(sub, lib)| format!("  prebuilt path        : packages/rust/prebuilt/{sub}/{lib}"))
        .unwrap_or_else(|| {
            "  prebuilt path        : (no prebuilt mapping for this target)".to_string()
        });

    let output_dirs = target_triple_to_output_dirs(&target);
    let expected_output = if output_dirs.is_empty() {
        "  output/ path         : (no output mapping for this target)".to_string()
    } else {
        format!("  output/ path(s)      : {}", output_dirs.join(", "))
    };

    panic!(
        "\n\
         graphviz-anywhere: unable to locate graphviz_api native library.\n\
         \n\
         Detected target : {target}\n\
         \n\
         Searched in order:\n\
           1. GRAPHVIZ_ANYWHERE_DIR / GRAPHVIZ_NATIVE_DIR env override\n\
         {expected_prebuilt}\n\
         {expected_output}\n\
         {expected_asset}\n\
         \n\
         Fix options:\n\
           a) Set GRAPHVIZ_ANYWHERE_DIR to the directory containing the library:\n\
                export GRAPHVIZ_ANYWHERE_DIR=/path/to/lib\n\
           b) Drop the prebuilt static lib into:\n\
                packages/rust/prebuilt/{target}/libgraphviz_api.a\n\
              (or .lib for Windows targets)\n\
           c) Build from source with scripts/build-<platform>.sh, then re-run\n\
                cargo (the resulting output/ dir is picked up automatically).\n\
           d) If a matching GitHub release is published, opt in to the network\n\
                fallback with GRAPHVIZ_ANYWHERE_ALLOW_DOWNLOAD=1 (override the\n\
                location with GRAPHVIZ_ANYWHERE_RELEASE_BASE_URL if needed).\
         "
    );
}
