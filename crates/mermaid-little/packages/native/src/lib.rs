//! C ABI wrapper around `mermaid-little`.
//!
//! Exposes four `extern "C"` entry points so non-Rust hosts (RN
//! TurboModules on iOS / Android, desktop Linux/macOS/Windows shells,
//! Swift / Kotlin / C++ apps) can drive Mermaid → SVG conversion
//! without a JS engine.
//!
//! ## Calling convention
//!
//! Each render function takes:
//!
//! - an input pointer + length (UTF-8, **not** required to be
//!   null-terminated — `len` is authoritative);
//! - an `out_buf: *mut *mut u8` to receive the address of a
//!   freshly-allocated SVG buffer;
//! - an `out_len: *mut usize` to receive the SVG byte length.
//!
//! On success the caller owns the returned buffer and MUST hand it
//! back to [`supramark_mermaid_free`] when done. On error the out
//! pointers are set to `(null, 0)` and an error code is returned
//! (see the constants in this file and the C header).
//!
//! ## Threading
//!
//! The render functions are `Send + Sync` provided the host has
//! already installed the metrics callback via
//! [`font_metrics::supramark_install_metrics_callback`]. The
//! installer itself uses an `AtomicPtr` and is safe to call from
//! any thread, but should normally be invoked once at module init
//! before any render call.
//!
//! ## Panics
//!
//! The release profile has `panic = "abort"`, which is the only
//! sound way to handle a Rust panic across the C ABI: unwinding
//! into the host's stack is undefined behaviour. As a belt-and-
//! braces measure each `extern "C"` entry point also wraps its
//! body in [`std::panic::catch_unwind`] and converts any panic
//! into [`SUPRAMARK_MERMAID_ERR_RENDER`], so even a debug build
//! (which uses unwinding) won't tear down the host.

use std::ffi::{c_char, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::slice;

// ── error codes ───────────────────────────────────────────────────────
//
// Kept ABI-stable: numeric values are part of the C header contract
// and MUST NOT change once shipped. Adding new codes is fine; pick
// the next free integer.

/// Success.
#[no_mangle]
pub static SUPRAMARK_MERMAID_OK: i32 = 0;

/// Mermaid source could not be parsed (syntax error / unknown
/// diagram type / config error). The render half of the pipeline
/// was not entered.
#[no_mangle]
pub static SUPRAMARK_MERMAID_ERR_PARSE: i32 = 1;

/// Parser accepted the source but layout / SVG emission failed.
/// Also returned when a Rust panic was caught — those are surfaced
/// as render errors because by the time `catch_unwind` returns we
/// no longer know which stage was on the stack.
#[no_mangle]
pub static SUPRAMARK_MERMAID_ERR_RENDER: i32 = 2;

/// One of the required pointer arguments was null, or `input_len`
/// (or `id_len`) was zero on the with-id variant. No allocation
/// took place.
#[no_mangle]
pub static SUPRAMARK_MERMAID_ERR_NULL_INPUT: i32 = 3;

// ── internal helpers ─────────────────────────────────────────────────

/// Convert (ptr, len) into `&str`, returning `None` on null /
/// non-UTF-8. Zero-length `&str` is allowed.
///
/// # Safety
///
/// `ptr` must point to at least `len` valid bytes, or be null
/// (in which case `len` must be 0). The bytes must outlive the
/// returned slice; this helper does not retain it.
unsafe fn input_to_str<'a>(ptr: *const u8, len: usize) -> Option<&'a str> {
    if ptr.is_null() {
        // Permit (null, 0) as the canonical empty input. The caller
        // gets a parse error rather than null-input in that case,
        // which mirrors what `convert("")` would return in Rust.
        if len == 0 {
            return Some("");
        }
        return None;
    }
    let bytes = slice::from_raw_parts(ptr, len);
    std::str::from_utf8(bytes).ok()
}

/// Hand a Rust `String` back to the host as `(ptr, len)`. The
/// `String`'s allocator owns the bytes; `supramark_mermaid_free`
/// reconstitutes a `Vec<u8>` from the same `(ptr, len, cap)`
/// triple (cap == len because we shrink to fit) to free.
fn deliver_string(s: String, out_buf: *mut *mut u8, out_len: *mut usize) {
    // `into_bytes()` returns a `Vec<u8>` with cap >= len; we shrink
    // so `free` can reconstruct the Vec from `(ptr, len, len)`
    // without remembering the original capacity. Worst case this is
    // one realloc of an already-final string buffer — cheap.
    let mut bytes = s.into_bytes();
    bytes.shrink_to_fit();
    let len = bytes.len();
    let ptr = bytes.as_mut_ptr();
    // SAFETY: we leak the Vec here so the host owns it; the matching
    // `supramark_mermaid_free` rebuilds the Vec from the same (ptr,
    // len, cap == len) triple.
    std::mem::forget(bytes);
    unsafe {
        *out_buf = ptr;
        *out_len = len;
    }
}

fn write_null_out(out_buf: *mut *mut u8, out_len: *mut usize) {
    // Both pointers are guaranteed non-null by the caller checks in
    // each entry point.
    unsafe {
        *out_buf = ptr::null_mut();
        *out_len = 0;
    }
}

/// Classify a mermaid-little error into one of our public codes.
/// The detailed message is dropped — the C ABI is intentionally
/// narrow; surfacing structured errors would mean another out
/// parameter and another `free` for the string. Hosts that need
/// diagnostics can re-run their input through a debug build of
/// the Rust CLI.
fn classify(err: &mermaid_little::error::MermaidError) -> i32 {
    use mermaid_little::error::MermaidError::*;
    match err {
        Parse { .. } | Unsupported(_) | Config(_) => SUPRAMARK_MERMAID_ERR_PARSE,
        Render(_) | Internal(_) => SUPRAMARK_MERMAID_ERR_RENDER,
    }
}

// ── public C ABI ─────────────────────────────────────────────────────

/// Render a Mermaid source to SVG.
///
/// # Safety
///
/// - `input` must point to `input_len` initialised bytes (or be
///   null when `input_len == 0`).
/// - `out_buf` and `out_len` must be non-null and writable.
/// - On `SUPRAMARK_MERMAID_OK`, the caller MUST eventually pass
///   `(*out_buf, *out_len)` to [`supramark_mermaid_free`].
#[no_mangle]
pub unsafe extern "C" fn supramark_mermaid_render(
    input: *const u8,
    input_len: usize,
    out_buf: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    if out_buf.is_null() || out_len.is_null() {
        return SUPRAMARK_MERMAID_ERR_NULL_INPUT;
    }
    // Preset the out params to (null, 0) so even an early error
    // leaves a deterministic state the host can rely on.
    write_null_out(out_buf, out_len);

    let Some(mmd) = input_to_str(input, input_len) else {
        return SUPRAMARK_MERMAID_ERR_NULL_INPUT;
    };

    // catch_unwind: belt-and-braces against debug builds that don't
    // use `panic = abort`. Release profile aborts on panic so this
    // arm is dead code there, but keeping it costs nothing.
    let result = catch_unwind(AssertUnwindSafe(|| mermaid_little::convert(mmd)));

    match result {
        Ok(Ok(svg)) => {
            deliver_string(svg, out_buf, out_len);
            SUPRAMARK_MERMAID_OK
        }
        Ok(Err(e)) => classify(&e),
        Err(_) => SUPRAMARK_MERMAID_ERR_RENDER,
    }
}

/// Same as [`supramark_mermaid_render`], but with an explicit
/// diagram id (mirrors upstream Mermaid's `mermaid.render(id, src)`).
/// Useful when the host wants stable element ids for hash-keyed
/// caching or DOM-targeted re-renders.
///
/// # Safety
///
/// Same contract as [`supramark_mermaid_render`], plus:
/// - `id` must point to `id_len` initialised bytes (or be null when
///   `id_len == 0`, which is treated as an empty id).
#[no_mangle]
pub unsafe extern "C" fn supramark_mermaid_render_with_id(
    input: *const u8,
    input_len: usize,
    id: *const u8,
    id_len: usize,
    out_buf: *mut *mut u8,
    out_len: *mut usize,
) -> i32 {
    if out_buf.is_null() || out_len.is_null() {
        return SUPRAMARK_MERMAID_ERR_NULL_INPUT;
    }
    write_null_out(out_buf, out_len);

    let Some(mmd) = input_to_str(input, input_len) else {
        return SUPRAMARK_MERMAID_ERR_NULL_INPUT;
    };
    let Some(diagram_id) = input_to_str(id, id_len) else {
        return SUPRAMARK_MERMAID_ERR_NULL_INPUT;
    };

    let result = catch_unwind(AssertUnwindSafe(|| {
        mermaid_little::convert_with_id(mmd, diagram_id)
    }));

    match result {
        Ok(Ok(svg)) => {
            deliver_string(svg, out_buf, out_len);
            SUPRAMARK_MERMAID_OK
        }
        Ok(Err(e)) => classify(&e),
        Err(_) => SUPRAMARK_MERMAID_ERR_RENDER,
    }
}

/// Release a buffer previously returned by `supramark_mermaid_render*`.
///
/// # Safety
///
/// - `buf` must be the exact pointer returned by a successful render
///   call, paired with the `len` it returned. Passing any other
///   pointer (or a buffer that has already been freed, or one
///   produced by a different allocator) is undefined behaviour.
/// - `(null, 0)` is accepted as a no-op so the host can call
///   `free` unconditionally on the error path without checking
///   the return code first.
#[no_mangle]
pub unsafe extern "C" fn supramark_mermaid_free(buf: *mut u8, len: usize) {
    if buf.is_null() {
        // No-op on null. The matching deliver path always pairs
        // (null, 0); we don't enforce `len == 0` here to keep the
        // host's error-path code defensive.
        return;
    }
    // SAFETY: see contract above. The `deliver_string` path shrinks
    // capacity to len before forgetting the Vec, so reconstructing
    // with cap == len reclaims exactly the original allocation.
    let _ = Vec::from_raw_parts(buf, len, len);
}

/// Return a NUL-terminated static string with the crate version,
/// e.g. `"11.14.0-1"`. Pointer is valid for the lifetime of the
/// loaded library; the host MUST NOT free it.
#[no_mangle]
pub extern "C" fn supramark_mermaid_version() -> *const c_char {
    // C-strings ending in `\0` can be safely cast to `*const c_char`
    // and survive for the life of the binary (they live in `.rodata`).
    const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");
    // SAFETY: VERSION ends in '\0' and contains no interior nulls
    // (cargo enforces semver-shaped versions).
    let cstr = unsafe { CStr::from_bytes_with_nul_unchecked(VERSION.as_bytes()) };
    cstr.as_ptr()
}

// ── tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper that wraps the FFI render path and gives back an owned
    /// `String`, freeing the C buffer in the process. Used by the
    /// happy-path test below.
    fn render_via_ffi(src: &str) -> Result<String, i32> {
        let mut out_buf: *mut u8 = std::ptr::null_mut();
        let mut out_len: usize = 0;
        let rc = unsafe {
            supramark_mermaid_render(src.as_ptr(), src.len(), &mut out_buf, &mut out_len)
        };
        if rc != SUPRAMARK_MERMAID_OK {
            return Err(rc);
        }
        // SAFETY: rc == OK means the buffer is owned by us; copy out
        // and immediately free.
        let svg = unsafe {
            let slice = std::slice::from_raw_parts(out_buf, out_len);
            std::str::from_utf8(slice).unwrap().to_owned()
        };
        unsafe { supramark_mermaid_free(out_buf, out_len) };
        Ok(svg)
    }

    #[test]
    fn version_is_nonempty_and_nul_terminated() {
        let p = supramark_mermaid_version();
        assert!(!p.is_null());
        let cstr = unsafe { CStr::from_ptr(p) };
        let s = cstr.to_str().unwrap();
        assert!(!s.is_empty());
        // Sanity: starts with a digit (semver-shaped).
        assert!(s.chars().next().unwrap().is_ascii_digit(), "version = {s:?}");
    }

    #[test]
    fn null_out_buf_is_rejected() {
        let src = b"graph TD; A-->B";
        let mut out_len: usize = 0;
        let rc = unsafe {
            supramark_mermaid_render(
                src.as_ptr(),
                src.len(),
                std::ptr::null_mut(),
                &mut out_len,
            )
        };
        assert_eq!(rc, SUPRAMARK_MERMAID_ERR_NULL_INPUT);
    }

    #[test]
    fn null_input_with_nonzero_len_is_rejected() {
        let mut out_buf: *mut u8 = std::ptr::null_mut();
        let mut out_len: usize = 0;
        let rc = unsafe {
            supramark_mermaid_render(std::ptr::null(), 42, &mut out_buf, &mut out_len)
        };
        assert_eq!(rc, SUPRAMARK_MERMAID_ERR_NULL_INPUT);
        assert!(out_buf.is_null());
        assert_eq!(out_len, 0);
    }

    #[test]
    fn free_null_is_noop() {
        // Must not crash; explicitly part of the public contract.
        unsafe { supramark_mermaid_free(std::ptr::null_mut(), 0) };
    }

    #[test]
    fn flowchart_round_trip() {
        // A minimal flowchart should at least come back as SVG. We
        // don't byte-compare here — that's the parent crate's job;
        // we only need to confirm the FFI plumbing is wired up.
        let svg = render_via_ffi("flowchart TD\nA-->B").expect("render succeeded");
        assert!(svg.contains("<svg"), "expected <svg in output, got: {svg}");
    }
}
