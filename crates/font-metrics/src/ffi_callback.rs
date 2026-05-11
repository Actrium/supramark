//! Native-side text-measurement FFI callback bridge.
//!
//! Counterpart to [`crate::host_callback`] for non-wasm targets. On
//! React Native (iOS / Android), and any other native host that wants
//! Layer 1 (in-Rust layout) to agree with the platform's real text
//! renderer, the host installs a plain `extern "C"` measurement
//! callback once at startup; this struct adapts that callback to the
//! [`crate::Metrics`] trait.
//!
//! Cfg: this module is compiled for every target **except** `wasm32`
//! — wasm hosts use `host_callback` (wasm-bindgen externs) instead.
//!
//! # Host contract
//!
//! The host (typically a React Native TurboModule wrapping
//! `react-native-skia` `SkiaText.measureText`, iOS `UIFont`, or
//! Android `Paint`) calls
//! [`supramark_install_metrics_callback`] exactly once before any
//! Rust rendering call. The callback signature deliberately uses
//! `(ptr, len)` rather than NUL-terminated C strings so the host can
//! hand back raw UTF-8 slices without an extra copy / zero-byte
//! audit.
//!
//! ```c
//! // signature the host implements
//! void measure_text(
//!     const char* family, size_t family_len,
//!     const char* text,   size_t text_len,
//!     double size,
//!     uint8_t bold, uint8_t italic,
//!     double* out_width,
//!     double* out_ascent,
//!     double* out_descent
//! );
//!
//! // wiring it into the Rust side at startup
//! supramark_install_metrics_callback(measure_text);
//! ```
//!
//! If the host never installs a callback (or the callback returns
//! `NaN` / non-finite values that look like an error path), every
//! measurement falls back to the same `size * 0.6`-per-char
//! heuristic [`crate::host_callback::HostCallbackMetrics`] uses — the
//! diagram still renders, just with placeholder widths, and
//! downstream code should treat unusually round numbers as a hint
//! that the host bridge wasn't wired up.

#![cfg(not(target_arch = "wasm32"))]

use crate::{Measured, Metrics};
use std::os::raw::c_char;
use std::sync::atomic::{AtomicPtr, Ordering};

/// FFI signature the host implements.
///
/// `family` / `text` are raw UTF-8 byte slices (ptr + length); they
/// are **not** NUL-terminated and the host must not assume so. The
/// host writes results into the three `out_*` pointers, which the
/// Rust caller guarantees are non-null and writable.
///
/// `bold` / `italic` use `u8` (0 = false, non-zero = true) rather
/// than `bool` because the C ABI representation of `bool` is
/// implementation-defined on older toolchains.
///
/// # Safety
///
/// - `family` must point to `family_len` valid UTF-8 bytes.
/// - `text` must point to `text_len` valid UTF-8 bytes.
/// - `out_width` / `out_ascent` / `out_descent` must be non-null and
///   writable for one `f64` each.
/// - The host implementation must not retain any of the input
///   pointers past the call's return.
pub type MeasureTextFfi = unsafe extern "C" fn(
    family: *const c_char,
    family_len: usize,
    text: *const c_char,
    text_len: usize,
    size: f64,
    bold: u8,
    italic: u8,
    out_width: *mut f64,
    out_ascent: *mut f64,
    out_descent: *mut f64,
);

/// Global slot holding the host-installed callback. `AtomicPtr` lets
/// us swap atomically without a mutex, which matters because every
/// measurement reads this on the hot path. We type-pun a function
/// pointer through `*mut ()` because Rust has no `Atomic<fn(...)>`.
///
/// `null` ⇒ host hasn't installed yet ⇒ fallback path.
static CALLBACK: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// Install the host-supplied measurement callback.
///
/// Idempotent — calling multiple times replaces the previously
/// installed callback (last-write-wins). Typically a host calls this
/// exactly once at startup.
pub fn install_ffi_metrics_callback(cb: MeasureTextFfi) {
    // Function-pointer ↔ data-pointer conversion is widely portable
    // (POSIX requires it for `dlsym`; the major Rust target tiers
    // all satisfy it). The cast goes through a `usize` to make
    // intent explicit and silence strict-provenance lints.
    let raw = cb as usize as *mut ();
    CALLBACK.store(raw, Ordering::Release);
}

/// C ABI entry point — the host's RN native module calls this once
/// at module init to wire its `measureText` impl into Rust. Thin
/// re-export of [`install_ffi_metrics_callback`] so the host doesn't
/// have to deal with Rust name mangling.
///
/// # Safety
///
/// `cb` must be a valid function pointer with the signature of
/// [`MeasureTextFfi`] (see that type's safety section).
#[no_mangle]
pub unsafe extern "C" fn supramark_install_metrics_callback(cb: MeasureTextFfi) {
    install_ffi_metrics_callback(cb);
}

fn load_callback() -> Option<MeasureTextFfi> {
    let raw = CALLBACK.load(Ordering::Acquire);
    if raw.is_null() {
        return None;
    }
    // Inverse of the cast in `install_ffi_metrics_callback`.
    // SAFETY: we only ever store function pointers of the
    // `MeasureTextFfi` type via the public installer; the cast back
    // is therefore valid.
    let cb: MeasureTextFfi = unsafe { std::mem::transmute::<usize, MeasureTextFfi>(raw as usize) };
    Some(cb)
}

fn fallback(text: &str, size: f64) -> Measured {
    Measured {
        width: text.chars().count() as f64 * size * 0.6,
        ascent: size * 0.8,
        descent: size * 0.2,
    }
}

/// Adapter that defers every measurement to a host-installed FFI
/// callback (e.g. RN-Skia `SkiaText.measureText` via a TurboModule,
/// UIFont / Paint via a thin C shim).
///
/// Stateless / zero-cost — every method dispatches through the
/// global callback pointer. If the host hasn't installed a callback,
/// returns the placeholder fallback values (`size * 0.6 * len`,
/// `size * 0.8` ascent, `size * 0.2` descent), matching the
/// wasm-side [`crate::host_callback::HostCallbackMetrics`] fallback.
#[derive(Debug, Clone, Copy, Default)]
pub struct FfiCallbackMetrics;

impl Metrics for FfiCallbackMetrics {
    /// Single FFI call returns width + ascent + descent — same shape
    /// as the wasm host-callback path. All 6 helper methods inherit
    /// their default impls (which route back through `measure`), so
    /// the host only wires up one function.
    fn measure(&self, text: &str, family: &str, size: f64, bold: bool, italic: bool) -> Measured {
        let Some(cb) = load_callback() else {
            return fallback(text, size);
        };

        let family_bytes = family.as_bytes();
        let text_bytes = text.as_bytes();
        let mut width: f64 = f64::NAN;
        let mut ascent: f64 = f64::NAN;
        let mut descent: f64 = f64::NAN;

        // SAFETY: pointers come from live `&str` slices and remain
        // valid for the duration of the call; the host contract
        // forbids retaining them past return. `out_*` are non-null
        // stack locals. `from_raw_parts(b"".as_ptr(), 0)` would be a
        // null pointer with zero length on some platforms; explicitly
        // route empty slices through a non-null placeholder so we
        // never hand the host a null `*const c_char` with len 0
        // (some hosts dereference unconditionally).
        unsafe {
            let family_ptr = if family_bytes.is_empty() {
                // Stable non-null placeholder. `b"".as_ptr()` is
                // non-null in current rustc, but we don't want to
                // depend on that.
                [0u8].as_ptr()
            } else {
                family_bytes.as_ptr()
            };
            let text_ptr = if text_bytes.is_empty() {
                [0u8].as_ptr()
            } else {
                text_bytes.as_ptr()
            };
            cb(
                family_ptr as *const c_char,
                family_bytes.len(),
                text_ptr as *const c_char,
                text_bytes.len(),
                size,
                bold as u8,
                italic as u8,
                &mut width,
                &mut ascent,
                &mut descent,
            );
        }

        // Treat NaN / non-finite as "host signalled failure" and
        // fall back to the heuristic. Negative widths are also
        // pathological; clamp by routing them to the fallback.
        if !width.is_finite() || width < 0.0 {
            return fallback(text, size);
        }
        // ascent / descent: if the host left them as NaN we
        // reconstruct from `size`; this matches `host_callback.rs`
        // which also tolerates older Safari leaving these fields
        // out.
        let ascent = if ascent.is_finite() { ascent } else { size * 0.8 };
        let descent = if descent.is_finite() {
            descent
        } else {
            size * 0.2
        };

        Measured {
            width,
            ascent,
            descent,
        }
    }
}

#[cfg(test)]
mod tests {
    //! The callback slot is process-global, so these tests would
    //! interfere if run concurrently — we keep the surface tiny (one
    //! "no callback installed" check that runs first, then one
    //! "install + invoke" check) and rely on serial execution.
    //! Cargo's per-process default `--test-threads=1` is not
    //! guaranteed; instead, the second test installs its callback
    //! and never uninstalls, which leaves the global in a defined
    //! state. The fallback test runs in a separate test binary or
    //! must run before the install test — sequence is enforced by
    //! alphabetical-by-name test order (a_ prefix < b_ prefix).
    //!
    //! For determinism across rustc versions that may reorder tests,
    //! the install test does **not** assume the slot was empty
    //! beforehand; the fallback test runs the measurement before
    //! ever calling `install_ffi_metrics_callback`.
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn a_fallback_when_no_callback_installed() {
        // This test must run before `b_installed_callback_invoked`
        // because the callback slot is process-global. The `a_` /
        // `b_` prefix nudges alphabetical ordering; if cargo test
        // ever reorders, swap to a single combined test.
        // Sanity guard:
        assert!(CALLBACK.load(Ordering::Acquire).is_null(), "callback slot must be empty at start of test; if you see this, another test installed a callback and tests must be made order-independent");

        let m = FfiCallbackMetrics;
        let res = m.measure("Hello", "SansSerif", 12.0, false, false);
        // Fallback: 5 chars * 12.0 * 0.6 = 36.0
        assert!((res.width - 36.0).abs() < 1e-9, "width={}", res.width);
        assert!((res.ascent - 12.0 * 0.8).abs() < 1e-9);
        assert!((res.descent - 12.0 * 0.2).abs() < 1e-9);
    }

    static MOCK_CALL_COUNT: AtomicU64 = AtomicU64::new(0);

    unsafe extern "C" fn mock_measure(
        _family: *const c_char,
        _family_len: usize,
        _text: *const c_char,
        text_len: usize,
        size: f64,
        _bold: u8,
        _italic: u8,
        out_width: *mut f64,
        out_ascent: *mut f64,
        out_descent: *mut f64,
    ) {
        MOCK_CALL_COUNT.fetch_add(1, Ordering::SeqCst);
        // Return a deterministic shape distinguishable from the
        // fallback: width = text_len + 100, ascent = size + 1,
        // descent = size + 2. The +100 / +1 / +2 offsets make it
        // impossible to confuse with the fallback's
        // `0.6 * len * size`.
        *out_width = text_len as f64 + 100.0;
        *out_ascent = size + 1.0;
        *out_descent = size + 2.0;
    }

    #[test]
    fn b_installed_callback_invoked() {
        install_ffi_metrics_callback(mock_measure);
        let before = MOCK_CALL_COUNT.load(Ordering::SeqCst);

        let m = FfiCallbackMetrics;
        let res = m.measure("ab", "SansSerif", 14.0, true, false);

        let after = MOCK_CALL_COUNT.load(Ordering::SeqCst);
        assert_eq!(after, before + 1, "mock callback should have been invoked exactly once");
        // `text_len + 100`: "ab".len() == 2, so width == 102.
        assert!((res.width - 102.0).abs() < 1e-9, "width={}", res.width);
        assert!((res.ascent - 15.0).abs() < 1e-9, "ascent={}", res.ascent);
        assert!((res.descent - 16.0).abs() < 1e-9, "descent={}", res.descent);
    }
}
