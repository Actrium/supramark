// Pure mapping functions used by build.rs.
//
// This module is `include!`-d from build.rs so the same logic can be unit-tested
// without duplicating code. Do NOT add dependencies beyond `std` here.

/// Architecture of a musl Linux target, or `None` when it is not one.
///
/// Matched by suffix rather than by exact triple. Alpine's distro rustc reports
/// its host as `x86_64-alpine-linux-musl`, not rustup's
/// `x86_64-unknown-linux-musl` — and `apk add cargo` is the most likely way
/// anyone builds this on Alpine. Keying on the rustup spelling alone left that
/// toolchain with no asset at all (the build.rs panic the download path exists
/// to avoid) and, worse, classified it as a shared-library target, producing a
/// binary that linked `libgraphviz_api.so` and then could not find it at
/// runtime.
pub fn musl_linux_arch(target: &str) -> Option<&'static str> {
    if !target.ends_with("-linux-musl") {
        return None;
    }
    match target.split('-').next() {
        Some("x86_64") => Some("x86_64"),
        Some("aarch64") => Some("aarch64"),
        _ => None,
    }
}

/// Returns `true` for any musl Linux target, including ABI variants this crate
/// ships no asset for (e.g. `armv7-unknown-linux-musleabihf`). Those still link
/// statically when a library is supplied by hand: rustc defaults musl targets to
/// `crt-static`, which makes a shared library unusable.
fn is_musl_linux(target: &str) -> bool {
    target.contains("-linux-musl")
}

/// Maps a Rust target triple to the GitHub release asset name.
///
/// Returns `None` for targets that are not yet covered (caller should fall back
/// to `GRAPHVIZ_ANYWHERE_DIR` env override or a manual prebuilt drop-in).
pub fn target_triple_to_asset_name(target: &str) -> Option<&'static str> {
    // musl first, by suffix, so distro triples resolve too — see `musl_linux_arch`.
    match musl_linux_arch(target) {
        Some("x86_64") => return Some("graphviz-native-linux-musl-x86_64.tar.gz"),
        Some("aarch64") => return Some("graphviz-native-linux-musl-aarch64.tar.gz"),
        _ => {}
    }

    match target {
        // ── Linux ──────────────────────────────────────────────────────────────
        "x86_64-unknown-linux-gnu" => Some("graphviz-native-linux-x86_64.tar.gz"),

        "aarch64-unknown-linux-gnu" => Some("graphviz-native-linux-aarch64.tar.gz"),

        // ── macOS ──────────────────────────────────────────────────────────────
        "x86_64-apple-darwin"
        | "aarch64-apple-darwin"
        | "universal-apple-darwin" => Some("graphviz-native-macos-universal.tar.gz"),

        // ── Android ────────────────────────────────────────────────────────────
        "aarch64-linux-android" => Some("graphviz-native-android-arm64-v8a.tar.gz"),
        "armv7-linux-androideabi" => Some("graphviz-native-android-armeabi-v7a.tar.gz"),
        "x86_64-linux-android" => Some("graphviz-native-android-x86_64.tar.gz"),
        "i686-linux-android" => Some("graphviz-native-android-x86.tar.gz"),

        // ── iOS device ─────────────────────────────────────────────────────────
        "aarch64-apple-ios" => Some("graphviz-native-ios-device-arm64.tar.gz"),

        // ── iOS simulator ──────────────────────────────────────────────────────
        "aarch64-apple-ios-sim" => Some("graphviz-native-ios-sim-arm64.tar.gz"),
        "x86_64-apple-ios" => Some("graphviz-native-ios-sim-x86_64.tar.gz"),

        // ── Windows ────────────────────────────────────────────────────────────
        "x86_64-pc-windows-msvc" => Some("graphviz-native-windows-x86_64.tar.gz"),
        "aarch64-pc-windows-msvc" => Some("graphviz-native-windows-arm64.tar.gz"),

        _ => None,
    }
}

/// Returns the `prebuilt/<triple>/` sub-path (relative to the manifest dir) and
/// the expected lib filename for the given target triple.
///
/// Returns `None` when the triple is unrecognised or wasm (no native link needed).
pub fn target_triple_to_prebuilt_subdir(target: &str) -> Option<(&'static str, &'static str)> {
    // Every musl spelling maps to the canonical rustup-named directory, which is
    // what CI populates.
    match musl_linux_arch(target) {
        Some("x86_64") => return Some(("x86_64-unknown-linux-musl", "libgraphviz_api.a")),
        Some("aarch64") => return Some(("aarch64-unknown-linux-musl", "libgraphviz_api.a")),
        _ => {}
    }

    // (subdirectory under prebuilt/, lib filename)
    match target {
        "x86_64-unknown-linux-gnu" => {
            Some(("x86_64-unknown-linux-gnu", "libgraphviz_api.a"))
        }

        "aarch64-unknown-linux-gnu" => {
            Some(("aarch64-unknown-linux-gnu", "libgraphviz_api.a"))
        }

        "x86_64-apple-darwin" => Some(("x86_64-apple-darwin", "libgraphviz_api.a")),
        "aarch64-apple-darwin" => Some(("aarch64-apple-darwin", "libgraphviz_api.a")),

        "aarch64-apple-ios" => Some(("aarch64-apple-ios", "libgraphviz_api.a")),
        "aarch64-apple-ios-sim" => Some(("aarch64-apple-ios-sim", "libgraphviz_api.a")),
        "x86_64-apple-ios" => Some(("x86_64-apple-ios", "libgraphviz_api.a")),

        "x86_64-pc-windows-msvc" => {
            Some(("x86_64-pc-windows-msvc", "graphviz_api.lib"))
        }
        "aarch64-pc-windows-msvc" => Some(("aarch64-pc-windows-msvc", "graphviz_api.lib")),

        _ => None,
    }
}

/// Maps a Rust target triple to the expected `output/` sub-paths (relative to
/// repo root) where the CI / script build places the library.
///
/// Returns an empty slice for unrecognised targets; the caller should treat that
/// as "not found".
pub fn target_triple_to_output_dirs(target: &str) -> &'static [&'static str] {
    // No `output/linux/lib` fallback for musl: that legacy directory is glibc.
    match musl_linux_arch(target) {
        Some("x86_64") => return &["output/linux-musl-x86_64/lib"],
        Some("aarch64") => return &["output/linux-musl-aarch64/lib"],
        _ => {}
    }

    match target {
        "x86_64-unknown-linux-gnu" => &["output/linux-x86_64/lib", "output/linux/lib"],

        "aarch64-unknown-linux-gnu" => &["output/linux-aarch64/lib", "output/linux/lib"],

        "x86_64-apple-darwin"
        | "aarch64-apple-darwin"
        | "universal-apple-darwin" => &["output/macos-universal/lib"],

        "aarch64-linux-android" => &["output/android/arm64-v8a/lib"],
        "armv7-linux-androideabi" => &["output/android/armeabi-v7a/lib"],
        "x86_64-linux-android" => &["output/android/x86_64/lib"],
        "i686-linux-android" => &["output/android/x86/lib"],

        "aarch64-apple-ios" => &["output/ios/iphoneos-arm64/lib"],
        "aarch64-apple-ios-sim" => &["output/ios/iphonesimulator-arm64/lib"],
        "x86_64-apple-ios" => &["output/ios/iphonesimulator-x86_64/lib"],

        "x86_64-pc-windows-msvc" => &[
            "output/windows-x86_64/lib",
            "output/windows-x86_64/bin",
        ],
        "aarch64-pc-windows-msvc" => &[
            "output/windows-arm64/lib",
            "output/windows-arm64/bin",
        ],

        _ => &[],
    }
}

/// Returns `true` for iOS targets (both device and simulator).
pub fn is_ios_target(target: &str) -> bool {
    matches!(
        target,
        "aarch64-apple-ios" | "aarch64-apple-ios-sim" | "x86_64-apple-ios"
    )
}

/// Returns whether the legacy per-OS prebuilt layout is safe to use.
///
/// Legacy directories do not encode an architecture, so they are only valid
/// for a native build where Cargo's host and target triples match exactly.
/// Missing host metadata must fail closed rather than risk linking a host
/// archive into a cross-compiled binary.
pub fn legacy_prebuilt_is_compatible(host: &str, target: &str) -> bool {
    !host.is_empty() && host == target
}

/// Returns `true` when the default native link is static.
///
/// Desktop and iOS executables must be self-contained: a Cargo build-script
/// rpath is not propagated to final downstream binaries, and a same-named
/// system library can otherwise be selected at process launch. Android keeps
/// the shared library because the application package owns JNI library
/// staging and loading.
///
/// musl is not merely "also desktop": rustc links those targets `crt-static` by
/// default, so a shared library is not linkable there at all.
pub fn asset_is_static(target: &str) -> bool {
    is_ios_target(target)
        || target.contains("windows-msvc")
        || target.contains("unknown-linux-gnu")
        || is_musl_linux(target)
        || target.contains("apple-darwin")
}

/// Returns the lib filename to expect inside the extracted release archive.
pub fn asset_lib_filename(target: &str) -> &'static str {
    match target {
        "x86_64-apple-darwin"
        | "aarch64-apple-darwin"
        | "universal-apple-darwin" => "libgraphviz_api.a",
        t if is_ios_target(t) => "libgraphviz_api.a",
        t if t.contains("windows-msvc") => "graphviz_api.lib",
        t if t.contains("unknown-linux-gnu") || is_musl_linux(t) => "libgraphviz_api.a",
        _ => "libgraphviz_api.so",
    }
}

/// Resolve a static library from an explicit override's candidate directories.
///
/// Windows source builds place a DLL import library and the merged static
/// archive beside one another. `graphviz_api_static.lib` always wins. The
/// canonical `graphviz_api.lib` is accepted only when no adjacent DLL proves
/// that it is an import library (the canonical name is used by release assets).
pub fn find_static_override(
    lib_dirs: &[std::path::PathBuf],
    target_os: &str,
) -> Option<(std::path::PathBuf, &'static str)> {
    if target_os != "windows" {
        return lib_dirs
            .iter()
            .map(|dir| dir.join("libgraphviz_api.a"))
            .find(|path| path.is_file())
            .map(|path| (path, "graphviz_api"));
    }

    if let Some(path) = lib_dirs
        .iter()
        .map(|dir| dir.join("graphviz_api_static.lib"))
        .find(|path| path.is_file())
    {
        return Some((path, "graphviz_api_static"));
    }

    let has_dll = lib_dirs.iter().any(|lib_dir| {
        lib_dir.join("graphviz_api.dll").is_file()
            || lib_dir
                .parent()
                .map_or(false, |parent| parent.join("bin/graphviz_api.dll").is_file())
    });
    if has_dll {
        return None;
    }

    lib_dirs
        .iter()
        .map(|dir| dir.join("graphviz_api.lib"))
        .find(|path| path.is_file())
        .map(|path| (path, "graphviz_api"))
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── target_triple_to_asset_name ────────────────────────────────────────────

    #[test]
    fn linux_x86_64_asset() {
        assert_eq!(
            target_triple_to_asset_name("x86_64-unknown-linux-gnu"),
            Some("graphviz-native-linux-x86_64.tar.gz")
        );
    }

    #[test]
    fn linux_aarch64_asset() {
        assert_eq!(
            target_triple_to_asset_name("aarch64-unknown-linux-gnu"),
            Some("graphviz-native-linux-aarch64.tar.gz")
        );
    }

    /// Regression: Alpine's distro rustc reports `x86_64-alpine-linux-musl`, and
    /// `apk add cargo` is how Alpine users most often build. Matching only the
    /// rustup spelling gave that toolchain no asset (build.rs panic) and treated
    /// it as a shared-library target, yielding a binary that linked
    /// `libgraphviz_api.so` and could not find it at runtime.
    #[test]
    fn distro_musl_triples_resolve_like_the_rustup_one() {
        for target in [
            "x86_64-unknown-linux-musl",
            "x86_64-alpine-linux-musl",
        ] {
            assert_eq!(
                target_triple_to_asset_name(target),
                Some("graphviz-native-linux-musl-x86_64.tar.gz"),
                "{target}"
            );
            assert_eq!(
                target_triple_to_prebuilt_subdir(target),
                Some(("x86_64-unknown-linux-musl", "libgraphviz_api.a")),
                "{target}"
            );
            assert_eq!(
                target_triple_to_output_dirs(target),
                &["output/linux-musl-x86_64/lib"],
                "{target}"
            );
            assert!(asset_is_static(target), "{target} must link statically");
            assert_eq!(asset_lib_filename(target), "libgraphviz_api.a", "{target}");
        }

        for target in [
            "aarch64-unknown-linux-musl",
            "aarch64-alpine-linux-musl",
        ] {
            assert_eq!(
                target_triple_to_asset_name(target),
                Some("graphviz-native-linux-musl-aarch64.tar.gz"),
                "{target}"
            );
            assert!(asset_is_static(target), "{target} must link statically");
        }
    }

    /// A musl ABI variant with no asset still must not be classified as a
    /// shared-library target: crt-static makes a `.so` unusable there.
    #[test]
    fn musl_abi_variants_without_assets_still_link_statically() {
        let target = "armv7-unknown-linux-musleabihf";
        assert_eq!(target_triple_to_asset_name(target), None);
        assert!(asset_is_static(target));
        assert_eq!(asset_lib_filename(target), "libgraphviz_api.a");
    }

    #[test]
    fn linux_musl_assets_are_separate_from_glibc() {
        // Sharing the glibc archive would link a glibc-built library into a musl
        // binary; the musl assets are built and packaged independently.
        assert_eq!(
            target_triple_to_asset_name("x86_64-unknown-linux-musl"),
            Some("graphviz-native-linux-musl-x86_64.tar.gz")
        );
        assert_eq!(
            target_triple_to_asset_name("aarch64-unknown-linux-musl"),
            Some("graphviz-native-linux-musl-aarch64.tar.gz")
        );
        assert_ne!(
            target_triple_to_asset_name("x86_64-unknown-linux-musl"),
            target_triple_to_asset_name("x86_64-unknown-linux-gnu")
        );
    }

    #[test]
    fn macos_asset_universal() {
        assert_eq!(
            target_triple_to_asset_name("x86_64-apple-darwin"),
            Some("graphviz-native-macos-universal.tar.gz")
        );
        assert_eq!(
            target_triple_to_asset_name("aarch64-apple-darwin"),
            Some("graphviz-native-macos-universal.tar.gz")
        );
    }

    #[test]
    fn android_assets() {
        assert_eq!(
            target_triple_to_asset_name("aarch64-linux-android"),
            Some("graphviz-native-android-arm64-v8a.tar.gz")
        );
        assert_eq!(
            target_triple_to_asset_name("armv7-linux-androideabi"),
            Some("graphviz-native-android-armeabi-v7a.tar.gz")
        );
        assert_eq!(
            target_triple_to_asset_name("x86_64-linux-android"),
            Some("graphviz-native-android-x86_64.tar.gz")
        );
        assert_eq!(
            target_triple_to_asset_name("i686-linux-android"),
            Some("graphviz-native-android-x86.tar.gz")
        );
    }

    #[test]
    fn ios_device_asset() {
        assert_eq!(
            target_triple_to_asset_name("aarch64-apple-ios"),
            Some("graphviz-native-ios-device-arm64.tar.gz")
        );
    }

    #[test]
    fn ios_simulator_assets() {
        assert_eq!(
            target_triple_to_asset_name("aarch64-apple-ios-sim"),
            Some("graphviz-native-ios-sim-arm64.tar.gz")
        );
        assert_eq!(
            target_triple_to_asset_name("x86_64-apple-ios"),
            Some("graphviz-native-ios-sim-x86_64.tar.gz")
        );
    }

    #[test]
    fn windows_arm64_asset() {
        assert_eq!(
            target_triple_to_asset_name("aarch64-pc-windows-msvc"),
            Some("graphviz-native-windows-arm64.tar.gz")
        );
    }

    #[test]
    fn windows_x86_64_asset() {
        assert_eq!(
            target_triple_to_asset_name("x86_64-pc-windows-msvc"),
            Some("graphviz-native-windows-x86_64.tar.gz")
        );
    }

    #[test]
    fn unknown_target_returns_none() {
        assert_eq!(target_triple_to_asset_name("wasm32-unknown-unknown"), None);
        assert_eq!(target_triple_to_asset_name("riscv64gc-unknown-linux-gnu"), None);
    }

    // ── target_triple_to_prebuilt_subdir ────────────────────────────────────────

    #[test]
    fn prebuilt_subdir_ios_device() {
        let (subdir, lib) = target_triple_to_prebuilt_subdir("aarch64-apple-ios").unwrap();
        assert_eq!(subdir, "aarch64-apple-ios");
        assert_eq!(lib, "libgraphviz_api.a");
    }

    #[test]
    fn prebuilt_subdir_ios_sim_arm64() {
        let (subdir, lib) = target_triple_to_prebuilt_subdir("aarch64-apple-ios-sim").unwrap();
        assert_eq!(subdir, "aarch64-apple-ios-sim");
        assert_eq!(lib, "libgraphviz_api.a");
    }

    #[test]
    fn prebuilt_subdir_linux_aarch64() {
        let (subdir, lib) = target_triple_to_prebuilt_subdir("aarch64-unknown-linux-gnu").unwrap();
        assert_eq!(subdir, "aarch64-unknown-linux-gnu");
        assert_eq!(lib, "libgraphviz_api.a");
    }

    #[test]
    fn prebuilt_subdir_android_x86() {
        assert_eq!(target_triple_to_prebuilt_subdir("i686-linux-android"), None);
    }

    #[test]
    fn prebuilt_subdir_windows_arm64() {
        let (subdir, lib) = target_triple_to_prebuilt_subdir("aarch64-pc-windows-msvc").unwrap();
        assert_eq!(subdir, "aarch64-pc-windows-msvc");
        assert_eq!(lib, "graphviz_api.lib");
    }

    // ── target_triple_to_output_dirs ────────────────────────────────────────────

    #[test]
    fn output_dirs_ios_device() {
        let dirs = target_triple_to_output_dirs("aarch64-apple-ios");
        assert_eq!(dirs, &["output/ios/iphoneos-arm64/lib"]);
    }

    #[test]
    fn output_dirs_ios_sim_arm64() {
        let dirs = target_triple_to_output_dirs("aarch64-apple-ios-sim");
        assert_eq!(dirs, &["output/ios/iphonesimulator-arm64/lib"]);
    }

    #[test]
    fn output_dirs_ios_sim_x86_64() {
        let dirs = target_triple_to_output_dirs("x86_64-apple-ios");
        assert_eq!(dirs, &["output/ios/iphonesimulator-x86_64/lib"]);
    }

    #[test]
    fn output_dirs_linux_aarch64() {
        let dirs = target_triple_to_output_dirs("aarch64-unknown-linux-gnu");
        assert!(dirs.contains(&"output/linux-aarch64/lib"));
    }

    #[test]
    fn output_dirs_android_x86() {
        let dirs = target_triple_to_output_dirs("i686-linux-android");
        assert_eq!(dirs, &["output/android/x86/lib"]);
    }

    // ── asset_lib_filename ───────────────────────────────────────────────────────

    #[test]
    fn asset_lib_filename_ios_is_static() {
        assert_eq!(asset_lib_filename("aarch64-apple-ios"), "libgraphviz_api.a");
        assert_eq!(asset_lib_filename("aarch64-apple-ios-sim"), "libgraphviz_api.a");
        assert_eq!(asset_lib_filename("x86_64-apple-ios"), "libgraphviz_api.a");
    }

    #[test]
    fn asset_lib_filename_macos_is_static() {
        assert_eq!(asset_lib_filename("aarch64-apple-darwin"), "libgraphviz_api.a");
    }

    #[test]
    fn asset_lib_filename_linux_is_static() {
        assert_eq!(asset_lib_filename("x86_64-unknown-linux-gnu"), "libgraphviz_api.a");
        assert_eq!(
            asset_lib_filename("x86_64-unknown-linux-musl"),
            "libgraphviz_api.a"
        );
    }

    #[test]
    fn asset_lib_filename_windows_is_static_import_name() {
        assert_eq!(asset_lib_filename("x86_64-pc-windows-msvc"), "graphviz_api.lib");
    }

    // ── is_ios_target ────────────────────────────────────────────────────────────

    #[test]
    fn is_ios_target_recognition() {
        assert!(is_ios_target("aarch64-apple-ios"));
        assert!(is_ios_target("aarch64-apple-ios-sim"));
        assert!(is_ios_target("x86_64-apple-ios"));
        assert!(!is_ios_target("aarch64-apple-darwin"));
        assert!(!is_ios_target("aarch64-linux-android"));
    }

    #[test]
    fn legacy_prebuilt_is_native_only() {
        assert!(legacy_prebuilt_is_compatible(
            "aarch64-apple-darwin",
            "aarch64-apple-darwin"
        ));
        assert!(!legacy_prebuilt_is_compatible(
            "aarch64-apple-darwin",
            "x86_64-apple-darwin"
        ));
        assert!(!legacy_prebuilt_is_compatible(
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu"
        ));
        assert!(!legacy_prebuilt_is_compatible(
            "",
            "x86_64-pc-windows-msvc"
        ));
    }

    // ── asset_is_static (static vs. dynamic link policy) ─────────────────────────

    #[test]
    fn asset_is_static_for_desktop_and_ios() {
        assert!(asset_is_static("aarch64-apple-ios"));
        assert!(asset_is_static("aarch64-apple-ios-sim"));
        assert!(asset_is_static("x86_64-apple-ios"));
        assert!(asset_is_static("x86_64-pc-windows-msvc"));
        assert!(asset_is_static("x86_64-unknown-linux-gnu"));
        assert!(asset_is_static("aarch64-unknown-linux-gnu"));
        // rustc links musl targets crt-static by default, so a shared library is
        // not an option there.
        assert!(asset_is_static("x86_64-unknown-linux-musl"));
        assert!(asset_is_static("aarch64-unknown-linux-musl"));
        assert!(asset_is_static("aarch64-apple-darwin"));
        assert!(asset_is_static("x86_64-apple-darwin"));
        assert!(!asset_is_static("aarch64-linux-android"));
    }

    /// A target only gets an asset once one is actually built *for its ABI*.
    /// musl now has its own (see `linux_musl_assets_are_separate_from_glibc`);
    /// windows-gnu still has none, and must not silently fall back to the MSVC
    /// archive.
    #[test]
    fn incompatible_abi_assets_are_not_auto_selected() {
        assert_eq!(target_triple_to_asset_name("x86_64-pc-windows-gnu"), None);
        assert_eq!(target_triple_to_prebuilt_subdir("x86_64-pc-windows-gnu"), None);
        assert!(target_triple_to_output_dirs("x86_64-pc-windows-gnu").is_empty());
    }

    #[test]
    fn windows_override_prefers_merged_static_archive() {
        let root = std::env::temp_dir().join(format!(
            "graphviz-anywhere-static-override-{}",
            std::process::id()
        ));
        let lib = root.join("lib");
        let bin = root.join("bin");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(lib.join("graphviz_api.lib"), b"import").unwrap();
        std::fs::write(lib.join("graphviz_api_static.lib"), b"static").unwrap();
        std::fs::write(bin.join("graphviz_api.dll"), b"dll").unwrap();

        let resolved = find_static_override(std::slice::from_ref(&lib), "windows").unwrap();
        assert_eq!(resolved.0, lib.join("graphviz_api_static.lib"));
        assert_eq!(resolved.1, "graphviz_api_static");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn windows_override_rejects_import_library_as_static() {
        let root = std::env::temp_dir().join(format!(
            "graphviz-anywhere-import-override-{}",
            std::process::id()
        ));
        let lib = root.join("lib");
        let bin = root.join("bin");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(lib.join("graphviz_api.lib"), b"import").unwrap();
        std::fs::write(bin.join("graphviz_api.dll"), b"dll").unwrap();

        assert!(find_static_override(&[lib], "windows").is_none());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn windows_release_canonical_library_is_static_without_dll() {
        let root = std::env::temp_dir().join(format!(
            "graphviz-anywhere-release-override-{}",
            std::process::id()
        ));
        let lib = root.join("lib");
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(lib.join("graphviz_api.lib"), b"static").unwrap();

        let resolved = find_static_override(std::slice::from_ref(&lib), "windows").unwrap();
        assert_eq!(resolved.0, lib.join("graphviz_api.lib"));
        assert_eq!(resolved.1, "graphviz_api");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn asset_is_static_agrees_with_lib_filename() {
        // The link policy must match the expected library file: static targets
        // ship a `.a`, dynamic targets a `.so` / `.dylib`.
        for target in [
            "aarch64-apple-ios",
            "x86_64-unknown-linux-gnu",
            "x86_64-unknown-linux-musl",
            "aarch64-unknown-linux-musl",
            "aarch64-apple-darwin",
            "aarch64-linux-android",
            "x86_64-pc-windows-msvc",
        ] {
            let lib_name = asset_lib_filename(target);
            let ships_archive = lib_name.ends_with(".a") || lib_name.ends_with(".lib");
            assert_eq!(
                asset_is_static(target),
                ships_archive,
                "asset_is_static and asset_lib_filename disagree for {target}"
            );
        }
    }
}
