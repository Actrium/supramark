/**
 * React Native native-engine adapter registry.
 *
 * Diagram engines that ship a Rust → C ABI native FFI wrapper
 * (`crates/<engine>-little/packages/native/`) become available on
 * RN through a thin TurboModule / NativeModule binding. The actual
 * binding code lives in consumer-side npm packages (e.g.
 * `@actrium/supramark-d2-native-rn`), one per engine, because the
 * native module shape is platform / linker specific.
 *
 * This file is the **routing layer**: consumers register an adapter
 * per engine name at startup, and `@supramark/engines/rn` discovers
 * them at render time.
 *
 * Companion to:
 *   - `host-bridge.ts` (web, host canvas measureText)
 *   - `rn-bridge.ts` (RN, measureFn registry)
 *
 * The metrics-callback installation (Rust side calls back into JS
 * for text width) is each native module's responsibility — they
 * each get a `supramark_install_metrics_callback` C symbol from the
 * statically linked `font-metrics::ffi_callback` and must call it
 * at module init with the host's measureText. See
 * `crates/font-metrics/src/ffi_callback.rs` and `rn-bridge.ts`.
 */

/**
 * What a native-engine adapter does for one call.
 *
 * @param code   The diagram source (mermaid mmd / d2 d2 / plantuml puml / etc.)
 * @param options Engine-specific options (theme / sketch / etc.).
 *                Adapter is free to ignore unknown keys.
 * @returns       SVG markup as UTF-8 string. Throws on parse / render error.
 */
export type NativeRenderFn = (code: string, options?: Record<string, unknown>) => Promise<string>;

export interface NativeEngineAdapter {
  /** Engine identifier matching the supramark diagram node `engine` field. */
  engine: string;
  /** Render entry. */
  render: NativeRenderFn;
  /**
   * Optional one-time setup hook. The native module calls
   * `supramark_install_metrics_callback` on the Rust side here so
   * font measurement flows through the host's measureText.
   *
   * The bridge invokes this on first use; calling it more than once
   * MUST be idempotent.
   */
  installMetricsCallback?: () => void;
}

const registry = new Map<string, NativeEngineAdapter>();
const installed = new Set<string>();

/**
 * Register a native engine adapter. Last write wins (replacing an
 * existing adapter for the same engine is allowed — useful for
 * tests or hot reload).
 */
export function registerNativeEngineAdapter(adapter: NativeEngineAdapter): void {
  registry.set(adapter.engine, adapter);
  // Re-arm: if the new adapter has its own metrics installer, it
  // hasn't been called yet for this adapter instance.
  installed.delete(adapter.engine);
}

/** Retrieve a previously-registered adapter, or `undefined`. */
export function getNativeEngineAdapter(engine: string): NativeEngineAdapter | undefined {
  return registry.get(engine);
}

/** List engine names that currently have a native adapter. */
export function listNativeEngines(): string[] {
  return Array.from(registry.keys());
}

/**
 * Run a native render through the registered adapter. Idempotently
 * triggers the adapter's metrics-callback installer on first use.
 *
   * Returns `null` if no adapter is registered for `engine`, letting
   * callers fall back to an error / unsupported message.
 */
export async function renderViaNative(
  engine: string,
  code: string,
  options?: Record<string, unknown>
): Promise<string | null> {
  const adapter = registry.get(engine);
  if (!adapter) return null;

  if (adapter.installMetricsCallback && !installed.has(engine)) {
    try {
      adapter.installMetricsCallback();
      installed.add(engine);
    } catch (err) {
      // Don't block render: the Rust side falls back to the
      // size * 0.6 heuristic when no callback is installed.
      // Surface the failure so it shows up in dev logs.
      console.warn(
        `[supramark] installMetricsCallback for engine "${engine}" failed:`,
        err
      );
    }
  }

  return adapter.render(code, options);
}

/** Test helper — wipe the registry. Not exported from the package barrel. */
export function __resetNativeEngineRegistryForTests(): void {
  registry.clear();
  installed.clear();
}
