import React, { useContext, useEffect, useMemo, useState } from 'react';
import {
  ActivityIndicator,
  type LayoutChangeEvent,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import { SvgXmlDecoded } from './svgAst';
import type { SupramarkDiagramNode, SupramarkDiagramConfig } from '@supramark/core';
import { shouldDeferDiagramRender } from '@supramark/core';
import { type DiagramRenderResult, type DiagramRenderService } from '@supramark/engines';
import { createReactNativeDiagramEngine } from '@supramark/engines/rn';
import { normalizeSvg, normalizeSvgLight, stripRootSvgSize } from './svgUtils';
import { SourceStateContext } from './SourceStateContext';
import { getRendererCache, resolveDiagramCachePolicy, stableSerialize } from './renderCache';

export interface DiagramNodeProps {
  node: SupramarkDiagramNode;
  /** Optional engine override for renderer composition and deterministic tests. */
  diagramEngine?: DiagramRenderService;
  /**
   * Diagram subsystem config.
   *
   * - Passed in via SupramarkConfig.diagram from the host;
   * - Used to inject per-engine defaults (server / timeout / etc.);
   * - Per-node `node.meta` still overrides these defaults.
   */
  diagramConfig?: SupramarkDiagramConfig;
  /** Global cache option used only when diagram-specific policies do not override it. */
  globalCache?: boolean;
}

const defaultDiagramEngine = createReactNativeDiagramEngine();

interface CachedDiagramResult {
  svg: string;
}

// Engine identities prevent an injected renderer from reading another engine's result.
const diagramEngineIds = new WeakMap<DiagramRenderService, number>();
let nextDiagramEngineId = 1;

/** Returns a stable process-local identity for a diagram engine instance. */
function getDiagramEngineId(engine: DiagramRenderService): number {
  const existing = diagramEngineIds.get(engine);
  if (existing !== undefined) {
    return existing;
  }

  const next = nextDiagramEngineId++;
  diagramEngineIds.set(engine, next);
  return next;
}

export const DiagramNode: React.FC<DiagramNodeProps> = ({
  node,
  diagramConfig,
  globalCache,
  diagramEngine = defaultDiagramEngine,
}) => {
  const sourceState = useContext(SourceStateContext);
  // Open fences remain in a receiving state while their source can still grow.
  const deferRender = shouldDeferDiagramRender(node, sourceState);
  // Resolve the existing per-engine override on top of diagram.defaultCache.
  const normalizedEngine = String(node.engine || '').toLowerCase();
  const options = useMemo(
    () => buildRenderOptions(node.engine, node.meta, diagramConfig),
    [node.engine, node.meta, diagramConfig]
  );
  const cachePolicy = useMemo(
    () =>
      resolveDiagramCachePolicy(
        diagramConfig?.engines?.[node.engine]?.cache,
        diagramConfig?.defaultCache,
        globalCache
      ),
    [diagramConfig, globalCache, node.engine]
  );
  const diagramCache = getRendererCache<CachedDiagramResult>(
    `diagram:${normalizedEngine}`,
    cachePolicy,
    value => value.svg.length
  );
  // Include every render-affecting input so cached SVG cannot cross option variants.
  const diagramCacheKey = useMemo(
    () =>
      `${getDiagramEngineId(diagramEngine)}\u0000${normalizedEngine}\u0000${node.code}\u0000${stableSerialize(options)}`,
    [diagramEngine, node.code, normalizedEngine, options]
  );
  const [svg, setSvg] = useState<string | null>(() =>
    deferRender ? null : (diagramCache?.get(diagramCacheKey)?.svg ?? null)
  );
  const [error, setError] = useState<string | null>(null);
  // Width granted by the container, reported by the diagram box's own onLayout.
  // Placeholder states deliberately do not report: they shrink-wrap their own
  // spinner/text, so their width is not the container's and would poison the
  // first frame's box height.
  const [measuredWidth, setMeasuredWidth] = useState<number>(0);

  const handleLayout = (event: LayoutChangeEvent) => {
    const next = Math.max(0, Math.floor(event.nativeEvent.layout.width));
    setMeasuredWidth((prev) => (prev === next ? prev : next));
  };

  useEffect(() => {
    if (deferRender) {
      setError(null);
      setSvg(null);
      return;
    }

    let cancelled = false;
    setError(null);
    const cached = diagramCache?.get(diagramCacheKey);
    if (cached) {
      setSvg(cached.svg);
      return;
    }

    setSvg(null);

    // Cache the normalized SVG rather than only the engine payload so remounts
    // can restore the exact render input synchronously and skip normalization.
    const renderDiagram = async (): Promise<CachedDiagramResult> => {
      const result: DiagramRenderResult = await diagramEngine.render({
        engine: normalizedEngine,
        code: node.code,
        options,
      });

      if (!result.success) {
        const errorMsg = result.error
          ? `${result.error.message}: ${result.error.details || result.payload}`
          : result.payload || 'Unknown error';
        throw new Error(errorMsg);
      }

      let normalized: string;
      try {
        normalized = result.payload.includes('<style')
          ? normalizeSvg(result.payload)
          : normalizeSvgLight(result.payload);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`SVG normalization failed: ${message}`);
      }
      return { svg: normalized };
    };

    const renderPromise = diagramCache
      ? diagramCache.getOrCreate(diagramCacheKey, renderDiagram)
      : renderDiagram();

    renderPromise
      .then(result => {
        if (cancelled) return;
        setSvg(result.svg);
      })
      .catch(err => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : String(err));
      });

    return () => {
      cancelled = true;
    };
  }, [
    deferRender,
    node.engine,
    node.code,
    node.meta,
    diagramConfig,
    globalCache,
    normalizedEngine,
    options,
    diagramCache,
    diagramCacheKey,
    diagramEngine,
  ]);

  if (deferRender) {
    return (
      <View style={styles.placeholder} testID="supramark-diagram-receiving">
        <ActivityIndicator size="small" />
        <Text style={styles.placeholderText}>Receiving diagram ({node.engine})…</Text>
      </View>
    );
  }

  if (!svg && !error) {
    return (
      <View style={styles.placeholder} testID="supramark-diagram-rendering">
        <ActivityIndicator size="small" />
        <Text style={styles.placeholderText}>Rendering diagram ({node.engine})…</Text>
      </View>
    );
  }

  if (error) {
    return (
      <View style={styles.placeholder} testID="supramark-diagram-error">
        <Text style={styles.errorText}>Diagram render error: {error}</Text>
      </View>
    );
  }

  if (svg) {
    const viewBoxMatch = svg.match(/viewBox="([^"]+)"/);
    const widthAttrMatch = svg.match(/<svg[^>]*\bwidth="([^"]+)"/);
    const heightAttrMatch = svg.match(/<svg[^>]*\bheight="([^"]+)"/);

    let svgWidth = 0;
    let svgHeight = 0;

    if (viewBoxMatch) {
      const parts = viewBoxMatch[1].split(/[\s,]+/);
      if (parts.length === 4) {
        svgWidth = parseFloat(parts[2]);
        svgHeight = parseFloat(parts[3]);
      }
    }

    if (svgWidth <= 0 && widthAttrMatch) svgWidth = parseFloat(widthAttrMatch[1]);
    if (svgHeight <= 0 && heightAttrMatch) svgHeight = parseFloat(heightAttrMatch[1]);

    // Display size = the SVG's intrinsic width, capped by the host config and by
    // the height budget; the *container* may narrow it further through the
    // `maxWidth: '100%'` in styles.diagram.
    // Declaring the intrinsic width (rather than `width: '100%'`) keeps
    // shrink-wrapped parents — a chat bubble with only a max width — expanding,
    // while the container cap stops the diagram from ever painting outside it.
    // Window/screen dimensions are deliberately not consulted: they describe the
    // host window, not the container the diagram is rendered into.
    const hostCap =
      typeof diagramConfig?.maxWidth === 'number' &&
      Number.isFinite(diagramConfig.maxWidth) &&
      diagramConfig.maxWidth > 0
        ? diagramConfig.maxWidth
        : Number.POSITIVE_INFINITY;
    // Trim the width instead of clamping the height so the box keeps the SVG's
    // exact aspect ratio (a clamped height letterboxes the drawing).
    const heightLimitedWidth =
      isPositiveFinite(svgWidth) && isPositiveFinite(svgHeight)
        ? (MAX_DIAGRAM_HEIGHT * svgWidth) / svgHeight
        : Number.POSITIVE_INFINITY;
    // No usable intrinsic width (malformed SVG): fall back to the width measured
    // on the previous layout, else a fixed width. Never `width: '100%'` — that
    // resolves to 0 in a shrink-wrapped chain and would collapse the bubble.
    const intrinsicWidth = isPositiveFinite(svgWidth)
      ? svgWidth
      : measuredWidth > 0
        ? measuredWidth
        : FALLBACK_DIAGRAM_WIDTH;
    const declaredWidth = Math.min(intrinsicWidth, hostCap, heightLimitedWidth);
    const aspectRatio =
      isPositiveFinite(svgWidth) && isPositiveFinite(svgHeight)
        ? svgWidth / svgHeight
        : undefined;
    // Yoga derives an `aspectRatio` height from the *declared* width, not from
    // the width the container actually grants: the maxWidth clamp lands on the
    // cross axis in a column parent (the chat bubble), leaving the box taller
    // than the drawing and the surplus as blank space. onLayout reports the
    // granted width, so the height is derived from it here instead. The width
    // never depends on the height, so this settles in a single pass.
    const grantedWidth =
      measuredWidth > 0 ? Math.min(measuredWidth, declaredWidth) : declaredWidth;
    const boxHeight = aspectRatio === undefined ? FALLBACK_DIAGRAM_HEIGHT : grantedWidth / aspectRatio;

    let scalableSvg = svg;
    if (!viewBoxMatch && svgWidth > 0 && svgHeight > 0) {
      scalableSvg = scalableSvg.replace(
        /<svg([^>]*)>/,
        `<svg$1 viewBox="0 0 ${svgWidth} ${svgHeight}">`
      );
    }

    scalableSvg = stripRootSvgSize(scalableSvg);

    return (
      <View
        style={[styles.diagram, { width: declaredWidth, height: boxHeight }]}
        onLayout={handleLayout}
        testID="supramark-diagram-svg"
      >
        <SvgXmlDecoded
          xml={scalableSvg}
          width="100%"
          height="100%"
          preserveAspectRatio="xMidYMid meet"
        />
      </View>
    );
  }

};

/**
 * Compose render options from per-engine global defaults +
 * node-specific meta overrides.
 *
 * Resolution order:
 * - diagramConfig.engines[engine] supplies engine-level defaults
 *   (server / timeout / etc.);
 * - fields on `node.meta` override those defaults;
 * - returns `undefined` when neither carries any options.
 */
function buildRenderOptions(
  engine: string,
  meta: SupramarkDiagramNode['meta'],
  diagramConfig?: SupramarkDiagramConfig
): Record<string, unknown> | undefined {
  const base: Record<string, unknown> = {};

  const engineConfig = diagramConfig?.engines?.[engine];
  if (engineConfig) {
    if (typeof engineConfig.server === 'string') {
      base.server = engineConfig.server;
      base.plantumlServer = engineConfig.server;
    }
    if (typeof engineConfig.timeoutMs === 'number') {
      base.timeout = engineConfig.timeoutMs;
    }
    if (engineConfig.cache) {
      base.cache = engineConfig.cache;
    }

    for (const [key, value] of Object.entries(engineConfig as Record<string, unknown>)) {
      if (value === undefined) continue;
      if (key === 'enabled' || key === 'timeoutMs' || key === 'server' || key === 'cache') continue;
      base[key] = value;
    }
  }

  if (base.timeout === undefined) {
    const fallback = diagramConfig?.defaultTimeoutMs;
    if (typeof fallback === 'number' && fallback > 0 && Number.isFinite(fallback)) {
      base.timeout = fallback;
    }
  }

  if (!meta) {
    return Object.keys(base).length > 0 ? base : undefined;
  }

  return { ...base, ...meta };
}

// Engines lay tall diagrams out at their intrinsic width; without a budget a
// vertical flowchart would occupy an unbounded amount of the host's scroll
// view. The width is trimmed to honour this, so the aspect ratio stays exact.
const MAX_DIAGRAM_HEIGHT = 500;

// Size used when the SVG exposes no usable intrinsic width/height.
const FALLBACK_DIAGRAM_WIDTH = 300;
const FALLBACK_DIAGRAM_HEIGHT = 300;

/** True for a finite, positive number — the only values usable as a size. */
function isPositiveFinite(value: number): boolean {
  return Number.isFinite(value) && value > 0;
}

const styles = StyleSheet.create({
  diagram: {
    // The container owns the final width: a diagram narrower than the host
    // window (a chat bubble, a card, a side panel) must never paint outside it.
    maxWidth: '100%',
    // Deliberately not clipped. A viewport clip only matches a browser when the
    // engine's viewBox actually covers the drawing; where it does not (engines
    // have gaps here — see the flowchart cluster bounds fix in
    // crates/mermaid-little), clipping would silently truncate the diagram
    // instead of showing it whole. Hosts render the full drawing.
    // The engine's own bounds are covered by regression tests there
    // (flowchart_*_stay_inside_the_viewbox).
  },
  placeholder: {
    width: '100%',
    padding: 8,
    borderRadius: 4,
    borderWidth: 1,
    borderColor: '#ccc',
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
  },
  placeholderText: {
    fontSize: 12,
    color: '#666',
    marginLeft: 6,
  },
  errorText: {
    fontSize: 12,
    color: '#d4380d',
  },
});
