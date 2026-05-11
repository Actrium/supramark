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
            emit_search_path(&dir, false);
            println!("cargo:rustc-link-lib=static=graphviz_api");
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
            emit_search_path(&dir, false);
            println!("cargo:rustc-link-lib=static=graphviz_api");
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

    // Determine whether we expect a static or dynamic lib for this target.
    let prefer_static = is_ios_target(&target) || target_os == "macos";

    // ── Collect candidate directories ────────────────────────────────────────

    let mut candidates: Vec<(PathBuf, bool /* is_static */)> = Vec::new();

    // Standard output/ dirs produced by CI/scripts.
    for rel in target_triple_to_output_dirs(&target) {
        candidates.push((repo_root.join(rel), prefer_static));
    }

    // RN postinstall paths — React-Native's postinstall script copies the
    // native libs under packages/react-native/{ios,android}/.
    let rn_root = repo_root.join("packages").join("react-native");
    if is_ios_target(&target) {
        candidates.push((
            rn_root.join("ios").join("Frameworks").join("lib"),
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
                rn_root.join("android").join("libs").join(abi),
                false, // Android: .so preferred
            ));
            candidates.push((
                rn_root.join("android").join("Frameworks").join("lib"),
                false,
            ));
        }
    } else if target_os == "macos" {
        candidates.push((rn_root.join("ios").join("Frameworks").join("lib"), true));
    }

    // ── Walk candidates ──────────────────────────────────────────────────────

    for (dir, want_static) in candidates {
        if !dir.exists() {
            continue;
        }

        if want_static {
            let static_lib = dir.join("libgraphviz_api.a");
            if static_lib.exists() {
                emit_search_path(&dir, false);
                println!("cargo:rustc-link-lib=static=graphviz_api");
                return true;
            }
        } else {
            // Dynamic: prefer .a on macOS/Linux desktop for cleaner downstream
            // linking; fall back to .so / .dylib.
            let static_lib = dir.join("libgraphviz_api.a");
            if static_lib.exists() && (target_os == "macos" || target_os == "linux") {
                emit_search_path(&dir, false);
                println!("cargo:rustc-link-lib=static=graphviz_api");
                return true;
            }
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

/// Last-resort fallback: download the prebuilt library matching this crate's
/// version from the GitHub release and link against it.  Lets a downstream
/// `cargo add graphviz-anywhere` work with zero local setup.
///
/// Disable with `GRAPHVIZ_ANYWHERE_NO_DOWNLOAD=1` (forces a hard error so
/// you can detect it in CI/sandbox builds).  Override the release tag with
/// `GRAPHVIZ_ANYWHERE_RELEASE_VERSION=0.1.7` (defaults to CARGO_PKG_VERSION,
/// useful when a crate version's matching tag hasn't shipped yet).
fn try_github_release() -> bool {
    if env::var_os("GRAPHVIZ_ANYWHERE_NO_DOWNLOAD").is_some() {
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

    let url = format!(
        "https://github.com/kookyleo/graphviz-anywhere/releases/download/v{release_version}/{asset}"
    );

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
    println!("cargo:rerun-if-env-changed=GRAPHVIZ_ANYWHERE_NO_DOWNLOAD");
    true
}

fn main() {
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    let target_arch =
        env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| "unknown".to_string());

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
        .unwrap_or_else(|| {
            format!("  GitHub release asset : (none known for target {target:?})")
        });

    let expected_prebuilt = target_triple_to_prebuilt_subdir(&target)
        .map(|(sub, lib)| {
            format!(
                "  prebuilt path        : packages/rust/prebuilt/{sub}/{lib}"
            )
        })
        .unwrap_or_else(|| {
            "  prebuilt path        : (no prebuilt mapping for this target)".to_string()
        });

    let output_dirs = target_triple_to_output_dirs(&target);
    let expected_output = if output_dirs.is_empty() {
        "  output/ path         : (no output mapping for this target)".to_string()
    } else {
        format!(
            "  output/ path(s)      : {}",
            output_dirs.join(", ")
        )
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
           c) Unset GRAPHVIZ_ANYWHERE_NO_DOWNLOAD to allow auto-download from\n\
                GitHub release v$CARGO_PKG_VERSION\n\
           d) Build from source with scripts/build-<platform>.sh and re-run cargo.\
         "
    );
}
