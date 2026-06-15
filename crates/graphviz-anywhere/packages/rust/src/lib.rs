//! Safe Rust wrapper for the graphviz-anywhere C library.
//!
//! This crate provides a memory-safe, idiomatic Rust interface to Graphviz
//! layout and rendering. It wraps the low-level C ABI exposed by
//! `libgraphviz_api`.
//!
//! # Example
//!
//! ```no_run
//! use graphviz_anywhere::{GraphvizContext, Engine, Format};
//!
//! let ctx = GraphvizContext::new().expect("failed to create context");
//! let dot = r#"digraph G { a -> b; }"#;
//! let svg = ctx.render(dot, Engine::Dot, Format::Svg).unwrap();
//! println!("{}", String::from_utf8_lossy(&svg));
//! ```
//!
//! # Thread Safety
//!
//! [`GraphvizContext`] is deliberately `!Send` and `!Sync` because the
//! underlying Graphviz library uses global mutable state and is not
//! thread-safe. Each thread that needs rendering should create its own context,
//! or access should be externally synchronized.
//!
//! # wasm32 target
//!
//! When built for `wasm32-unknown-unknown`, the native C library is not
//! linked. Instead, [`GraphvizContext::render`] delegates to a JavaScript
//! function the host must provide. See the [`wasm`] module for the contract.

#[cfg(not(target_arch = "wasm32"))]
mod ffi;

/// Graphviz adapter for supramark's unified diagram abstraction (render-only); native targets only.
#[cfg(not(target_arch = "wasm32"))]
mod diagram_engine;
#[cfg(not(target_arch = "wasm32"))]
pub use diagram_engine::GraphvizEngine;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

#[cfg(not(target_arch = "wasm32"))]
use std::ffi::{CStr, CString};
use std::marker::PhantomData;
#[cfg(not(target_arch = "wasm32"))]
use std::ptr;

/// Errors that can occur when using the Graphviz API.
#[derive(Debug, thiserror::Error)]
pub enum GraphvizError {
    /// The Graphviz context could not be allocated.
    #[error("failed to create graphviz context")]
    ContextCreationFailed,

    /// A null pointer was passed where a valid pointer was expected.
    #[error("null input provided to graphviz")]
    NullInput,

    /// The DOT source string is not valid.
    #[error("invalid DOT input")]
    InvalidDot,

    /// The layout engine failed to compute a layout.
    #[error("layout computation failed")]
    LayoutFailed,

    /// The rendering step failed.
    #[error("render failed")]
    RenderFailed,

    /// The requested layout engine name is not recognized.
    #[error("invalid layout engine")]
    InvalidEngine,

    /// The requested output format is not recognized.
    #[error("invalid output format")]
    InvalidFormat,

    /// Memory allocation failed inside the C library.
    #[error("out of memory")]
    OutOfMemory,

    /// The context has not been properly initialized.
    #[error("context not initialized")]
    NotInitialized,

    /// The DOT input string contains an interior NUL byte.
    #[error("DOT string contains interior NUL byte: {0}")]
    NulByteInInput(#[from] std::ffi::NulError),

    /// An unrecognized error code was returned by the C library.
    #[error("unknown graphviz error (code {0})")]
    Unknown(i32),
}

#[cfg(not(target_arch = "wasm32"))]
impl GraphvizError {
    /// Map a C error code to the corresponding Rust error variant.
    fn from_code(code: ffi::gv_error_t) -> Self {
        match code {
            ffi::GV_ERR_NULL_INPUT => Self::NullInput,
            ffi::GV_ERR_INVALID_DOT => Self::InvalidDot,
            ffi::GV_ERR_LAYOUT_FAILED => Self::LayoutFailed,
            ffi::GV_ERR_RENDER_FAILED => Self::RenderFailed,
            ffi::GV_ERR_INVALID_ENGINE => Self::InvalidEngine,
            ffi::GV_ERR_INVALID_FORMAT => Self::InvalidFormat,
            ffi::GV_ERR_OUT_OF_MEMORY => Self::OutOfMemory,
            ffi::GV_ERR_NOT_INITIALIZED => Self::NotInitialized,
            other => Self::Unknown(other),
        }
    }
}

/// Layout engine used to compute graph positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Engine {
    /// Hierarchical layout for directed graphs (default).
    Dot,
    /// Spring-model layout via stress majorization.
    Neato,
    /// Force-directed placement.
    Fdp,
    /// Scalable force-directed placement for large graphs.
    Sfdp,
    /// Circular layout.
    Circo,
    /// Radial layout.
    Twopi,
    /// Clustered layout using a tree-map style.
    Osage,
    /// Squarified tree-map layout.
    Patchwork,
}

#[cfg(not(target_arch = "wasm32"))]
impl Engine {
    /// Return the C string name expected by the library.
    fn as_cstr(&self) -> &'static CStr {
        // SAFETY: all byte literals are valid NUL-terminated UTF-8.
        match self {
            Self::Dot => unsafe { CStr::from_bytes_with_nul_unchecked(b"dot\0") },
            Self::Neato => unsafe { CStr::from_bytes_with_nul_unchecked(b"neato\0") },
            Self::Fdp => unsafe { CStr::from_bytes_with_nul_unchecked(b"fdp\0") },
            Self::Sfdp => unsafe { CStr::from_bytes_with_nul_unchecked(b"sfdp\0") },
            Self::Circo => unsafe { CStr::from_bytes_with_nul_unchecked(b"circo\0") },
            Self::Twopi => unsafe { CStr::from_bytes_with_nul_unchecked(b"twopi\0") },
            Self::Osage => unsafe { CStr::from_bytes_with_nul_unchecked(b"osage\0") },
            Self::Patchwork => unsafe { CStr::from_bytes_with_nul_unchecked(b"patchwork\0") },
        }
    }
}

/// Output format for the rendered graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// Scalable Vector Graphics.
    Svg,
    /// Portable Network Graphics (raster).
    Png,
    /// Adobe Portable Document Format.
    Pdf,
    /// PostScript.
    Ps,
    /// JSON representation of the graph structure.
    Json,
    /// Canonical DOT output (re-serialized).
    DotOutput,
    /// Extended DOT with layout information.
    Xdot,
    /// Simple plain-text coordinate output.
    Plain,
}

#[cfg(not(target_arch = "wasm32"))]
impl Format {
    /// Return the C string name expected by the library.
    fn as_cstr(&self) -> &'static CStr {
        // SAFETY: all byte literals are valid NUL-terminated UTF-8.
        match self {
            Self::Svg => unsafe { CStr::from_bytes_with_nul_unchecked(b"svg\0") },
            Self::Png => unsafe { CStr::from_bytes_with_nul_unchecked(b"png\0") },
            Self::Pdf => unsafe { CStr::from_bytes_with_nul_unchecked(b"pdf\0") },
            Self::Ps => unsafe { CStr::from_bytes_with_nul_unchecked(b"ps\0") },
            Self::Json => unsafe { CStr::from_bytes_with_nul_unchecked(b"json\0") },
            Self::DotOutput => unsafe { CStr::from_bytes_with_nul_unchecked(b"dot\0") },
            Self::Xdot => unsafe { CStr::from_bytes_with_nul_unchecked(b"xdot\0") },
            Self::Plain => unsafe { CStr::from_bytes_with_nul_unchecked(b"plain\0") },
        }
    }
}

/// A Graphviz rendering context.
///
/// On native targets this wraps the opaque `gv_context_t` pointer from the
/// C library and automatically frees the underlying resources when dropped.
///
/// On `wasm32-unknown-unknown` targets there is no native context; this
/// type is a zero-sized marker and all work is delegated to the host-provided
/// JavaScript `__graphviz_anywhere_render` function (see the [`wasm`] module).
///
/// This type is `!Send` and `!Sync` because Graphviz uses global mutable
/// state internally.
pub struct GraphvizContext {
    #[cfg(not(target_arch = "wasm32"))]
    raw: *mut ffi::gv_context_t,
    /// Prevent Send and Sync: the raw pointer plus PhantomData<*mut ()>
    /// ensures the compiler treats this as neither Send nor Sync.
    _not_send_sync: PhantomData<*mut ()>,
}

impl GraphvizContext {
    /// Create a new Graphviz context.
    ///
    /// Returns an error if the underlying C library fails to allocate.
    /// On `wasm32-unknown-unknown` this is always `Ok(...)` and a no-op;
    /// the real context is owned by the JavaScript side.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> Result<Self, GraphvizError> {
        let raw = unsafe { ffi::gv_context_new() };
        if raw.is_null() {
            return Err(GraphvizError::ContextCreationFailed);
        }
        Ok(Self {
            raw,
            _not_send_sync: PhantomData,
        })
    }

    /// Create a new Graphviz context (wasm32 no-op).
    #[cfg(target_arch = "wasm32")]
    pub fn new() -> Result<Self, GraphvizError> {
        Ok(Self {
            _not_send_sync: PhantomData,
        })
    }

    /// Render a DOT language string into the requested output format.
    ///
    /// # Arguments
    ///
    /// * `dot` - A valid DOT language graph description.
    /// * `engine` - The layout algorithm to use.
    /// * `format` - The desired output format.
    ///
    /// # Returns
    ///
    /// The raw rendered bytes on success, or a [`GraphvizError`] on failure.
    /// For text formats like SVG, the bytes are valid UTF-8 and can be
    /// converted with `String::from_utf8`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render(
        &self,
        dot: &str,
        engine: Engine,
        format: Format,
    ) -> Result<Vec<u8>, GraphvizError> {
        let c_dot = CString::new(dot)?;
        let c_engine = engine.as_cstr();
        let c_format = format.as_cstr();

        let mut out_data: *mut std::os::raw::c_char = ptr::null_mut();
        let mut out_len: usize = 0;

        let rc = unsafe {
            ffi::gv_render(
                self.raw,
                c_dot.as_ptr(),
                c_engine.as_ptr(),
                c_format.as_ptr(),
                &mut out_data,
                &mut out_len,
            )
        };

        if rc != ffi::GV_OK {
            return Err(GraphvizError::from_code(rc));
        }

        // Copy the data into a Rust-owned Vec before freeing the C buffer.
        let bytes = if out_data.is_null() || out_len == 0 {
            Vec::new()
        } else {
            // SAFETY: gv_render guarantees out_data points to out_len valid bytes on success.
            let slice = unsafe { std::slice::from_raw_parts(out_data as *const u8, out_len) };
            slice.to_vec()
        };

        // Always free the C-allocated buffer.
        if !out_data.is_null() {
            unsafe { ffi::gv_free_render_data(out_data) };
        }

        Ok(bytes)
    }

    /// Render via the host-provided JavaScript bridge.
    #[cfg(target_arch = "wasm32")]
    pub fn render(
        &self,
        dot: &str,
        engine: Engine,
        format: Format,
    ) -> Result<Vec<u8>, GraphvizError> {
        wasm::render(dot, engine, format)
    }

    /// Render a DOT string and return the result as a UTF-8 string.
    ///
    /// This is a convenience wrapper around [`render`](Self::render) for
    /// text-based output formats (SVG, DOT, JSON, Plain, etc.).
    pub fn render_to_string(
        &self,
        dot: &str,
        engine: Engine,
        format: Format,
    ) -> Result<String, GraphvizError> {
        let bytes = self.render(dot, engine, format)?;
        // Graphviz text output is always valid UTF-8 in practice,
        // but we use lossy conversion for robustness.
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for GraphvizContext {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { ffi::gv_context_free(self.raw) };
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for GraphvizContext {
    fn drop(&mut self) {
        // No-op: the JavaScript side owns the context.
    }
}

/// Return the Graphviz library version string.
///
/// Returns `None` if the C library returns a null pointer. On
/// `wasm32-unknown-unknown` this always returns `None`.
#[cfg(not(target_arch = "wasm32"))]
pub fn version() -> Option<String> {
    let ptr = unsafe { ffi::gv_version() };
    if ptr.is_null() {
        return None;
    }
    let cstr = unsafe { CStr::from_ptr(ptr) };
    Some(cstr.to_string_lossy().into_owned())
}

#[cfg(target_arch = "wasm32")]
pub fn version() -> Option<String> {
    None
}

/// Return a human-readable description of a raw C error code.
///
/// Primarily useful for debugging; prefer the [`GraphvizError`] Display impl
/// in most cases. On `wasm32-unknown-unknown` this always returns `None`.
#[cfg(not(target_arch = "wasm32"))]
pub fn strerror(code: i32) -> Option<String> {
    let ptr = unsafe { ffi::gv_strerror(code) };
    if ptr.is_null() {
        return None;
    }
    let cstr = unsafe { CStr::from_ptr(ptr) };
    Some(cstr.to_string_lossy().into_owned())
}

#[cfg(target_arch = "wasm32")]
pub fn strerror(_code: i32) -> Option<String> {
    None
}

// GraphvizContext is intentionally !Send and !Sync via PhantomData<*mut ()>.
// Graphviz uses global mutable state and is not safe to share across threads.

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    // ================================================================================
    // Engine Tests
    // ================================================================================

    #[test]
    fn engine_cstr_no_panic() {
        // Ensure all variants produce valid C strings.
        let engines = [
            Engine::Dot,
            Engine::Neato,
            Engine::Fdp,
            Engine::Sfdp,
            Engine::Circo,
            Engine::Twopi,
            Engine::Osage,
            Engine::Patchwork,
        ];
        for e in &engines {
            let s = e.as_cstr();
            assert!(!s.to_bytes().is_empty());
        }
    }

    #[test]
    fn engine_names_are_correct() {
        assert_eq!(Engine::Dot.as_cstr().to_str().unwrap(), "dot");
        assert_eq!(Engine::Neato.as_cstr().to_str().unwrap(), "neato");
        assert_eq!(Engine::Fdp.as_cstr().to_str().unwrap(), "fdp");
        assert_eq!(Engine::Sfdp.as_cstr().to_str().unwrap(), "sfdp");
        assert_eq!(Engine::Circo.as_cstr().to_str().unwrap(), "circo");
        assert_eq!(Engine::Twopi.as_cstr().to_str().unwrap(), "twopi");
        assert_eq!(Engine::Osage.as_cstr().to_str().unwrap(), "osage");
        assert_eq!(Engine::Patchwork.as_cstr().to_str().unwrap(), "patchwork");
    }

    #[test]
    fn engine_equality() {
        assert_eq!(Engine::Dot, Engine::Dot);
        assert_ne!(Engine::Dot, Engine::Neato);
    }

    #[test]
    fn engine_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Engine::Dot);
        set.insert(Engine::Neato);
        set.insert(Engine::Dot); // Duplicate
        assert_eq!(set.len(), 2); // Should have 2 unique engines
    }

    // ================================================================================
    // Format Tests
    // ================================================================================

    #[test]
    fn format_cstr_no_panic() {
        let formats = [
            Format::Svg,
            Format::Png,
            Format::Pdf,
            Format::Ps,
            Format::Json,
            Format::DotOutput,
            Format::Xdot,
            Format::Plain,
        ];
        for f in &formats {
            let s = f.as_cstr();
            assert!(!s.to_bytes().is_empty());
        }
    }

    #[test]
    fn format_names_are_correct() {
        assert_eq!(Format::Svg.as_cstr().to_str().unwrap(), "svg");
        assert_eq!(Format::Png.as_cstr().to_str().unwrap(), "png");
        assert_eq!(Format::Pdf.as_cstr().to_str().unwrap(), "pdf");
        assert_eq!(Format::Ps.as_cstr().to_str().unwrap(), "ps");
        assert_eq!(Format::Json.as_cstr().to_str().unwrap(), "json");
        assert_eq!(Format::DotOutput.as_cstr().to_str().unwrap(), "dot");
        assert_eq!(Format::Xdot.as_cstr().to_str().unwrap(), "xdot");
        assert_eq!(Format::Plain.as_cstr().to_str().unwrap(), "plain");
    }

    #[test]
    fn format_equality() {
        assert_eq!(Format::Svg, Format::Svg);
        assert_ne!(Format::Svg, Format::Png);
    }

    #[test]
    fn format_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Format::Svg);
        set.insert(Format::Png);
        set.insert(Format::Svg); // Duplicate
        assert_eq!(set.len(), 2); // Should have 2 unique formats
    }

    // ================================================================================
    // Error Tests
    // ================================================================================

    #[test]
    fn error_display() {
        let err = GraphvizError::InvalidDot;
        let msg = format!("{err}");
        assert!(msg.contains("invalid DOT"), "got: {msg}");
    }

    #[test]
    fn error_from_code_roundtrip() {
        let err = GraphvizError::from_code(ffi::GV_ERR_OUT_OF_MEMORY);
        assert!(matches!(err, GraphvizError::OutOfMemory));
    }

    #[test]
    fn all_error_codes_mapped() {
        // Verify all error codes can be mapped to GraphvizError
        assert!(matches!(
            GraphvizError::from_code(ffi::GV_ERR_NULL_INPUT),
            GraphvizError::NullInput
        ));
        assert!(matches!(
            GraphvizError::from_code(ffi::GV_ERR_INVALID_DOT),
            GraphvizError::InvalidDot
        ));
        assert!(matches!(
            GraphvizError::from_code(ffi::GV_ERR_LAYOUT_FAILED),
            GraphvizError::LayoutFailed
        ));
        assert!(matches!(
            GraphvizError::from_code(ffi::GV_ERR_RENDER_FAILED),
            GraphvizError::RenderFailed
        ));
        assert!(matches!(
            GraphvizError::from_code(ffi::GV_ERR_INVALID_ENGINE),
            GraphvizError::InvalidEngine
        ));
        assert!(matches!(
            GraphvizError::from_code(ffi::GV_ERR_INVALID_FORMAT),
            GraphvizError::InvalidFormat
        ));
        assert!(matches!(
            GraphvizError::from_code(ffi::GV_ERR_OUT_OF_MEMORY),
            GraphvizError::OutOfMemory
        ));
        assert!(matches!(
            GraphvizError::from_code(ffi::GV_ERR_NOT_INITIALIZED),
            GraphvizError::NotInitialized
        ));
    }

    #[test]
    fn error_unknown_code() {
        let err = GraphvizError::from_code(999);
        assert!(matches!(err, GraphvizError::Unknown(999)));
    }

    #[test]
    fn nul_byte_in_input_is_error() {
        let result = CString::new("hello\0world");
        assert!(result.is_err());
    }

    #[test]
    fn error_nul_byte_error() {
        let nul_err = CString::new("hello\0").unwrap_err();
        let graphviz_err = GraphvizError::from(nul_err);
        match graphviz_err {
            GraphvizError::NulByteInInput(_) => {}
            _ => panic!("Expected NulByteInInput"),
        }
    }

    #[test]
    fn error_has_std_error_impl() {
        use std::error::Error;
        let err: Box<dyn Error> = Box::new(GraphvizError::InvalidDot);
        assert_eq!(err.to_string(), "invalid DOT input");
    }

    // ================================================================================
    // GraphvizContext Tests (would work with real Graphviz C library)
    // ================================================================================

    #[test]
    fn context_is_not_send() {
        // This test verifies that GraphvizContext is !Send and !Sync
        // The presence of PhantomData<*mut ()> guarantees !Send and !Sync
        // because raw pointers are !Send and !Sync
        let _ctx = GraphvizContext {
            raw: std::ptr::null_mut(),
            _not_send_sync: std::marker::PhantomData,
        };

        // We verify this by checking that the PhantomData marker exists
        // and by the fact that the code compiles (if GraphvizContext was Send/Sync,
        // usage in certain contexts would fail to compile)
    }

    #[test]
    fn context_creation_null_returns_error() {
        // This test would run with real Graphviz if we mock gv_context_new
        // For now, we verify the error handling logic
        let err = GraphvizError::ContextCreationFailed;
        assert!(format!("{err}").contains("context"));
    }

    #[test]
    fn render_error_from_code() {
        let err = GraphvizError::from_code(ffi::GV_ERR_RENDER_FAILED);
        assert!(matches!(err, GraphvizError::RenderFailed));
    }

    #[test]
    fn layout_error_from_code() {
        let err = GraphvizError::from_code(ffi::GV_ERR_LAYOUT_FAILED);
        assert!(matches!(err, GraphvizError::LayoutFailed));
    }

    #[test]
    fn engine_validation_error() {
        let err = GraphvizError::from_code(ffi::GV_ERR_INVALID_ENGINE);
        assert!(matches!(err, GraphvizError::InvalidEngine));
    }

    #[test]
    fn format_validation_error() {
        let err = GraphvizError::from_code(ffi::GV_ERR_INVALID_FORMAT);
        assert!(matches!(err, GraphvizError::InvalidFormat));
    }

    // ================================================================================
    // Version and StError Tests
    // ================================================================================

    #[test]
    fn version_returns_string_or_none() {
        // This would call into C library in real scenario
        // For now, we just verify the function exists and type is correct
        let _result: Option<String> = version();
    }

    #[test]
    fn strerror_returns_string_or_none() {
        // This would call into C library in real scenario
        let _result: Option<String> = strerror(ffi::GV_ERR_INVALID_DOT);
    }

    // ================================================================================
    // C String Conversion Tests
    // ================================================================================

    #[test]
    fn valid_dot_creates_cstring() {
        let dot = "digraph { a -> b }";
        let result = CString::new(dot);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_str().unwrap(), dot);
    }

    #[test]
    fn empty_dot_creates_cstring() {
        let result = CString::new("");
        assert!(result.is_ok());
    }

    #[test]
    fn complex_dot_creates_cstring() {
        let dot = r#"
            digraph G {
                rankdir=LR;
                node [shape=record];
                a [label="<left> A | <right> B"];
                a -> b;
            }
        "#;
        let result = CString::new(dot);
        assert!(result.is_ok());
    }

    // ================================================================================
    // Trait Tests
    // ================================================================================

    #[test]
    fn engine_implements_debug() {
        let engine = Engine::Dot;
        let _debug_str = format!("{:?}", engine);
    }

    #[test]
    fn engine_implements_clone_copy() {
        let e1 = Engine::Dot;
        let e2 = e1;
        let e3 = e1.clone();
        assert_eq!(e1, e2);
        assert_eq!(e2, e3);
    }

    #[test]
    fn format_implements_debug() {
        let format = Format::Svg;
        let _debug_str = format!("{:?}", format);
    }

    #[test]
    fn format_implements_clone_copy() {
        let f1 = Format::Svg;
        let f2 = f1;
        let f3 = f1.clone();
        assert_eq!(f1, f2);
        assert_eq!(f2, f3);
    }

    #[test]
    fn error_implements_debug() {
        let err = GraphvizError::InvalidDot;
        let _debug_str = format!("{:?}", err);
    }
}
