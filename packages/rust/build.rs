use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn emit_search_path(dir: &Path, dynamic: bool) {
    println!("cargo:rustc-link-search=native={}", dir.display());

    if dynamic && !cfg!(target_os = "windows") {
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

fn try_prebuilt(manifest_dir: &Path) -> bool {
    let target_dir = if cfg!(target_os = "macos") {
        manifest_dir.join("prebuilt/macos")
    } else if cfg!(target_os = "linux") {
        manifest_dir.join("prebuilt/linux")
    } else if cfg!(target_os = "windows") {
        manifest_dir.join("prebuilt/windows")
    } else {
        return false;
    };

    if !target_dir.exists() {
        return false;
    }

    let static_lib = if cfg!(target_os = "windows") {
        target_dir.join("graphviz_api.lib")
    } else {
        target_dir.join("libgraphviz_api.a")
    };

    if !static_lib.exists() {
        return false;
    }

    emit_search_path(&target_dir, false);
    println!("cargo:rustc-link-lib=static=graphviz_api");
    true
}

fn try_repo_output(manifest_dir: &Path) -> bool {
    let Some(repo_root) = manifest_dir.parent().and_then(Path::parent) else {
        return false;
    };
    let output_root = repo_root.join("output");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    let candidates: Vec<PathBuf> = if target_os == "android" {
        let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        let abi = match target_arch.as_str() {
            "aarch64" => "arm64-v8a",
            "arm" => "armeabi-v7a",
            "x86_64" => "x86_64",
            "x86" => "x86",
            _ => return false,
        };
        vec![output_root.join("android").join(abi).join("lib")]
    } else if cfg!(target_os = "macos") {
        vec![output_root.join("macos-universal/lib")]
    } else if cfg!(target_os = "linux") {
        vec![
            output_root.join("linux-x86_64/lib"),
            output_root.join("linux/lib"),
        ]
    } else if cfg!(target_os = "windows") {
        vec![
            output_root.join("windows-x86_64/lib"),
            output_root.join("windows-x86_64/bin"),
        ]
    } else {
        Vec::new()
    };

    for dir in candidates {
        if dir.exists() {
            emit_search_path(&dir, true);
            println!("cargo:rustc-link-lib=dylib=graphviz_api");
            return true;
        }
    }

    false
}

/// Last-resort fallback: download the prebuilt shared library matching this
/// crate's version from the GitHub release and link against it. Lets a
/// downstream `cargo add graphviz-anywhere` work with zero local setup.
///
/// Disable with `GRAPHVIZ_ANYWHERE_NO_DOWNLOAD=1` (forces a hard error so
/// you can detect it in CI/sandbox builds). Override the release tag with
/// `GRAPHVIZ_ANYWHERE_RELEASE_VERSION=0.1.7` (defaults to CARGO_PKG_VERSION,
/// useful when a crate version's matching tag has not shipped yet).
fn try_github_release() -> bool {
    if env::var_os("GRAPHVIZ_ANYWHERE_NO_DOWNLOAD").is_some() {
        return false;
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    let asset = match (target_os.as_str(), target_arch.as_str()) {
        ("linux", "x86_64") => "graphviz-native-linux-x86_64.tar.gz",
        ("macos", _) => "graphviz-native-macos-universal.tar.gz",
        ("android", "aarch64") => "graphviz-native-android-arm64-v8a.tar.gz",
        ("android", "arm") => "graphviz-native-android-armeabi-v7a.tar.gz",
        ("android", "x86_64") => "graphviz-native-android-x86_64.tar.gz",
        // Windows ships a .zip with a different layout; not auto-handled
        // here yet — Windows users must set GRAPHVIZ_ANYWHERE_DIR or drop
        // the prebuilt static lib under packages/rust/prebuilt/windows/.
        _ => return false,
    };

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
    let lib_file = match target_os.as_str() {
        "macos" => lib_subdir.join("libgraphviz_api.dylib"),
        _ => lib_subdir.join("libgraphviz_api.so"),
    };

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

    emit_search_path(&lib_subdir, true);
    println!("cargo:rustc-link-lib=dylib=graphviz_api");
    println!("cargo:rerun-if-env-changed=GRAPHVIZ_ANYWHERE_RELEASE_VERSION");
    println!("cargo:rerun-if-env-changed=GRAPHVIZ_ANYWHERE_NO_DOWNLOAD");
    true
}

fn main() {
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

    panic!(
        "Unable to locate graphviz_api. Tried in order: \
GRAPHVIZ_ANYWHERE_DIR / GRAPHVIZ_NATIVE_DIR env override; \
packages/rust/prebuilt/<os>/libgraphviz_api.{{a,lib}}; \
sibling output/<platform>/lib/; \
GitHub release download for v$CARGO_PKG_VERSION. \
Set one of these or unset GRAPHVIZ_ANYWHERE_NO_DOWNLOAD to allow auto-download."
    );
}
