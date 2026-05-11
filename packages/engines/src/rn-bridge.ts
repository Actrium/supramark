/**
 * RN-side measurement bridge for the native FFI metrics path.
 *
 * Companion to `host-bridge.ts` (web). On RN, font measurement is
 * served by a native module that calls into iOS UIFont / Android
 * Paint / react-native-skia and returns a `MeasureResult` per call.
 * The Rust side (`font-metrics::ffi_callback::FfiCallbackMetrics`)
 * reads back via an `extern "C"` callback installed at startup.
 *
 * Each engine's RN native module is expected to expose a thin
 * `installSupramarkMetricsCallback(measureFn)` shim that takes the
 * JS-side measureFn and registers a C ABI function pointer with the
 * Rust global. This file is the JS-side handle: it stores the host
 * application's measureFn so the engine native modules can pick it
 * up — without forcing every engine to re-implement the storage.
 *
 * The library does **not** ship a default RN measurer because
 * different RN projects use different stacks (react-native-skia,
 * react-native-text-size, a custom Yoga + TurboModule, …); pick
 * whichever you already use and pass its `measureText` here.
 *
 * Example with react-native-skia:
 *
 * ```ts
 * import { Skia } from '@shopify/react-native-skia';
 * import { installRnMetricsBridge } from '@supramark/engines/rn-bridge';
 *
 * installRnMetricsBridge((family, text, size, bold, italic) => {
 *   const font = Skia.Font(
 *     Skia.FontMgr.System().matchFamilyStyle(family, {
 *       weight: bold ? 700 : 400,
 *       slant: italic ? 1 : 0,
 *       width: 5,
 *     }),
 *     size,
 *   );
 *   const w = font.measureText(text).width;
 *   const m = font.getMetrics();
 *   return { width: w, ascent: -m.ascent, descent: m.descent };
 * });
 * ```
 */

export interface MeasureResult {
  width: number;
  ascent?: number;
  descent?: number;
}

export type MeasureFn = (
  family: string,
  text: string,
  size: number,
  bold: boolean,
  italic: boolean,
) => MeasureResult;

let pending: MeasureFn | null = null;
let installed = false;

/**
 * Register the host-side `measureText` impl. The actual wiring
 * through to Rust is driven by each engine's native module (which
 * knows the engine-specific TurboModule name); this file just holds
 * the callback so engines can fetch it on init.
 *
 * Idempotent — calling multiple times replaces the previously
 * registered callback (last-write-wins). Engines that already
 * fetched and forwarded the previous callback to Rust should
 * re-fetch and re-install after every change.
 */
export function installRnMetricsBridge(measureFn: MeasureFn): void {
  pending = measureFn;
  installed = true;
}

/**
 * Read back the registered host measurer. Engines call this from
 * their native-module init path to forward the JS callback into
 * Rust via the engine's `installSupramarkMetricsCallback` shim.
 * Returns `null` if no host measurer has been registered yet, in
 * which case the Rust side stays on its `size * 0.6`-per-char
 * fallback.
 */
export function getRnMeasureFn(): MeasureFn | null {
  return pending;
}

/**
 * Whether `installRnMetricsBridge` has been called at least once
 * this process. Engines can use this to log a one-time warning when
 * a host forgets to wire up the bridge.
 */
export function isRnMetricsBridgeInstalled(): boolean {
  return installed;
}
