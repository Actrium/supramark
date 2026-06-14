//! Host-side text-measurement callback bridge.
//!
//! When the wasm module runs inside a browser or React Native host
//! that already has a real text renderer, the cleanest way to keep
//! Layer 1 (in-wasm layout) consistent with Layer 3 (browser /
//! RN-svg actual rendering) is to defer measurement to the host.
//! The host injects a callback (e.g. wrapping
//! `canvas.getContext('2d').measureText` or
//! `react-native-skia.Skia.Text.Measure`) at module init via
//! wasm-bindgen externs; this struct adapts those externs to the
//! [`crate::Metrics`] trait.
//!
//! # Host contract
//!
//! The host is expected to install, before any wasm rendering call:
//!
//! ```js
//! globalThis.supramark = {
//!   measureText: (family, text, size, bold) => ({
//!     width:   /* number */,
//!     ascent:  /* number, optional */,
//!     descent: /* number, optional */,
//!   })
//! };
//! ```
//!
//! Italic isn't passed because canvas/Skia bridges typically lump
//! italic with style strings; we'll add it later if needed. If the
//! bridge isn't installed (or throws), every measurement falls back
//! to a `size * 0.6`-per-char heuristic — downstream code should
//! treat unusually round numbers as a hint that the host bridge
//! wasn't wired up rather than a real measurement.

#![cfg(target_arch = "wasm32")]

use crate::{Measured, Metrics};
use js_sys::{Object, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;

#[wasm_bindgen]
extern "C" {
    /// Host-supplied text-measurement bridge. The JS side should
    /// install
    /// `globalThis.supramark = { measureText: (family, text, size, bold) => ({width, ascent, descent}) }`
    /// before any wasm rendering call. The bridge typically wraps
    /// `canvas.getContext('2d').measureText(text)` or the
    /// `react-native-skia` equivalent.
    #[wasm_bindgen(js_namespace = supramark, js_name = measureText, catch)]
    fn js_measure_text(family: &str, text: &str, size: f64, bold: bool)
        -> Result<JsValue, JsValue>;
}

/// Adapter that defers every measurement to a host-supplied callback
/// (e.g. browser `canvas.measureText`, RN-Skia `SkiaText.measureText`).
///
/// Stateless / zero-cost — every method dispatches through the
/// `extern "C"` bridge above. If the host hasn't installed
/// `supramark.measureText`, every call returns the placeholder
/// fallback values; downstream code should treat that as a sign the
/// host bridge wasn't wired up rather than a real measurement.
#[derive(Debug, Clone, Copy, Default)]
pub struct HostCallbackMetrics;

#[derive(Debug, Clone, Copy)]
struct MeasuredBox {
    width: f64,
    ascent: f64,
    descent: f64,
}

impl HostCallbackMetrics {
    fn measure_box(&self, text: &str, family: &str, size: f64, bold: bool) -> MeasuredBox {
        match js_measure_text(family, text, size, bold) {
            Ok(value) => parse_box(&value, size).unwrap_or_else(|| fallback_box(text, size)),
            Err(_) => fallback_box(text, size),
        }
    }
}

fn parse_box(value: &JsValue, size: f64) -> Option<MeasuredBox> {
    let obj = value.dyn_ref::<Object>()?;
    let width = read_f64(obj, "width")?;
    // ascent / descent are optional on some hosts (e.g. older Safari);
    // fall back to size-based estimates rather than zero.
    let ascent = read_f64(obj, "ascent").unwrap_or(size * 0.8);
    let descent = read_f64(obj, "descent").unwrap_or(size * 0.2);
    Some(MeasuredBox {
        width,
        ascent,
        descent,
    })
}

fn read_f64(obj: &Object, key: &str) -> Option<f64> {
    let v = Reflect::get(obj, &JsValue::from_str(key)).ok()?;
    v.as_f64()
}

fn fallback_box(text: &str, size: f64) -> MeasuredBox {
    MeasuredBox {
        width: text.chars().count() as f64 * size * 0.6,
        ascent: size * 0.8,
        descent: size * 0.2,
    }
}

impl Metrics for HostCallbackMetrics {
    /// Single bridge call returns width + ascent + descent — the host's
    /// natural shape. All 6 helper methods inherit their default impls
    /// (which route back through `measure`); the bridge is the only
    /// thing the host needs to wire up. `typo_ascent` collapses to
    /// `ascent` via the default impl, which matches what every browser
    /// bridge currently exposes.
    fn measure(&self, text: &str, family: &str, size: f64, bold: bool, _italic: bool) -> Measured {
        let m = self.measure_box(text, family, size, bold);
        Measured {
            width: m.width,
            ascent: m.ascent,
            descent: m.descent,
        }
    }
}
