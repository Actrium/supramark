import React, { useContext, useEffect, useMemo, useState } from 'react';
import {
  ActivityIndicator,
  Dimensions,
  type LayoutChangeEvent,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import { SvgXml } from 'react-native-svg';
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
  const diagramCache = getRendererCache<CachedDiagramResult>('diagram', cachePolicy);
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
  // Actual container width: the diagram should follow its parent container
  // (e.g. a narrow container like a chat bubble) rather than rendering
  // straight to the screen width, otherwise it would drift right or
  // overflow. 0 means not yet measured; rendering falls back to the screen width.
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
      <View style={styles.placeholder} onLayout={handleLayout} testID="supramark-diagram-receiving">
        <ActivityIndicator size="small" />
        <Text style={styles.placeholderText}>Receiving diagram ({node.engine})…</Text>
      </View>
    );
  }

  if (!svg && !error) {
    return (
      <View style={styles.placeholder} onLayout={handleLayout} testID="supramark-diagram-rendering">
        <ActivityIndicator size="small" />
        <Text style={styles.placeholderText}>Rendering diagram ({node.engine})…</Text>
      </View>
    );
  }

  if (error) {
    return (
      <View style={styles.placeholder} onLayout={handleLayout} testID="supramark-diagram-error">
        <Text style={styles.errorText}>Diagram render error: {error}</Text>
      </View>
    );
  }

  if (svg) {
    const viewBoxMatch = svg.match(/viewBox="([^"]+)"/);
    const widthAttrMatch = svg.match(/<svg[^>]*\bwidth="([^"]+)"/);
    const heightAttrMatch = svg.match(/<svg[^>]*\bheight="([^"]+)"/);

    const { width: screenWidth } = Dimensions.get('window');
    // Diagram width cap: 90% of the screen width, leaving room for the
    // bubble's padding so the diagram doesn't touch the edge.
    // Uses a percentage rather than a fixed pixel value to adapt to different hosts' padding.
    const maxChartWidth = screenWidth * 0.9;
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

    // Diagram display width: take the SVG's intrinsic width, clamped to
    // [maxChartWidth×0.6, maxChartWidth].
    // The diagram proactively declares a width to expand the bubble (solving
    // the collapse that happens because width:100% has nothing to expand
    // against in a diagram-only message bubble); the value is derived from
    // the SVG's intrinsic attributes, doesn't depend on onLayout, and forms
    // no feedback loop with the parent container (unlike the old code, which
    // used width:containerWidth measured back from the parent via onLayout).
    const minChartWidth = maxChartWidth * 0.6;
    const intrinsicWidth = svgWidth > 0
      ? svgWidth
      : (measuredWidth > 0 ? measuredWidth : maxChartWidth);
    const chartWidth = Math.max(minChartWidth, Math.min(maxChartWidth, intrinsicWidth));

    let height = 300;
    if (svgWidth > 0 && svgHeight > 0) {
      height = (svgHeight / svgWidth) * chartWidth;
      height = Math.min(height, 500);
    }

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
        style={[styles.diagram, { width: chartWidth, height }]}
        onLayout={handleLayout}
        testID="supramark-diagram-svg"
      >
        <SvgXml
          xml={scalableSvg}
          width={chartWidth}
          height={height}
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

const styles = StyleSheet.create({
  diagram: {},
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
