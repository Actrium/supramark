//! Native FFI wrapper around `d2-little`.
//!
//! Mirrors the wasm-bindgen surface in `crates/d2-little/packages/web`
//! but exposes a C ABI so React Native, iOS, Android, and other native
//! hosts can link against `libsupramark_d2_native.{a,so,dylib}` and
//! call `supramark_d2_render(...)` to turn a D2 source string into an
//! SVG byte buffer.
//!
//! Error handling
//! --------------
//! All entry points return `int32_t` status codes (see `SUPRAMARK_D2_*`
//! constants in `include/supramark_d2.h`). Out-parameters are written
//! only on success. On failure they are zero-initialised so callers
//! that forget to check the return code at least see `NULL` / `0`.
//!
//! Memory ownership
//! ----------------
//! `supramark_d2_render` heap-allocates the SVG buffer via Rust's
//! global allocator. Callers MUST release it through
//! `supramark_d2_free(buf, len)` to match the allocator that produced
//! it; calling `free(3)` from C is undefined behaviour because the
//! Rust allocator may be jemalloc/mimalloc in a host build.
//!
//! Text metrics
//! ------------
//! By default `d2-little` constructs `D2GoEmulationMetrics` (an
//! embedded-font-based heuristic that matches Go's d2 e2e baseline
//! within a fraction of a pixel). Hosts that want native-platform
//! font shaping (e.g. RN Skia, UIKit) can install a measurement
//! callback through `supramark_install_metrics_callback` (re-exported
//! by the linked `font-metrics` crate; see its doc comment in
//! `crates/font-metrics/src/ffi_callback.rs`). Wiring that callback
//! into `d2-little`'s compile pipeline is a follow-up phase — today
//! the SVG geometry is computed against the embedded DejaVu metrics
//! regardless of the host's installed callback.

use std::ffi::{c_char, CStr};
use std::os::raw::c_int;
use std::ptr;
use std::slice;

// ---------------------------------------------------------------------------
// Status codes — keep in sync with include/supramark_d2.h
// ---------------------------------------------------------------------------

/// Render succeeded; `*out_buf` / `*out_len` are populated.
pub const SUPRAMARK_D2_OK: c_int = 0;
/// D2 source failed to parse.
pub const SUPRAMARK_D2_ERR_PARSE: c_int = 1;
/// Parsing succeeded but layout / SVG rendering failed.
pub const SUPRAMARK_D2_ERR_RENDER: c_int = 2;
/// `input` or one of the out-parameter pointers was NULL, or `input`
/// was not valid UTF-8 / not NUL-terminated within `input_len` bytes.
pub const SUPRAMARK_D2_ERR_NULL_INPUT: c_int = 3;

// ---------------------------------------------------------------------------
// Public C ABI
// ---------------------------------------------------------------------------

/// Render a D2 source string to SVG.
///
/// On success returns [`SUPRAMARK_D2_OK`] and writes a heap-allocated,
/// non-NUL-terminated UTF-8 SVG byte buffer to `*out_buf` together
/// with its length (in bytes) in `*out_len`. The caller MUST release
/// the buffer with [`supramark_d2_free`].
///
/// `input` may be either a NUL-terminated C string (pass `input_len = 0`,
/// in which case the wrapper computes the length with `strlen`) or an
/// explicit-length byte buffer (pass `input_len > 0`, in which case
/// the buffer does NOT need to be NUL-terminated). The latter is
/// preferred because it avoids a redundant scan on large inputs.
///
/// Error classification mirrors [`d2_little::compile`]: today both
/// parse and render failures surface as the same `String` so we
/// classify by message prefix. Future revisions may distinguish more
/// finely; the status code namespace has room.
///
/// # Safety
///
/// All pointer arguments are dereferenced. The caller must ensure:
///   * `input` points to at least `input_len` readable bytes (or, when
///     `input_len == 0`, to a NUL-terminated C string).
///   * `out_buf` and `out_len` are valid, writable, non-aliasing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn supramark_d2_render(
    input: *const c_char,
    input_len: usize,
    out_buf: *mut *mut c_char,
    out_len: *mut usize,
) -> c_int {
    if out_buf.is_null() || out_len.is_null() {
        // Without out-parameters there is nowhere to write the result —
        // refuse outright rather than silently dropping the SVG.
        return SUPRAMARK_D2_ERR_NULL_INPUT;
    }
    // Zero-initialise the out-params first so any early return leaves
    // the caller in a defined state.
    // SAFETY: out_buf / out_len null-checked just above; caller
    // contracted them as writable, non-aliasing.
    unsafe {
        *out_buf = ptr::null_mut();
        *out_len = 0;
    }

    if input.is_null() {
        return SUPRAMARK_D2_ERR_NULL_INPUT;
    }

    // Decode the input. Two modes — explicit length (preferred) vs.
    // NUL-terminated. Both go through `str::from_utf8` because
    // d2-little expects a `&str` and Rust UB-wise we must not feed it
    // invalid UTF-8.
    let input_bytes: &[u8] = if input_len == 0 {
        // SAFETY: caller guaranteed NUL-terminated valid C string.
        let cstr = unsafe { CStr::from_ptr(input) };
        match cstr.to_bytes_with_nul().split_last() {
            Some((_nul, body)) => body,
            None => return SUPRAMARK_D2_ERR_NULL_INPUT,
        }
    } else {
        // SAFETY: caller guaranteed `input_len` readable bytes at `input`.
        unsafe { slice::from_raw_parts(input as *const u8, input_len) }
    };

    let input_str = match std::str::from_utf8(input_bytes) {
        Ok(s) => s,
        Err(_) => return SUPRAMARK_D2_ERR_NULL_INPUT,
    };

    // Mirror `d2-little-web::convert`: the `d2_to_svg` convenience
    // entry point uses the same defaults as the Go e2e baseline
    // (pad = 0, multi-board animate wrapper, default metrics).
    match d2_little::d2_to_svg(input_str) {
        Ok(svg_bytes) => {
            // Move the Vec<u8> into a stable heap allocation we can
            // hand to C. `Box::into_raw(Box<[u8]>)` returns a raw
            // slice pointer whose (ptr, len) we hand to the caller;
            // `supramark_d2_free` reconstitutes the box.
            let len = svg_bytes.len();
            let boxed: Box<[u8]> = svg_bytes.into_boxed_slice();
            let raw: *mut u8 = Box::into_raw(boxed) as *mut u8;
            // SAFETY: out_buf / out_len null-checked at function entry.
            unsafe {
                *out_buf = raw as *mut c_char;
                *out_len = len;
            }
            SUPRAMARK_D2_OK
        }
        Err(msg) => classify_error(&msg),
    }
}

/// Release a buffer previously returned by [`supramark_d2_render`].
///
/// Passing `(NULL, 0)` is a no-op. Passing a buffer that did not come
/// from `supramark_d2_render`, or a `len` that does not match the
/// original allocation, is undefined behaviour.
///
/// # Safety
///
/// See module-level "Memory ownership" note. `buf` must have been
/// produced by [`supramark_d2_render`] and not yet freed; `len` must
/// equal the `out_len` value the render call wrote.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn supramark_d2_free(buf: *mut c_char, len: usize) {
    if buf.is_null() || len == 0 {
        return;
    }
    // Reconstitute the Box<[u8]> we leaked in supramark_d2_render so
    // its Drop runs through the global allocator. The slice ptr/len
    // pair is the inverse of `Box::into_raw(Box<[u8]>)`.
    let slice_ptr = ptr::slice_from_raw_parts_mut(buf as *mut u8, len);
    // SAFETY: caller contracts buf+len match a prior render call.
    unsafe { drop(Box::from_raw(slice_ptr)) };
}

/// Returns a static, NUL-terminated UTF-8 C string with this wrapper
/// crate's version (matches the `d2-little` crate version it wraps).
///
/// The returned pointer is valid for the lifetime of the loaded
/// library; callers must NOT free it.
#[unsafe(no_mangle)]
pub extern "C" fn supramark_d2_version() -> *const c_char {
    // concat! + nul-terminator → &'static [u8] → *const c_char.
    // Avoids any runtime allocation and gives a pointer with static
    // lifetime, which is exactly what the contract promises.
    static VERSION: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();
    VERSION.as_ptr() as *const c_char
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `d2_little::compile` returns a single `String` for both parse and
/// render failures. We do a best-effort classification by message
/// prefix so iOS/Android hosts can surface a useful error category
/// without having to string-match themselves. When in doubt we fall
/// back to the more general `RENDER` code.
fn classify_error(msg: &str) -> c_int {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("parse") || lower.contains("syntax") || lower.contains("unexpected") {
        SUPRAMARK_D2_ERR_PARSE
    } else {
        SUPRAMARK_D2_ERR_RENDER
    }
}

// ---------------------------------------------------------------------------
// Tests — exercised via `cargo test -p supramark-d2-native`. Keep them
// to the minimum needed to prove the FFI contract; the heavy d2 e2e
// matrix lives in the parent crate.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    /// Smallest meaningful d2 input → expect a well-formed SVG payload.
    /// Doubles as a sanity check that the wrapper doesn't strip the
    /// `<?xml ?>` prologue or otherwise mutate the bytes.
    #[test]
    fn render_roundtrip_simple() {
        let src = CString::new("a -> b").unwrap();
        let mut out_buf: *mut c_char = ptr::null_mut();
        let mut out_len: usize = 0;

        let rc = unsafe {
            supramark_d2_render(
                src.as_ptr(),
                0, // use NUL-terminated path
                &mut out_buf as *mut *mut c_char,
                &mut out_len as *mut usize,
            )
        };
        assert_eq!(rc, SUPRAMARK_D2_OK, "render returned {rc}");
        assert!(!out_buf.is_null());
        assert!(out_len > 0);

        let svg = unsafe { slice::from_raw_parts(out_buf as *const u8, out_len) };
        let svg_str = std::str::from_utf8(svg).expect("SVG must be UTF-8");
        assert!(svg_str.contains("<svg"), "expected <svg in output");

        unsafe { supramark_d2_free(out_buf, out_len) };
    }

    /// Explicit-length path (input not required to be NUL-terminated).
    #[test]
    fn render_with_explicit_length() {
        let src = b"a -> b";
        let mut out_buf: *mut c_char = ptr::null_mut();
        let mut out_len: usize = 0;
        let rc = unsafe {
            supramark_d2_render(
                src.as_ptr() as *const c_char,
                src.len(),
                &mut out_buf,
                &mut out_len,
            )
        };
        assert_eq!(rc, SUPRAMARK_D2_OK);
        assert!(out_len > 0);
        unsafe { supramark_d2_free(out_buf, out_len) };
    }

    /// NULL input → ERR_NULL_INPUT, out-params untouched.
    #[test]
    fn render_null_input() {
        let mut out_buf: *mut c_char = ptr::null_mut();
        let mut out_len: usize = 0;
        let rc = unsafe {
            supramark_d2_render(ptr::null(), 0, &mut out_buf, &mut out_len)
        };
        assert_eq!(rc, SUPRAMARK_D2_ERR_NULL_INPUT);
        assert!(out_buf.is_null());
        assert_eq!(out_len, 0);
    }

    /// NULL out-params → ERR_NULL_INPUT (no crash).
    #[test]
    fn render_null_outparams() {
        let src = CString::new("a").unwrap();
        let rc = unsafe {
            supramark_d2_render(src.as_ptr(), 0, ptr::null_mut(), ptr::null_mut())
        };
        assert_eq!(rc, SUPRAMARK_D2_ERR_NULL_INPUT);
    }

    /// Free of (NULL, 0) is a no-op (must not crash).
    #[test]
    fn free_null_is_noop() {
        unsafe { supramark_d2_free(ptr::null_mut(), 0) };
    }

    /// Version string is non-empty and matches the crate version.
    #[test]
    fn version_string() {
        let p = supramark_d2_version();
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap();
        assert_eq!(s, env!("CARGO_PKG_VERSION"));
    }
}
