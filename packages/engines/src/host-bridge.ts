/**
 * Host-side text-measurement bridge for the wasm rendering pipeline.
 *
 * The Rust crate `font-metrics` ships a wasm-only `Metrics` impl
 * (`crates/font-metrics/src/host_callback.rs`) that defers every
 * measurement to a JS-installed callback at
 * `globalThis.supramark.measureText: (family, text, size, bold) =>
 * { width, ascent?, descent? }`. Layer-1 layout inside the wasm
 * (e.g. mermaid-little, plantuml-little) then matches whatever fonts
 * the host browser actually paints with at Layer 3.
 *
 * `installHostMetricsBridge` provides a default, idempotent installer
 * that wires this contract to the standard browser canvas API.
 * Hosts that already supply their own (richer) measurer should
 * install it first; this function detects an existing bridge and
 * leaves it alone. In SSR / non-DOM environments where neither
 * `OffscreenCanvas` nor `document` is available, the installer is a
 * no-op and the wasm side falls back to its `size * 0.6`-per-char
 * heuristic.
 */

interface MeasureResult {
  width: number;
  ascent?: number;
  descent?: number;
}

type MeasureFn = (family: string, text: string, size: number, bold: boolean) => MeasureResult;

let installed = false;

function pickContext(): CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null {
  try {
    const g = globalThis as unknown as { OffscreenCanvas?: typeof OffscreenCanvas };
    if (typeof g.OffscreenCanvas === 'function') {
      const canvas = new g.OffscreenCanvas(8, 8);
      const ctx = canvas.getContext('2d');
      if (ctx) return ctx;
    }
  } catch {
    // OffscreenCanvas exists but ctor / getContext threw — fall through.
  }

  try {
    if (typeof document !== 'undefined' && typeof document.createElement === 'function') {
      const canvas = document.createElement('canvas');
      const ctx = canvas.getContext('2d');
      if (ctx) return ctx;
    }
  } catch {
    // document exists but createElement / getContext threw — fall through.
  }

  return null;
}

function fallbackBox(text: string, size: number): MeasureResult {
  return { width: text.length * size * 0.6 };
}

export function installHostMetricsBridge(): void {
  if (installed) return;

  const g = globalThis as unknown as {
    supramark?: { measureText?: MeasureFn } & Record<string, unknown>;
  };
  const existing = g.supramark;
  if (existing && typeof existing.measureText === 'function') {
    installed = true;
    return;
  }

  const ctx = pickContext();
  if (!ctx) {
    installed = true;
    return;
  }

  const measureText: MeasureFn = (family, text, size, bold) => {
    try {
      ctx.font = `${bold ? 'bold ' : ''}${size}px ${family}`;
      const m = ctx.measureText(text);
      const ascent =
        typeof m.actualBoundingBoxAscent === 'number' ? m.actualBoundingBoxAscent : size * 0.8;
      const descent =
        typeof m.actualBoundingBoxDescent === 'number' ? m.actualBoundingBoxDescent : size * 0.2;
      return { width: m.width, ascent, descent };
    } catch {
      return fallbackBox(text, size);
    }
  };

  g.supramark = { ...(existing ?? {}), measureText };
  installed = true;
}
