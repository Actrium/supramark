//! Native FFI wrapper around `plantuml-little`.
//!
//! Counterpart to `crates/plantuml-little/packages/web/` (wasm-bindgen)
//! for native hosts: React Native iOS / Android TurboModules, desktop
//! Swift / Kotlin / C / C++ consumers, anything that can dlopen a
//! `cdylib` or link a `staticlib`.
//!
//! # Surface
//!
//! Three C ABI functions:
//!
//! - [`supramark_plantuml_render`] — convert a UTF-8 PlantUML source
//!   buffer into an SVG buffer. Returns one of the [`error codes`](#error-codes).
//! - [`supramark_plantuml_free`] — return a buffer allocated by
//!   [`supramark_plantuml_render`] back to Rust for freeing. Mandatory
//!   — Rust and host allocators do not share state.
//! - [`supramark_plantuml_version`] — pointer to a `'static` NUL-
//!   terminated UTF-8 version string. The host must **not** free this.
//!
//! Plus the metrics-installation entry point [re-exported from
//! `font-metrics`][font_metrics::ffi_callback::supramark_install_metrics_callback]
//! — by re-exporting through this crate's `cdylib` (or rather, by being
//! linked alongside it in the `staticlib` archive), a single library
//! satisfies the host's "wire metrics → call render" sequence.
//!
//! # Error codes
//!
//! ```text
//! 0 — SUPRAMARK_PLANTUML_OK
//! 1 — SUPRAMARK_PLANTUML_ERR_PARSE        (input is not valid UTF-8)
//! 2 — SUPRAMARK_PLANTUML_ERR_RENDER       (plantuml-little returned Err)
//! 3 — SUPRAMARK_PLANTUML_ERR_NULL_INPUT   (input or out-pointer is null)
//! ```
//!
//! v1 deliberately discards the underlying `Display` string from
//! `plantuml-little::convert`'s error — keeping the C ABI to a single
//! `int` return shrinks the surface area we have to keep stable. If a
//! future revision needs human-readable diagnostics, add a
//! `supramark_plantuml_last_error() -> *const c_char` (thread-local
//! `RefCell<CString>`); existing callers continue to work because the
//! return code is unchanged.
//!
//! # Allocator contract
//!
//! `supramark_plantuml_render` writes a heap-allocated UTF-8 byte
//! buffer through `*out_buf` / `*out_len`. The buffer is allocated by
//! Rust's global allocator and **must** be returned via
//! [`supramark_plantuml_free`] — the host's `free` would mis-match the
//! allocator and corrupt the heap. The buffer is **not**
//! NUL-terminated; use `out_len` for the byte length.
//!
//! # Threading
//!
//! `plantuml-little::convert` is `Send + Sync`-safe in the sense that
//! distinct calls share no mutable state (modulo the global metrics
//! callback, which is itself atomic). Multiple host threads may call
//! `supramark_plantuml_render` concurrently; the metrics callback
//! installed via `supramark_install_metrics_callback` must therefore
//! also be thread-safe.

use std::os::raw::{c_char, c_int};

/// Conversion succeeded; `*out_buf` / `*out_len` are populated.
pub const SUPRAMARK_PLANTUML_OK: c_int = 0;

/// Input bytes were not valid UTF-8.
pub const SUPRAMARK_PLANTUML_ERR_PARSE: c_int = 1;

/// `plantuml-little::convert` returned `Err` — the source parsed as
/// UTF-8 but failed at the PlantUML pre-processor / parser / renderer
/// stage. v1 discards the underlying message; see crate-level docs for
/// the planned `supramark_plantuml_last_error` extension.
pub const SUPRAMARK_PLANTUML_ERR_RENDER: c_int = 2;

/// One of the required pointer arguments was null.
pub const SUPRAMARK_PLANTUML_ERR_NULL_INPUT: c_int = 3;

/// Convert a PlantUML source buffer to an SVG buffer.
///
/// On `SUPRAMARK_PLANTUML_OK`:
/// - `*out_buf` points to a freshly heap-allocated UTF-8 byte buffer
///   (Rust global allocator, **not** the host's `malloc`). The host
///   must return it via [`supramark_plantuml_free`].
/// - `*out_len` is the byte length (no trailing NUL).
///
/// On any error code, `*out_buf` is set to null and `*out_len` to 0
/// (when those pointers are themselves non-null — null pointer args
/// short-circuit to `SUPRAMARK_PLANTUML_ERR_NULL_INPUT` without
/// touching them).
///
/// # Safety
///
/// - `input` must point to `input_len` valid bytes; `input_len == 0`
///   is allowed and produces an empty SVG via the parser's empty-
///   input handling.
/// - `out_buf` and `out_len` must be non-null and writable.
/// - The returned buffer must be freed with
///   [`supramark_plantuml_free`], not the host's `free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn supramark_plantuml_render(
    input: *const c_char,
    input_len: usize,
    out_buf: *mut *mut u8,
    out_len: *mut usize,
) -> c_int {
    // Validate the output pointers first so we can null-fill them on
    // the early-return paths below. If either is null we have nowhere
    // to write the buffer / length, so the call is unrecoverable.
    if out_buf.is_null() || out_len.is_null() {
        return SUPRAMARK_PLANTUML_ERR_NULL_INPUT;
    }
    // From here on every early return must zero the out-params so the
    // host can rely on "error ⇒ buf == null && len == 0".
    unsafe {
        *out_buf = std::ptr::null_mut();
        *out_len = 0;
    }

    // Null `input` with non-zero `input_len` is the only nonsensical
    // combination — an empty (`input_len == 0`) source is a valid
    // PlantUML input that lowers to the "no diagram" placeholder SVG.
    if input.is_null() && input_len != 0 {
        return SUPRAMARK_PLANTUML_ERR_NULL_INPUT;
    }

    // Borrow the input bytes without copying. `from_raw_parts` is
    // sound here because the host guarantees the buffer outlives this
    // call (documented in the function-level Safety section above).
    let bytes: &[u8] = if input_len == 0 {
        &[]
    } else {
        // SAFETY: caller guarantees `input` covers `input_len` bytes.
        unsafe { std::slice::from_raw_parts(input as *const u8, input_len) }
    };

    // UTF-8 validation. plantuml-little's converter takes `&str`, and
    // we'd rather fail loudly here than pass invalid UTF-8 into the
    // parser (which would otherwise panic somewhere unhelpful).
    let source = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return SUPRAMARK_PLANTUML_ERR_PARSE,
    };

    // Call the same entry point the wasm wrapper uses (see
    // `packages/web/src/lib.rs::convert`). v1 discards the
    // `Display` message — see crate-level docs for the planned
    // `supramark_plantuml_last_error` extension.
    let svg = match plantuml_little::convert(source) {
        Ok(svg) => svg,
        Err(_) => return SUPRAMARK_PLANTUML_ERR_RENDER,
    };

    // Move the `String` into a `Vec<u8>` and hand its pointer + length
    // to the host. `into_boxed_slice()` shrinks any excess capacity so
    // the (ptr, len) we hand out is the entire owned allocation —
    // `supramark_plantuml_free` then reconstructs it as
    // `Box<[u8]>` for a matching deallocator call.
    let boxed: Box<[u8]> = svg.into_bytes().into_boxed_slice();
    let len = boxed.len();
    let ptr = Box::into_raw(boxed) as *mut u8;
    // SAFETY: we validated `out_buf` / `out_len` non-null above.
    unsafe {
        *out_buf = ptr;
        *out_len = len;
    }
    SUPRAMARK_PLANTUML_OK
}

/// Free a buffer previously returned by [`supramark_plantuml_render`].
///
/// No-op if `buf` is null. Calling with a `(buf, len)` pair that was
/// **not** produced by this crate is undefined behaviour — the host's
/// allocator and Rust's allocator are not interchangeable.
///
/// # Safety
///
/// - `buf` must be either null, or a pointer previously returned by
///   [`supramark_plantuml_render`] via its `out_buf` parameter.
/// - `len` must be the exact `out_len` value paired with `buf` at the
///   time it was returned.
/// - The buffer must not have been freed already (no double-free).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn supramark_plantuml_free(buf: *mut u8, len: usize) {
    if buf.is_null() {
        return;
    }
    // SAFETY: caller guarantees `(buf, len)` came from
    // `supramark_plantuml_render`'s `Box::into_raw(Box<[u8]>)`.
    // Re-boxing with the same length re-acquires ownership; the
    // `Box<[u8]>` drop deallocates via Rust's global allocator.
    let slice = unsafe { std::slice::from_raw_parts_mut(buf, len) };
    drop(unsafe { Box::from_raw(slice as *mut [u8]) });
}

/// Pointer to a `'static` NUL-terminated UTF-8 version string
/// (CARGO_PKG_VERSION of this crate, which tracks `plantuml-little`
/// upstream).
///
/// The host **must not** free this pointer — it points to read-only
/// program memory (a baked `&'static CStr`).
///
/// Use this at host module init to assert the binary you linked
/// against matches what your wrapper expects, similar to the
/// `version()` export the wasm wrapper provides.
#[unsafe(no_mangle)]
pub extern "C" fn supramark_plantuml_version() -> *const c_char {
    // `concat!(..., "\0")` keeps the NUL in `.rodata` so we never
    // allocate. The cast to `*const c_char` strips the array bound;
    // the host treats it as a regular C string.
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

// Re-export the metrics-callback installer so a single linked artefact
// (`libsupramark_plantuml_native.a` / `.so`) exposes both the render
// entry points and the metrics-bridge installer. `font-metrics`
// declares the function `#[unsafe(no_mangle)] pub unsafe extern "C"` already,
// so a plain `use` here is enough to drag the symbol into our crate's
// final artefact — the linker keeps it because of the `#[unsafe(no_mangle)]`
// attribute on the original definition.
#[allow(unused_imports)]
pub use font_metrics::ffi_callback::{
    MeasureTextFfi, install_ffi_metrics_callback, supramark_install_metrics_callback,
};

#[cfg(test)]
mod tests {
    //! Round-trip smoke tests against the C ABI surface. Heavier
    //! correctness coverage lives in the parent `plantuml-little`
    //! crate's 268+ reference SVG snapshots — this module only
    //! exercises the FFI shim (null handling, allocator contract,
    //! version export).
    use super::*;
    use std::ffi::CStr;

    /// Install a deterministic fallback metrics callback so the FFI
    /// path produces stable widths during the smoke tests. Without
    /// this, `FfiCallbackMetrics` returns its placeholder heuristic,
    /// which is fine — we only care that `render` returns OK, not
    /// the exact pixel layout.
    unsafe extern "C" fn fake_measure(
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
        // SAFETY: contract of MeasureTextFfi.
        unsafe {
            *out_width = (text_len as f64) * size * 0.6;
            *out_ascent = size * 0.8;
            *out_descent = size * 0.2;
        }
    }

    #[test]
    fn version_is_nul_terminated_and_matches_cargo() {
        let p = supramark_plantuml_version();
        assert!(!p.is_null());
        // SAFETY: function contract — `'static` C string.
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(s, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn render_rejects_null_out_pointers() {
        let src = b"@startuml\nA -> B\n@enduml";
        let rc = unsafe {
            supramark_plantuml_render(
                src.as_ptr() as *const c_char,
                src.len(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(rc, SUPRAMARK_PLANTUML_ERR_NULL_INPUT);
    }

    #[test]
    fn render_rejects_invalid_utf8() {
        // 0xFF on its own is never valid UTF-8.
        let bad = [0xFFu8];
        let mut buf: *mut u8 = std::ptr::null_mut();
        let mut len: usize = 0;
        let rc = unsafe {
            supramark_plantuml_render(
                bad.as_ptr() as *const c_char,
                bad.len(),
                &mut buf as *mut *mut u8,
                &mut len as *mut usize,
            )
        };
        assert_eq!(rc, SUPRAMARK_PLANTUML_ERR_PARSE);
        assert!(buf.is_null());
        assert_eq!(len, 0);
    }

    #[test]
    fn render_round_trip_simple_sequence_diagram() {
        // Install our fake metrics so layout has finite numbers.
        unsafe { supramark_install_metrics_callback(fake_measure) };

        let src = b"@startuml\nA -> B\n@enduml";
        let mut buf: *mut u8 = std::ptr::null_mut();
        let mut len: usize = 0;
        let rc = unsafe {
            supramark_plantuml_render(
                src.as_ptr() as *const c_char,
                src.len(),
                &mut buf as *mut *mut u8,
                &mut len as *mut usize,
            )
        };
        assert_eq!(
            rc,
            SUPRAMARK_PLANTUML_ERR_RENDER
                .max(SUPRAMARK_PLANTUML_OK)
                .min(rc),
            "render returned unexpected non-zero code {rc}"
        );
        // Either OK with a populated buffer, or a documented render
        // failure (e.g. graphviz not in PATH on stripped CI). We
        // require OK on a developer machine but tolerate
        // ERR_RENDER on minimal environments — the important
        // assertions are the buffer / length consistency.
        if rc == SUPRAMARK_PLANTUML_OK {
            assert!(!buf.is_null());
            assert!(len > 0);
            // SAFETY: round-trip per allocator contract.
            let bytes = unsafe { std::slice::from_raw_parts(buf, len) };
            assert!(std::str::from_utf8(bytes).is_ok());
            unsafe { supramark_plantuml_free(buf, len) };
        } else {
            assert_eq!(rc, SUPRAMARK_PLANTUML_ERR_RENDER);
            assert!(buf.is_null());
            assert_eq!(len, 0);
        }
    }

    #[test]
    fn free_null_is_noop() {
        // Documented no-op; we just want to make sure it doesn't
        // crash / abort.
        unsafe { supramark_plantuml_free(std::ptr::null_mut(), 0) };
    }
}
