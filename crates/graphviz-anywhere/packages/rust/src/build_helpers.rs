/// Pure mapping functions used by build.rs.
///
/// This module is `include!`-d from build.rs so the same logic can be unit-tested
/// without duplicating code. Do NOT add dependencies beyond `std` here.

/// Maps a Rust target triple to the GitHub release asset name.
///
/// Returns `None` for targets that are not yet covered (caller should fall back
/// to `GRAPHVIZ_ANYWHERE_DIR` env override or a manual prebuilt drop-in).
pub fn target_triple_to_asset_name(target: &str) -> Option<&'static str> {
    match target {
        // ── Linux ──────────────────────────────────────────────────────────────
        "x86_64-unknown-linux-gnu"
        | "x86_64-unknown-linux-musl" => Some("graphviz-native-linux-x86_64.tar.gz"),

        "aarch64-unknown-linux-gnu"
        | "aarch64-unknown-linux-musl" => Some("graphviz-native-linux-aarch64.tar.gz"),

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
        // Windows ships a .zip with a different layout; not auto-extracted here yet.
        // TODO: windows zip extraction — implement curl+unzip logic for these.
        "x86_64-pc-windows-msvc"
        | "x86_64-pc-windows-gnu" => Some("graphviz-native-windows-x86_64.zip"),
        "aarch64-pc-windows-msvc" => Some("graphviz-native-windows-arm64.zip"),

        _ => None,
    }
}

/// Returns the `prebuilt/<triple>/` sub-path (relative to the manifest dir) and
/// the expected lib filename for the given target triple.
///
/// Returns `None` when the triple is unrecognised or wasm (no native link needed).
pub fn target_triple_to_prebuilt_subdir(target: &str) -> Option<(&'static str, &'static str)> {
    // (subdirectory under prebuilt/, lib filename)
    match target {
        "x86_64-unknown-linux-gnu"
        | "x86_64-unknown-linux-musl" => Some(("x86_64-unknown-linux-gnu", "libgraphviz_api.a")),

        "aarch64-unknown-linux-gnu"
        | "aarch64-unknown-linux-musl" => Some(("aarch64-unknown-linux-gnu", "libgraphviz_api.a")),

        "x86_64-apple-darwin" => Some(("x86_64-apple-darwin", "libgraphviz_api.a")),
        "aarch64-apple-darwin" => Some(("aarch64-apple-darwin", "libgraphviz_api.a")),

        "aarch64-apple-ios" => Some(("aarch64-apple-ios", "libgraphviz_api.a")),
        "aarch64-apple-ios-sim" => Some(("aarch64-apple-ios-sim", "libgraphviz_api.a")),
        "x86_64-apple-ios" => Some(("x86_64-apple-ios", "libgraphviz_api.a")),

        "aarch64-linux-android" => Some(("aarch64-linux-android", "libgraphviz_api.a")),
        "armv7-linux-androideabi" => Some(("armv7-linux-androideabi", "libgraphviz_api.a")),
        "x86_64-linux-android" => Some(("x86_64-linux-android", "libgraphviz_api.a")),
        "i686-linux-android" => Some(("i686-linux-android", "libgraphviz_api.a")),

        "x86_64-pc-windows-msvc"
        | "x86_64-pc-windows-gnu" => Some(("x86_64-pc-windows-msvc", "graphviz_api.lib")),
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
    match target {
        "x86_64-unknown-linux-gnu"
        | "x86_64-unknown-linux-musl" => &["output/linux-x86_64/lib", "output/linux/lib"],

        "aarch64-unknown-linux-gnu"
        | "aarch64-unknown-linux-musl" => &["output/linux-aarch64/lib", "output/linux/lib"],

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

        "x86_64-pc-windows-msvc"
        | "x86_64-pc-windows-gnu" => &[
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

/// Returns `true` for targets where the release asset uses `.a` (static archive)
/// rather than `.so` / `.dylib`.
pub fn asset_is_static(target: &str) -> bool {
    is_ios_target(target)
}

/// Returns the lib filename to expect inside the extracted release archive.
pub fn asset_lib_filename(target: &str) -> &'static str {
    match target {
        "x86_64-apple-darwin"
        | "aarch64-apple-darwin"
        | "universal-apple-darwin" => "libgraphviz_api.dylib",
        t if is_ios_target(t) => "libgraphviz_api.a",
        // Windows is not auto-extracted; this value is unused in practice.
        t if t.contains("windows") => "graphviz_api.lib",
        _ => "libgraphviz_api.so",
    }
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
        assert_eq!(
            target_triple_to_asset_name("aarch64-unknown-linux-musl"),
            Some("graphviz-native-linux-aarch64.tar.gz")
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
            Some("graphviz-native-windows-arm64.zip")
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
        let (subdir, lib) = target_triple_to_prebuilt_subdir("i686-linux-android").unwrap();
        assert_eq!(subdir, "i686-linux-android");
        assert_eq!(lib, "libgraphviz_api.a");
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
    fn asset_lib_filename_macos_is_dylib() {
        assert_eq!(asset_lib_filename("aarch64-apple-darwin"), "libgraphviz_api.dylib");
    }

    #[test]
    fn asset_lib_filename_linux_is_so() {
        assert_eq!(asset_lib_filename("x86_64-unknown-linux-gnu"), "libgraphviz_api.so");
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
}
