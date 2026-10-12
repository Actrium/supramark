import React, { createContext, useContext, useEffect, useMemo, useState } from 'react';
import { Text, View, Image, Linking, ScrollView, TouchableOpacity, Dimensions } from 'react-native';
import type {
  SupramarkRootNode,
  SupramarkNode,
  SupramarkImageNode,
  SupramarkHeadingNode,
  SupramarkCodeNode,
  SupramarkMathBlockNode,
  SupramarkDiagramNode,
  SupramarkContainerNode,
  SupramarkDefinitionItemNode,
  SupramarkDefinitionTermNode,
  SupramarkDefinitionDescriptionNode,
  SupramarkDiagramConfig,
  SupramarkConfig,
  SupramarkCodeHighlightResult,
  SupramarkCodeHighlighter,
  SupramarkSourceState,
} from '@supramark/core';
import {
  parse,
  expandOpaqueContainers,
  isFeatureEnabled,
  isDiagramFeatureEnabled,
  getFeatureOptionsAs,
  SUPRAMARK_ADMONITION_KINDS,
} from '@supramark/core';
import { DiagramNode } from './DiagramNode';
import { MathBlock } from './MathBlock';
import { MathInline } from './MathInline';
import { type SupramarkStyles, mergeStyles, darkThemeStyles } from './styles';
import { ErrorBoundary, type ErrorInfo, ErrorDisplay } from './ErrorBoundary';
import { SourceStateContext } from './SourceStateContext';
import { resolveDevelopmentMode } from './devMode';
import {
  getRendererCache,
  resolveDiagramCachePolicy,
  resolveRendererCachePolicy,
  type RendererCachePolicy,
} from './renderCache';

type RenderedNode = React.ComponentProps<typeof Text>['children'];

// Dev mode: __DEV__ in React Native bundles, NODE_ENV elsewhere (web/tests).
// Deep-freeze of the shared cached AST only runs in dev so production keeps
// the freeze cost off the render path while the read-only contract holds.
const isDevMode = resolveDevelopmentMode();

interface ParsedDocument {
  /** Immutable after expansion; cached snapshots may be shared by multiple renderer instances. */
  readonly root: SupramarkRootNode;
  readonly highlighted: ReadonlyMap<string, SupramarkCodeHighlightResult>;
  readonly sourceState: SupramarkSourceState;
}

/**
 * Estimates the byte footprint of a cached parsed document for the
 * byte-aware cache cap. Walks the AST summing string `value`/`code`/`text`
 * lengths plus a small fixed overhead per node; ignores the highlighted Map
 * (its entries are keyed by source slice and bounded by the AST size).
 * Runs only on a cache miss (one `set()`), so an O(nodes) walk is acceptable.
 */
function estimateParsedDocumentBytes(document: ParsedDocument): number {
  let bytes = 0;
  const stack: SupramarkNode[] = [...document.root.children];
  while (stack.length > 0) {
    const node = stack.pop() as SupramarkNode & {
      value?: unknown;
      code?: unknown;
      text?: unknown;
      children?: SupramarkNode[];
    };
    bytes += 64; // node object overhead (fields, prototype, refs).
    const text =
      typeof node.value === 'string'
        ? node.value
        : typeof node.code === 'string'
          ? node.code
          : typeof node.text === 'string'
            ? node.text
            : undefined;
    if (text !== undefined) {
      bytes += text.length;
    }
    if (node.children) {
      for (const child of node.children) {
        stack.push(child);
      }
    }
  }
  return Math.max(bytes, 1);
}

/**
 * Recursively freezes plain objects/arrays reachable from a cached AST in dev
 * mode so a host `containerRenderers` annotating AST nodes in place cannot
 * silently cross-contaminate other rows sharing the cached snapshot. Production
 * keeps the freeze off the render path; the read-only contract still applies by
 * convention. Class instances, Maps, and other non-plain values are skipped
 * (freezing them is unsafe or a no-op on their entries). Assumes AST nodes are
 * plain object literals (Rust canonical parser output) — class-wrapped nodes
 * would bypass the freeze silently.
 */
function deepFreezeAst(value: unknown): void {
  if (value === null || typeof value !== 'object') {
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) {
      deepFreezeAst(item);
    }
  } else {
    // Object.getPrototypeOf is typed `any` in the ES5 lib, so assert the return
    // to keep the type-aware lint (no-unsafe-assignment) happy. Only recurse
    // into plain objects; class instances, Maps, etc. are left untouched.
    const proto = Object.getPrototypeOf(value) as object | null;
    if (proto !== null && proto !== Object.prototype) {
      return;
    }
    const record = value as Record<PropertyKey, unknown>;
    for (const key of Object.keys(record)) {
      deepFreezeAst(record[key]);
    }
    for (const sym of Object.getOwnPropertySymbols(record)) {
      deepFreezeAst(record[sym]);
    }
  }
  // Object.freeze is idempotent and a no-op on non-objects; safe to call.
  try {
    Object.freeze(value);
  } catch {
    // Some environments throw on exotic objects; the freeze is best-effort.
  }
}

// Highlighter identities keep parsed-document cache entries separated when a host swaps services.
const codeHighlighterIds = new WeakMap<SupramarkCodeHighlighter, number>();
let nextCodeHighlighterId = 1;

/** Returns a stable process-local identity for one optional code highlighter. */
function getCodeHighlighterId(highlighter?: SupramarkCodeHighlighter): number {
  if (!highlighter) {
    return 0;
  }

  const existing = codeHighlighterIds.get(highlighter);
  if (existing !== undefined) {
    return existing;
  }

  const next = nextCodeHighlighterId++;
  codeHighlighterIds.set(highlighter, next);
  return next;
}

/** Resolves document-cache bounds from global, default-diagram, or engine policies. */
function resolveDocumentCachePolicy(config?: SupramarkConfig): RendererCachePolicy {
  if (config?.options?.cache === true) {
    return resolveRendererCachePolicy({ enabled: true }, config.diagram?.defaultCache);
  }

  if (config?.diagram?.defaultCache?.enabled === true) {
    return resolveRendererCachePolicy(undefined, config.diagram.defaultCache);
  }

  const enabledEnginePolicies = Object.values(config?.diagram?.engines ?? {})
    .map(engineConfig => engineConfig?.cache)
    .filter(cache => cache?.enabled === true)
    .map(cache => resolveRendererCachePolicy(cache, undefined));
  if (enabledEnginePolicies.length === 0) {
    return resolveRendererCachePolicy(undefined, undefined);
  }

  // Use the strictest enabled engine bounds because one document may contain
  // several diagram engines backed by the same parsed-document cache.
  const finiteTtls = enabledEnginePolicies
    .map(policy => policy.ttl)
    .filter((ttl): ttl is number => ttl !== undefined);
  const finiteMaxBytes = enabledEnginePolicies
    .map(policy => policy.maxBytes)
    .filter((maxBytes): maxBytes is number => maxBytes !== undefined);
  return {
    enabled: true,
    maxSize: Math.min(...enabledEnginePolicies.map(policy => policy.maxSize)),
    ttl: finiteTtls.length > 0 ? Math.min(...finiteTtls) : undefined,
    maxBytes: finiteMaxBytes.length > 0 ? Math.min(...finiteMaxBytes) : undefined,
  };
}

// Minimal shape of the optional `react-native-maps` module. Only the members
// used here are declared; the package itself is an optional peer dependency.
interface ReactNativeMapsModule {
  default: React.ComponentType<Record<string, unknown>>;
  Marker: React.ComponentType<Record<string, unknown>>;
}

function getDefinitionTerms(item: SupramarkDefinitionItemNode): SupramarkDefinitionTermNode[] {
  return item.children.filter(
    (child): child is SupramarkDefinitionTermNode => child.type === 'definition_term'
  );
}

function getDefinitionDescriptions(
  item: SupramarkDefinitionItemNode
): SupramarkDefinitionDescriptionNode[] {
  return item.children.filter(
    (child): child is SupramarkDefinitionDescriptionNode => child.type === 'definition_description'
  );
}

export interface ContainerRendererRN {
  (args: {
    node: SupramarkContainerNode;
    key: number;
    styles: ReturnType<typeof mergeStyles>;
    config?: SupramarkConfig;
    onOpenHtmlPage?: (node: SupramarkContainerNode) => void;
    renderNode: (node: SupramarkNode, key: number) => RenderedNode;
    renderChildren: (children: SupramarkNode[]) => RenderedNode;
  }): RenderedNode;
}

/** A reference to an image within a gallery group. */
export interface SupramarkImageRef {
  url: string;
  alt?: string;
  title?: string;
}

/**
 * Image tap event delivered to a host-supplied {@link SupramarkImagePressHandler}.
 *
 * `galleryImages`/`galleryIndex` carry the adjacent images merged into the same
 * gallery as the tapped image, so the host can open a swipeable preview covering
 * the whole group. A single image still receives a one-element array.
 */
export interface SupramarkImagePressEvent {
  /** The image's own URL. */
  url: string;
  /** Alt text, if present. */
  alt?: string;
  /** Title, if present. */
  title?: string;
  /** Outer link URL when the image is wrapped in a link. */
  linkUrl?: string;
  /** All images in the tapped image's gallery group (adjacent merged images). */
  galleryImages: SupramarkImageRef[];
  /** Index of the tapped image within {@link galleryImages}. */
  galleryIndex: number;
}

/** Host handler invoked when the user taps a block image. */
export type SupramarkImagePressHandler = (event: SupramarkImagePressEvent) => void;

/**
 * Context pipe for the image-press handler. Carries the host callback from the
 * root <Supramark> down to <MarkdownImage> without threading it through every
 * renderNode/renderRootNodes signature (image taps are an internal concern;
 * unlike onOpenHtmlPage they never reach a custom container renderer).
 */
const ImagePressContext = createContext<SupramarkImagePressHandler | undefined>(undefined);

export interface SupramarkProps {
  /** Markdown source text */
  markdown: string;
  /** Pre-parsed AST (takes precedence over markdown) */
  ast?: SupramarkRootNode;
  /**
   * Custom styles (override the default styles).
   *
   * Spacing model: inter-block spacing is managed uniformly by root.gap
   * (default 8) instead of each block's marginBottom. Customizing a block's
   * marginBottom (e.g. paragraph:12) stacks with root.gap → an effective
   * spacing of 20. To fully customize spacing, also set root: { gap: 0 }.
   */
  styles?: SupramarkStyles;
  /**
   * Theme: adjusts the foreground color and decoration colors of content
   * elements (text, code block background, borders, etc.) so they stay
   * readable on a canvas of the corresponding brightness.
   *
   * - 'dark': applies darkThemeStyles (dark-friendly foreground/decoration colors).
   * - 'light': uses the default (light) foreground, equivalent to not passing theme.
   * - You may also pass a custom SupramarkStyles object directly as the theme.
   *
   * Important: the component does not paint a canvas background on root. The
   * host must provide a canvas color matching the theme's brightness for the
   * rendering container (the exported {@link themeBackground} is a recommended
   * value) — otherwise foreground text may become unreadable, e.g. when
   * theme="dark" the host container should use a dark background.
   */
  theme?: 'light' | 'dark' | SupramarkStyles;
  /**
   * Feature configuration (used to enable/disable diagrams and other
   * extension capabilities as needed).
   * `options.cache` is the global default for the document and diagram
   * caches; a more specific diagram policy can override it. The cache is
   * shared by equivalent inputs and does not require the config object
   * reference to stay stable across remounts.
   */
  config?: SupramarkConfig;
  /** Whether the Markdown source may still receive appended streaming content. */
  sourceState?: SupramarkSourceState;
  /** Error callback (optional) */
  onError?: (error: Error, errorInfo?: React.ErrorInfo) => void;
  /** Custom error display component (optional) */
  errorFallback?: (error: ErrorInfo) => RenderedNode;

  /**
   * Container extension renderer registry: dispatched by node.name when
   * node.type === 'container'.
   */
  containerRenderers?: Record<string, ContainerRendererRN>;
  codeHighlighter?: SupramarkCodeHighlighter;
  codeHighlightTheme?: string;

  /**
   * Callback invoked when the user taps an HTML Page card.
   *
   * - node.data.html holds the full HTML content;
   * - the host may open a new page / modal / external browser from the callback.
   */
  onOpenHtmlPage?: (node: SupramarkContainerNode) => void;

  /**
   * Callback invoked when the user taps a block image.
   *
   * - When supplied, image taps are delegated to the host (e.g. to open a
   *   full-screen gallery); the host fully owns the tap behavior.
   * - When omitted: a standalone image is not tappable; an image link still
   *   opens its URL via Linking.
   * - `linkUrl` is set when the image is wrapped in a link.
   */
  onImagePress?: SupramarkImagePressHandler;
}

export const Supramark: React.FC<SupramarkProps> = ({
  markdown,
  ast,
  styles: customStyles,
  theme,
  config,
  sourceState = 'complete',
  onError,
  errorFallback,
  onOpenHtmlPage,
  onImagePress,
  containerRenderers,
  codeHighlighter,
  codeHighlightTheme,
}) => {
  // Global options.cache provides the least-specific cache default.
  const documentCachePolicy = useMemo(() => resolveDocumentCachePolicy(config), [config]);
  const documentCache = getRendererCache<ParsedDocument>(
    'parsed-document',
    documentCachePolicy,
    estimateParsedDocumentBytes
  );
  const codeHighlightEnabled = isFeatureGroupEnabled(config, ['@supramark/feature-code-highlight']);
  // Completed documents can share parsing/highlighting only when every input is equivalent.
  // Config is intentionally omitted because parse currently derives no plugins or AST transforms
  // from it; add the relevant config fingerprint here if that parse contract ever changes.
  const documentCacheKey = useMemo(
    () =>
      `${markdown}\u0000${codeHighlightEnabled ? 'highlight' : 'plain'}\u0000${codeHighlightTheme ?? ''}\u0000${getCodeHighlighterId(codeHighlighter)}`,
    [markdown, codeHighlightEnabled, codeHighlighter, codeHighlightTheme]
  );
  // The AST and its source state must advance together so a stale open fence
  // is never marked complete.
  const [parsedDocument, setParsedDocument] = useState<ParsedDocument | null>(() =>
    ast
      ? {
          root: ast,
          highlighted: new Map(),
          sourceState,
        }
      : sourceState === 'complete'
        ? (documentCache?.get(documentCacheKey) ?? null)
        : null
  );
  const [parseError, setParseError] = useState<ErrorInfo | null>(null);

  // Merge styles: theme -> customStyles -> defaultStyles
  const mergedStyles = useMemo(() => {
    let themeStyles: SupramarkStyles | undefined;

    if (typeof theme === 'string') {
      themeStyles = theme === 'dark' ? darkThemeStyles : undefined;
    } else if (theme) {
      themeStyles = theme;
    }

    // When both theme and customStyles are provided, customStyles takes precedence
    const finalCustomStyles = {
      ...themeStyles,
      ...customStyles,
    };

    return mergeStyles(finalCustomStyles);
  }, [customStyles, theme]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const cached =
          !ast && sourceState === 'complete' ? documentCache?.get(documentCacheKey) : undefined;
        if (cached) {
          if (!cancelled) {
            setParsedDocument(cached);
            setParseError(null);
          }
          return;
        }

        // Build one immutable render snapshot; after opaque containers are expanded,
        // renderers must treat root and descendants as read-only because completed
        // snapshots may be shared across instances and virtual-list remounts.
        const buildParsedDocument = async (): Promise<ParsedDocument> => {
          const parsed = ast ?? (await parse(markdown, { config }));
          // Post-process: recursively parse opaque containers' value.
          // In AST v2, opaque container children are empty; the body lives in
          // value (the raw markdown). The Rust parser doesn't know about
          // registerContainerHook registered on the JS-side feature plugins,
          // so it treats every :::xxx as opaque. Here, in the main component's
          // async context, we parse value into an AST subtree and fill it back
          // into children so renderNode can render it normally.
          await expandOpaqueContainers(parsed);
          const highlightedMap = await preHighlightAll(
            collectCodeHighlightTasks(parsed.children, config, codeHighlightTheme),
            codeHighlighter
          );
          // Dev-only deep freeze BEFORE the snapshot can be shared. The factory
          // result is stored into the document cache by getOrCreate, so freezing
          // here (rather than after the await) closes the microtask window in
          // which a concurrent cache hit could observe an unfrozen shared AST
          // and let a containerRenderer mutate it in place. Production skips it;
          // the read-only contract still holds by convention. See deepFreezeAst.
          if (isDevMode) {
            deepFreezeAst(parsed);
          }
          return {
            root: parsed,
            highlighted: highlightedMap,
            sourceState,
          };
        };

        const nextDocument =
          !ast && sourceState === 'complete' && documentCache
            ? await documentCache.getOrCreate(
                documentCacheKey,
                buildParsedDocument,
                // diagram.defaultCache alone retains only diagram-bearing documents;
                // options.cache=true explicitly opts the host into caching all documents.
                document =>
                  config?.options?.cache === true ||
                  containsCacheableDiagramNode(
                    document.root.children,
                    config?.diagram,
                    config?.options?.cache
                  )
              )
            : await buildParsedDocument();
        if (!cancelled) {
          // The shared snapshot was frozen inside buildParsedDocument (dev) so
          // every consumer — first mount, virtual-list remount, or a concurrent
          // cache hit — sees the same read-only AST from the start.
          setParsedDocument(nextDocument);
          setParseError(null);
        }
      } catch (error) {
        if (!cancelled) {
          const err = error as Error;
          const errorInfo: ErrorInfo = {
            type: 'parse',
            message: err.message || 'Failed to parse Markdown',
            details: err.toString(),
            stack: err.stack,
          };
          setParseError(errorInfo);
          setParsedDocument(null);

          // Invoke the error callback
          if (onError) {
            onError(err);
          }
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [
    markdown,
    ast,
    config,
    onError,
    codeHighlighter,
    codeHighlightTheme,
    sourceState,
    documentCache,
    documentCacheKey,
  ]);

  const mergedContainerRenderers = useMemo(() => {
    // FeatureConfig only describes enabled state and options; it no longer
    // carries a renderer definition. Container renderers must be injected
    // explicitly by the host to avoid implicit runtime coupling to a
    // feature package's implementation.
    return containerRenderers ?? {};
  }, [containerRenderers]);

  // Parse-error fallback: show the error info or the raw markdown
  if (parseError) {
    if (errorFallback) {
      return <>{errorFallback(parseError)}</>;
    }
    return (
      <View>
        <ErrorDisplay error={parseError} />
        <View style={mergedStyles.codeBlock}>
          <Text style={mergedStyles.code}>{markdown}</Text>
        </View>
      </View>
    );
  }

  if (!parsedDocument) {
    // Simple fallback while parsing: show the raw markdown text directly.
    return <Text>{markdown}</Text>;
  }

  return (
    <ErrorBoundary onError={onError} fallback={errorFallback}>
      <SourceStateContext.Provider value={parsedDocument.sourceState}>
        <ImagePressContext.Provider value={onImagePress}>
          <View style={mergedStyles.root}>
            {renderRootNodes(
              parsedDocument.root.children,
              mergedStyles,
              parsedDocument.highlighted,
              config,
              onOpenHtmlPage,
              mergedContainerRenderers
            )}
          </View>
        </ImagePressContext.Provider>
      </SourceStateContext.Provider>
    </ErrorBoundary>
  );
};

/** Returns whether a parsed subtree contains a diagram whose resolved cache is enabled. */
function containsCacheableDiagramNode(
  nodes: SupramarkNode[],
  diagramConfig?: SupramarkDiagramConfig,
  globalCache?: boolean
): boolean {
  for (const node of nodes) {
    if (node.type === 'diagram') {
      const policy = resolveDiagramCachePolicy(
        diagramConfig?.engines?.[node.engine]?.cache,
        diagramConfig?.defaultCache,
        globalCache
      );
      if (policy.enabled) {
        return true;
      }
    }
    if (
      'children' in node &&
      Array.isArray((node as { children?: SupramarkNode[] }).children) &&
      containsCacheableDiagramNode(
        (node as { children: SupramarkNode[] }).children,
        diagramConfig,
        globalCache
      )
    ) {
      return true;
    }
  }
  return false;
}

interface BlockImageItem {
  image: SupramarkImageNode;
  linkUrl?: string;
}

/** Whether an image URL has a scheme RN can load (http/data/file/blob). Relative paths and empty strings are not loadable as remote sources. */
function hasLoadableImageUrl(url: string): boolean {
  return /^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(url);
}

/**
 * Placeholder label for an image whose bitmap cannot be shown: alt, then
 * title, then a generic marker. Shared by the block placeholder and the
 * inline-image fallback so the label rule stays single-sourced.
 */
function imageFallbackLabel(image: { alt?: string; title?: string }): string {
  return image.alt || image.title || '[image]';
}

/**
 * Collects images from a sequence containing only images, image links, and
 * layout separators (whitespace text, hard breaks). Returns null as soon as
 * any non-image inline content appears, so mixed content keeps its inline
 * layout. Shared by paragraph rendering and tight/loose list items so a
 * standalone image in a list is not squeezed to 20×20.
 */
function collectImageItems(children: SupramarkNode[]): BlockImageItem[] | null {
  const items: BlockImageItem[] = [];
  for (const child of children) {
    // Parser-produced whitespace between images is a layout separator, not mixed content.
    if (child.type === 'text' && child.value.trim().length === 0) {
      continue;
    }
    // Hard breaks between images are layout separators too, so `![a](u)\n![b](u)`
    // (image + break + image) lands in the same gallery as the soft-newline form
    // instead of degrading to 20×20 inlines.
    if (child.type === 'break') {
      continue;
    }
    // A direct image participates in the wrapping block-image flow.
    if (child.type === 'image') {
      items.push({ image: child });
      continue;
    }
    // An image-only link participates while retaining its navigation target.
    if (
      child.type === 'link' &&
      child.children.length === 1 &&
      child.children[0].type === 'image'
    ) {
      items.push({ image: child.children[0], linkUrl: child.url });
      continue;
    }
    return null;
  }

  return items.length > 0 ? items : null;
}

/** Extracts block-image items from an image-only paragraph. */
function getBlockImageItems(node: SupramarkNode): BlockImageItem[] | null {
  // Only paragraphs participate; other block types retain their existing layout.
  if (node.type !== 'paragraph') {
    return null;
  }
  return collectImageItems(node.children);
}

/** Renders one horizontally scrollable row of stable block-image containers. */
function renderImageGallery(
  items: BlockImageItem[],
  key: number,
  styles: ReturnType<typeof mergeStyles>
): RenderedNode {
  // Shared gallery context: every image in this adjacent group, so the host
  // can open a swipeable preview covering the whole group.
  const galleryImages: SupramarkImageRef[] = items.map(item => ({
    url: item.image.url,
    alt: item.image.alt || undefined,
    title: item.image.title || undefined,
  }));

  // One image needs no horizontal gesture surface, so outer scrolling stays completely direct.
  if (items.length === 1) {
    const item = items[0];
    return (
      <MarkdownImage
        key={key}
        image={item.image}
        linkUrl={item.linkUrl}
        styles={styles}
        galleryImages={galleryImages}
        galleryIndex={0}
      />
    );
  }

  // Match the viewport to the reserved image height so a flex parent cannot
  // create blank space — unless the host explicitly set a viewport height.
  // The cast is needed because mergeStyles returns the narrow defaultStyles
  // literal type, which does not declare height on imageGalleryViewport.
  const viewportHeight =
    (styles.imageGalleryViewport as { height?: number }).height ?? styles.imageContainer.height;
  const galleryViewportStyle = {
    ...styles.imageGalleryViewport,
    height: viewportHeight,
  };
  return (
    <ScrollView
      key={key}
      horizontal
      directionalLockEnabled
      nestedScrollEnabled
      style={galleryViewportStyle}
      contentContainerStyle={styles.imageGallery}
    >
      {items.map((item, index) => (
        <MarkdownImage
          key={index}
          image={item.image}
          linkUrl={item.linkUrl}
          styles={styles}
          galleryImages={galleryImages}
          galleryIndex={index}
        />
      ))}
    </ScrollView>
  );
}

/** Groups consecutive top-level image-only paragraphs into one horizontal image flow. */
function renderRootNodes(
  nodes: SupramarkNode[],
  styles: ReturnType<typeof mergeStyles>,
  highlighted: ReadonlyMap<string, SupramarkCodeHighlightResult>,
  config?: SupramarkConfig,
  onOpenHtmlPage?: (node: SupramarkContainerNode) => void,
  containerRenderers?: Record<string, ContainerRendererRN>
): RenderedNode[] {
  const rendered: RenderedNode[] = [];
  let index = 0;
  while (index < nodes.length) {
    const firstItems = getBlockImageItems(nodes[index]);
    if (!firstItems) {
      rendered.push(
        renderNode(
          nodes[index],
          index,
          styles,
          highlighted,
          config,
          onOpenHtmlPage,
          containerRenderers
        )
      );
      index += 1;
      continue;
    }

    // Consume only the contiguous image-only run so ordinary blocks remain untouched.
    const galleryItems = [...firstItems];
    const galleryKey = index;
    index += 1;
    while (index < nodes.length) {
      const nextItems = getBlockImageItems(nodes[index]);
      if (!nextItems) break;
      galleryItems.push(...nextItems);
      index += 1;
    }
    rendered.push(renderImageGallery(galleryItems, galleryKey, styles));
  }
  return rendered;
}

/** Renders a block image without changing its measured size when the bitmap finishes loading. */
function MarkdownImage({
  image,
  linkUrl,
  styles,
  galleryImages,
  galleryIndex,
}: {
  image: SupramarkImageNode;
  linkUrl?: string;
  styles: ReturnType<typeof mergeStyles>;
  galleryImages: SupramarkImageRef[];
  galleryIndex: number;
}): RenderedNode {
  const onImagePress = useContext(ImagePressContext);
  // An empty or relative URL cannot be loaded as a remote source; show the
  // placeholder up front instead of a blank 200×200 hole.
  const loadable = hasLoadableImageUrl(image.url);
  // Failure is tracked per-URL: galleries key images by index, so the same
  // component instance can receive a different image.url on re-render. A
  // boolean flag would keep showing the placeholder for a URL that never
  // failed; binding the failure to the URL that produced it resets for free.
  const [failedUrl, setFailedUrl] = useState<string | null>(null);
  const failed = failedUrl === image.url;

  // A TouchableOpacity becomes the accessible element for its subtree, so the
  // label must live on the wrapper or it is easy to lose. When wrapped, the
  // inner image/placeholder stops being individually focusable instead.
  const wrapped = Boolean(linkUrl || onImagePress);
  // Empty alt + no title marks the image as decorative for screen readers —
  // but only when nothing actionable wraps it. The wrapper IS the link /
  // press control: hiding it (accessibilityElementsHidden +
  // no-hide-descendants) would make the control unreachable, so a decorative
  // wrapped image instead keeps the wrapper exposed and falls back to the
  // link URL for its accessible name.
  const isDecorative = !image.alt && !image.title && !wrapped;
  const accessibilityLabel = image.alt || image.title || linkUrl || undefined;
  const accessibilityProps: {
    accessibilityLabel?: string;
    accessibilityElementsHidden: boolean;
    importantForAccessibility: 'yes' | 'no-hide-descendants';
  } = {
    accessibilityLabel,
    accessibilityElementsHidden: isDecorative,
    importantForAccessibility: isDecorative ? 'no-hide-descendants' : 'yes',
  };
  const innerAccessibilityProps = wrapped ? { accessible: false } : accessibilityProps;

  const imageContent = (
    <View style={styles.imageContainer}>
      {loadable && !failed ? (
        <Image
          source={{ uri: image.url }}
          {...innerAccessibilityProps}
          style={styles.image}
          onError={() => setFailedUrl(image.url)}
        />
      ) : (
        <View style={styles.imagePlaceholder} {...innerAccessibilityProps}>
          <Text style={styles.imagePlaceholderText}>
            {imageFallbackLabel(image)}
          </Text>
        </View>
      )}
    </View>
  );

  const handlePress = () =>
    onImagePress?.({
      url: image.url,
      alt: image.alt || undefined,
      title: image.title || undefined,
      linkUrl,
      galleryImages,
      galleryIndex,
    });

  // An image link delegates to the host when it supplied a press handler;
  // otherwise it opens the URL via Linking (preserves existing behavior).
  if (linkUrl) {
    return (
      <TouchableOpacity
        onPress={() => {
          if (onImagePress) {
            handlePress();
            return;
          }
          Linking.openURL(linkUrl).catch(err => console.error('Failed to open URL:', err));
        }}
        {...accessibilityProps}
      >
        {imageContent}
      </TouchableOpacity>
    );
  }

  // A standalone image is tappable only when the host supplied a press handler.
  if (onImagePress) {
    return (
      <TouchableOpacity onPress={handlePress} {...accessibilityProps}>
        {imageContent}
      </TouchableOpacity>
    );
  }

  return imageContent;
}

function renderNode(
  node: SupramarkNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  highlighted: ReadonlyMap<string, SupramarkCodeHighlightResult>,
  config?: SupramarkConfig,
  onOpenHtmlPage?: (node: SupramarkContainerNode) => void,
  containerRenderers?: Record<string, ContainerRendererRN>,
  listMarker?: string
): RenderedNode {
  switch (node.type) {
    case 'paragraph': {
      const blockImages = getBlockImageItems(node);
      // Image-only paragraphs (including nested ones reached via renderNode) render
      // as a stable block gallery instead of 20×20 inlines.
      if (blockImages) {
        return renderImageGallery(blockImages, key, styles);
      }
      return (
        <Text key={key} style={styles.paragraph}>
          {renderInlineNodes(node.children, styles, highlighted, config)}
        </Text>
      );
    }
    case 'heading': {
      const heading = node;
      return (
        <Text key={key} style={headingStyle(heading.depth, styles)}>
          {renderInlineNodes(heading.children, styles, highlighted, config)}
        </Text>
      );
    }
    case 'code': {
      const codeBlock = node;
      return renderCodeBlock(codeBlock, key, styles, highlighted);
    }
    case 'math_block': {
      const mathBlock = node;
      // If the Math feature is disabled, fall back to a plain code block showing raw TeX
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-math'])) {
        return renderDisabledMathBlock(mathBlock, key, styles);
      }
      return <MathBlock key={key} node={mathBlock} timeoutMs={config?.diagram?.defaultTimeoutMs} />;
    }
    case 'list': {
      const list = node;
      const startIndex = list.start ?? 1;
      return (
        <View key={key} style={styles.list}>
          {list.children.map((item, index) =>
            renderNode(
              item,
              index,
              styles,
              highlighted,
              config,
              onOpenHtmlPage,
              containerRenderers,
              list.ordered ? `${startIndex + index}.` : '•'
            )
          )}
        </View>
      );
    }
    case 'list_item': {
      const item = node;
      const isTaskList = item.checked !== undefined;
      const checkSymbol = item.checked === true ? '☑' : '☐';
      const marker = `${listMarker ?? '•'} ${isTaskList ? `${checkSymbol} ` : ''}`;

      // Tight list (inline-only children). Width-safe column layout (see #101).
      // An image-only tight item gets the same stable block container as a
      // standalone image paragraph, instead of degrading to 20×20 inlines.
      if (item.children.every(isInlineNode)) {
        const blockImages = collectImageItems(item.children);
        if (blockImages) {
          return (
            <View key={key} style={styles.listItemBlock}>
              <Text style={styles.paragraph}>{marker}</Text>
              <View style={styles.listItemIndent}>
                {renderImageGallery(blockImages, 0, styles)}
              </View>
            </View>
          );
        }
        return (
          <Text key={key} style={styles.paragraph}>
            {marker}
            {renderInlineNodes(item.children, styles, highlighted, config)}
          </Text>
        );
      }

      // Loose / nested list (block children): render via renderNode so paragraph
      // and nested list don't fall through to null. Column layout keeps <Text>
      // children stretching to full width (width-safe, unlike row + flex:1).
      return (
        <View key={key} style={styles.listItemBlock}>
          {renderListItemBody(
            item.children,
            marker,
            styles,
            highlighted,
            config,
            onOpenHtmlPage,
            containerRenderers
          )}
        </View>
      );
    }
    case 'diagram': {
      const diagram = node;
      // If the config explicitly disables the corresponding diagram feature, fall back to code-block rendering
      if (!isDiagramFeatureEnabled(config, diagram.engine, 'rn:diagram-feature')) {
        return renderDisabledDiagram(diagram, key, styles);
      }
      return (
        <DiagramNode
          key={key}
          node={diagram}
          diagramConfig={config?.diagram}
          globalCache={config?.options?.cache}
        />
      );
    }
    case 'container': {
      const container = node;
      const containerName = container.name;

      // Vertical block container: the html card (title + hint), and block
      // children of unrecognized containers.
      // Don't reuse styles.listItem — it's a row layout that would lay out
      // block children horizontally with no spacing.
      const blockContainerStyle = { flexDirection: 'column' as const, gap: 8 };

      // Check whether a custom renderer is registered
      if (containerRenderers && containerRenderers[containerName]) {
        return containerRenderers[containerName]({
          node: container,
          key,
          styles,
          config,
          onOpenHtmlPage,
          renderNode: (n, k) =>
            renderNode(n, k, styles, highlighted, config, onOpenHtmlPage, containerRenderers),
          renderChildren: children =>
            children.map((child, index) =>
              renderNode(
                child,
                index,
                styles,
                highlighted,
                config,
                onOpenHtmlPage,
                containerRenderers
              )
            ),
        });
      }

      // Built-in handling: map type
      if (containerName === 'map') {
        return renderMapNodeFromContainer(container, key, styles, config);
      }

      // Built-in handling: html type
      if (containerName === 'html') {
        const data = container.data || {};
        const title = (data.title as string) || container.params || '[HTML Page]';
        const content = (
          <View style={blockContainerStyle}>
            <Text style={{ fontWeight: '600', lineHeight: 20 }}>{title}</Text>
            <Text style={{ lineHeight: 20 }}>
              Tap the card to open the HTML page in a standalone container (requires the host to
              implement the onOpenHtmlPage callback).
            </Text>
          </View>
        );

        if (!onOpenHtmlPage) {
          return <View key={key}>{content}</View>;
        }

        return (
          <TouchableOpacity key={key} activeOpacity={0.8} onPress={() => onOpenHtmlPage(container)}>
            {content}
          </TouchableOpacity>
        );
      }

      // Built-in handling: admonition types (note, tip, warning, etc.)
      // An opaque container's children are already pre-parsed and filled in
      // by expandOpaqueContainers in the main component's useEffect
      // (parse(value) → children). Here we render children directly.
      // Column layout: title on one line, body on the next.
      if (
        SUPRAMARK_ADMONITION_KINDS.includes(
          containerName as (typeof SUPRAMARK_ADMONITION_KINDS)[number]
        )
      ) {
        const title = container.params || (container.data?.title as string | undefined);
        const kind = containerName;
        const admonitionContainerStyle = { flexDirection: 'column' as const, gap: 4 };

        const renderAdmonitionContent = () =>
          container.children.map((child, index) =>
            renderNode(
              child,
              index,
              styles,
              highlighted,
              config,
              onOpenHtmlPage,
              containerRenderers
            )
          );

        if (!isFeatureGroupEnabled(config, ['@supramark/feature-admonition'])) {
          return (
            <View key={key} style={admonitionContainerStyle}>
              {title ? <Text style={styles.paragraph}>{title}</Text> : null}
              {renderAdmonitionContent()}
            </View>
          );
        }

        const adOptions =
          getFeatureOptionsAs<{ kinds?: string[] }>(config, '@supramark/feature-admonition') ?? {};
        if (Array.isArray(adOptions.kinds) && adOptions.kinds.length > 0) {
          if (!adOptions.kinds.includes(kind)) {
            return (
              <View key={key} style={admonitionContainerStyle}>
                {title ? <Text style={styles.paragraph}>{title}</Text> : null}
                {renderAdmonitionContent()}
              </View>
            );
          }
        }

        return (
          <View key={key} style={admonitionContainerStyle}>
            {title ? (
              <Text style={[styles.paragraph, { fontWeight: '600' }]}>{title}</Text>
            ) : null}
            {renderAdmonitionContent()}
          </View>
        );
      }

      // Default: render as a generic container block
      return (
        <View key={key} style={blockContainerStyle}>
          {container.params && (
            <Text style={{ fontWeight: '600', lineHeight: 20 }}>
              {container.name}: {container.params}
            </Text>
          )}
          {container.children.map((child, index) =>
            renderNode(
              child,
              index,
              styles,
              highlighted,
              config,
              onOpenHtmlPage,
              containerRenderers
            )
          )}
        </View>
      );
    }
    case 'definition_list': {
      const list = node;
      const defOptions =
        getFeatureOptionsAs<{ compact?: boolean }>(config, '@supramark/feature-definition-list') ??
        {};
      const isCompact = defOptions.compact !== false; // Compact by default
      // Column layout: term on one line, description indented on the next.
      // This avoids a row layout squeezing the description against the term
      // and causing Text to fail to wrap.
      const defItemStyle = { flexDirection: 'column' as const };
      const defDescriptionStyle = { paddingLeft: 16, gap: 8 };
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-definition-list'])) {
        // When disabled, degrade the definition list to a plain list style
        return (
          <View key={key} style={styles.list}>
            {list.children.map((item, index) => {
              const defItem = item;
              const terms = getDefinitionTerms(defItem);
              const descriptions = getDefinitionDescriptions(defItem);
              return (
                <View key={index} style={defItemStyle}>
                  {terms.map((term, termIndex) => (
                    <Text
                      key={`term-${termIndex}`}
                      style={[styles.paragraph, { fontWeight: '600' }]}
                    >
                      {renderInlineNodes(term.children, styles, highlighted, config)}
                    </Text>
                  ))}
                  {descriptions.map((description, descriptionIndex) => (
                    <View key={`description-${descriptionIndex}`} style={defDescriptionStyle}>
                      {description.children.map((child, childIndex) =>
                        renderNode(
                          child,
                          childIndex,
                          styles,
                          highlighted,
                          config,
                          onOpenHtmlPage,
                          containerRenderers
                        )
                      )}
                    </View>
                  ))}
                </View>
              );
            })}
          </View>
        );
      }
      return (
        <View key={key} style={styles.list}>
          {list.children.map((item, index) => {
            const defItem = item;
            const terms = getDefinitionTerms(defItem);
            const descriptions = getDefinitionDescriptions(defItem);
            return (
              <View key={index} style={defItemStyle}>
                {terms.map((term, termIndex) => (
                  <Text key={`term-${termIndex}`} style={[styles.paragraph, { fontWeight: '600' }]}>
                    {renderInlineNodes(term.children, styles, highlighted, config)}
                  </Text>
                ))}
                {descriptions.map((description, idx) => (
                  <View key={idx} style={defDescriptionStyle}>
                    {description.children.map((child, childIndex) =>
                      renderNode(
                        child,
                        childIndex,
                        styles,
                        highlighted,
                        config,
                        onOpenHtmlPage,
                        containerRenderers
                      )
                    )}
                    {isCompact ? null : <Text style={styles.paragraph}>{'\n'}</Text>}
                  </View>
                ))}
              </View>
            );
          })}
        </View>
      );
    }
    case 'footnote_definition': {
      const def = node;
      // In AST v2, footnote_definition.children are block nodes (paragraph,
      // etc.), not inline nodes. Use renderNode to render children so
      // renderInlineNodes doesn't skip block nodes.
      const renderFootnoteContent = () =>
        def.children.map((child, childIndex) =>
          renderNode(
            child,
            childIndex,
            styles,
            highlighted,
            config,
            onOpenHtmlPage,
            containerRenderers
          )
        );
      // Phase one: simply append as "[n] content" at the end of the text
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-footnote'])) {
        // When the footnote feature is disabled, render footnote content without
        // the [n] bullet. Use the same View/gap wrapper as the enabled branch so
        // an image-only paragraph does not place a View under <Text> (RN invariant).
        return (
          <View key={key} style={styles.listItem}>
            <View style={[styles.listItemText, { gap: 8 }]}>{renderFootnoteContent()}</View>
          </View>
        );
      }
      return (
        <View key={key} style={styles.listItem}>
          <Text style={styles.bullet}>[{def.index}]</Text>
          <View style={[styles.listItemText, { gap: 8 }]}>{renderFootnoteContent()}</View>
        </View>
      );
    }
    case 'table': {
      const table = node;
      const screenWidth = Dimensions.get('window').width;
      return (
        <View key={key} style={[styles.table, { width: screenWidth }]}>
          {table.children.map((row, index) =>
            renderNode(row, index, styles, highlighted, config, onOpenHtmlPage, containerRenderers)
          )}
        </View>
      );
    }
    case 'table_row': {
      const row = node;
      return (
        <View key={key} style={styles.tableRow}>
          {row.children.map((cell, index) =>
            renderNode(cell, index, styles, highlighted, config, onOpenHtmlPage, containerRenderers)
          )}
        </View>
      );
    }
    case 'table_cell': {
      const cell = node;
      const cellStyle = [styles.tableCell, cell.header && styles.tableHeaderCell];
      const textStyle = [
        styles.tableCellText,
        cell.header && styles.tableHeaderText,
        cell.align === 'center' && styles.textCenter,
        cell.align === 'right' && styles.textRight,
      ];

      return (
        <View key={key} style={cellStyle}>
          <Text style={textStyle}>
            {renderInlineNodes(cell.children, styles, highlighted, config)}
          </Text>
        </View>
      );
    }
    case 'blockquote': {
      const quote = node;
      return (
        <View key={key} style={styles.blockquote}>
          {quote.children.map((child, i) =>
            renderNode(child, i, styles, highlighted, config, onOpenHtmlPage, containerRenderers)
          )}
        </View>
      );
    }
    case 'thematic_break': {
      return <View key={key} style={styles.thematicBreak} />;
    }
    case 'text':
      return (
        <Text key={key} style={styles.paragraph}>
          {node.value}
        </Text>
      );
    default:
      return null;
  }
}

function renderCodeBlock(
  codeBlock: SupramarkCodeNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  highlighted: ReadonlyMap<string, SupramarkCodeHighlightResult>
): RenderedNode {
  const highlight = highlighted.get(
    buildCodeHighlightKey(codeBlock.value, codeBlock.lang, codeBlock.meta)
  );

  if (!highlight) {
    return (
      <View key={key} style={styles.codeBlock}>
        <Text style={styles.code}>{codeBlock.value}</Text>
      </View>
    );
  }

  return (
    <View key={key} style={styles.codeBlock}>
      <Text style={styles.code}>
        {highlight.lines.map((line, lineIndex) => (
          <Text key={lineIndex}>
            {line.tokens.map((token, tokenIndex) => (
              <Text key={tokenIndex} style={codeTokenTextStyle(token)}>
                {token.text}
              </Text>
            ))}
            {lineIndex < highlight.lines.length - 1 ? '\n' : null}
          </Text>
        ))}
      </Text>
    </View>
  );
}

function codeTokenTextStyle(token: {
  color?: string;
  backgroundColor?: string;
  fontStyle?: Array<'bold' | 'italic' | 'underline'>;
}) {
  const fontStyle = token.fontStyle ?? [];
  return {
    color: token.color,
    backgroundColor: token.backgroundColor,
    fontWeight: fontStyle.includes('bold') ? ('700' as const) : undefined,
    fontStyle: fontStyle.includes('italic') ? ('italic' as const) : undefined,
    textDecorationLine: fontStyle.includes('underline') ? ('underline' as const) : undefined,
  };
}

// Block node types handled by renderNode's switch below. Keep in sync with the
// switch: every case must be listed here, or the parse-smoke test will flag a
// shape drift between the real parser output and what the renderer renders.
export const BLOCK_NODE_TYPES: ReadonlySet<string> = new Set([
  'paragraph',
  'heading',
  'code',
  'math_block',
  'list',
  'list_item',
  'diagram',
  'container',
  'definition_list',
  'footnote_definition',
  'table',
  'table_row',
  'table_cell',
  'blockquote',
  'thematic_break',
  'text',
]);

// Inline node types — keep in sync with renderInlineNode's switch below: any
// inline type handled there must be listed here, or list_item will mistake it
// for a block and route it through renderNode.
export const INLINE_NODE_TYPES: ReadonlySet<string> = new Set([
  'text',
  'strong',
  'emphasis',
  'inline_code',
  'math_inline',
  'link',
  'image',
  'break',
  'delete',
  'footnote_reference',
  'raw',
]);

function isInlineNode(node: SupramarkNode): boolean {
  return INLINE_NODE_TYPES.has(node.type);
}

// Render list_item children that mix inline and block nodes (loose / nested
// lists). Inline runs collapse into one <Text> (the first run gets the marker);
// block nodes (paragraph, nested list) go through renderNode instead of being
// dropped by renderInlineNodes' default→null.
function renderListItemBody(
  children: SupramarkNode[],
  marker: string,
  styles: ReturnType<typeof mergeStyles>,
  highlighted: ReadonlyMap<string, SupramarkCodeHighlightResult>,
  config: SupramarkConfig | undefined,
  onOpenHtmlPage: ((node: SupramarkContainerNode) => void) | undefined,
  containerRenderers: Record<string, ContainerRendererRN> | undefined
): RenderedNode[] {
  const out: RenderedNode[] = [];
  let inlineBuf: SupramarkNode[] = [];
  let markerPending = true;
  let seq = 0;

  const flushInline = () => {
    if (inlineBuf.length === 0) return;
    const prefix = markerPending ? marker : '';
    out.push(
      <Text key={`li-${seq}`} style={styles.paragraph}>
        {prefix}
        {inlineBuf.map((n, i) => renderInlineNode(n, i, styles, highlighted, config))}
      </Text>
    );
    seq += 1;
    inlineBuf = [];
    markerPending = false;
  };

  for (const child of children) {
    if (isInlineNode(child)) {
      inlineBuf.push(child);
      continue;
    }
    // Prefix the marker onto the first paragraph's inline content (loose lists).
    if (child.type === 'paragraph' && markerPending) {
      flushInline();
      // An image-only first paragraph gets the stable block container too, so
      // a loose item's leading image is not squeezed to 20×20 beside the marker.
      const blockImages = getBlockImageItems(child);
      if (blockImages) {
        out.push(
          <View key={`li-${seq}`} style={styles.listItemBlock}>
            <Text style={styles.paragraph}>{marker}</Text>
            <View style={styles.listItemIndent}>{renderImageGallery(blockImages, 0, styles)}</View>
          </View>
        );
      } else {
        out.push(
          <Text key={`li-${seq}`} style={styles.paragraph}>
            {marker}
            {renderInlineNodes(child.children, styles, highlighted, config)}
          </Text>
        );
      }
      seq += 1;
      markerPending = false;
      continue;
    }
    // Other blocks (nested list, subsequent paragraph): indent to align under
    // the marker, then render via renderNode.
    flushInline();
    out.push(
      <View key={`li-${seq}`} style={styles.listItemIndent}>
        {renderNode(child, 0, styles, highlighted, config, onOpenHtmlPage, containerRenderers)}
      </View>
    );
    seq += 1;
  }
  flushInline();
  return out;
}

function renderInlineNodes(
  nodes: SupramarkNode[],
  styles: ReturnType<typeof mergeStyles>,
  highlighted: ReadonlyMap<string, SupramarkCodeHighlightResult>,
  config?: SupramarkConfig,
  parentType?: string
): RenderedNode {
  return nodes.map((node, index) =>
    renderInlineNode(node, index, styles, highlighted, config, parentType)
  );
}

function renderInlineNode(
  node: SupramarkNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  highlighted: ReadonlyMap<string, SupramarkCodeHighlightResult>,
  config?: SupramarkConfig,
  parentType?: string
): RenderedNode {
  switch (node.type) {
    case 'text': {
      const textNode = node;
      return textNode.value;
    }
    case 'strong': {
      const strongNode = node;
      // cmark-gfm 0.29 flattens a strong whose parent is strong; CommonMark
      // 0.31 keeps the nesting. The two references diverge, so flattening is
      // opt-in via `options.flattenNestedStrong` (mirrors the web renderer).
      // RN nested <Text style={strong}> renders bold-on-bold (= bold) either
      // way, so this is a behavioral-consistency no-op visually, but it keeps
      // the same config from yielding structurally different output per
      // platform.
      if (parentType === 'strong' && config?.options?.flattenNestedStrong === true) {
        return renderInlineNodes(
          strongNode.children,
          styles,
          highlighted,
          config,
          'strong'
        );
      }
      return (
        <Text key={key} style={styles.strong}>
          {renderInlineNodes(strongNode.children, styles, highlighted, config, 'strong')}
        </Text>
      );
    }
    case 'emphasis': {
      const emphasisNode = node;
      return (
        <Text key={key} style={styles.emphasis}>
          {renderInlineNodes(emphasisNode.children, styles, highlighted, config)}
        </Text>
      );
    }
    case 'inline_code': {
      const codeNode = node;
      return (
        <Text key={key} style={styles.inlineCode}>
          {codeNode.value}
        </Text>
      );
    }
    case 'math_inline': {
      const mathNode = node;
      if (!isFeatureGroupEnabled(config, ['@supramark/feature-math'])) {
        return mathNode.value;
      }
      return (
        <MathInline
          key={key}
          value={mathNode.value}
          textStyle={styles.paragraph}
          timeoutMs={config?.diagram?.defaultTimeoutMs}
        />
      );
    }
    case 'link': {
      const linkNode = node;
      return (
        <Text
          key={key}
          style={styles.link}
          onPress={() => {
            Linking.openURL(linkNode.url).catch(err => console.error('Failed to open URL:', err));
          }}
        >
          {renderInlineNodes(linkNode.children, styles, highlighted, config)}
        </Text>
      );
    }
    case 'image': {
      const imageNode = node;
      // Empty alt + no title marks the image as decorative for screen readers.
      const isDecorative = !imageNode.alt && !imageNode.title;
      // Mixed-content / table / heading images go through here; the same
      // loadability rule as the block path applies — an empty or relative src
      // would render as a blank 20×20 hole, so show the alt text instead.
      if (!hasLoadableImageUrl(imageNode.url)) {
        // Inherits the surrounding Text style; mirrors the block placeholder
        // label without breaking the inline flow.
        return <Text key={key}>{imageFallbackLabel(imageNode)}</Text>;
      }
      return (
        <Image
          key={key}
          source={{ uri: imageNode.url }}
          accessibilityLabel={imageNode.alt || imageNode.title || undefined}
          accessibilityElementsHidden={isDecorative}
          importantForAccessibility={isDecorative ? 'no-hide-descendants' : 'yes'}
          style={styles.inlineImage}
        />
      );
    }
    case 'break': {
      return '\n';
    }
    case 'delete': {
      const deleteNode = node;
      return (
        <Text key={key} style={styles.delete}>
          {renderInlineNodes(deleteNode.children, styles, highlighted, config)}
        </Text>
      );
    }
    case 'footnote_reference': {
      const ref = node;
      const label = ref.index;
      if (!isFeatureGroupEnabled(undefined, ['@supramark/feature-footnote'])) {
        return `[${label}]`;
      }
      return (
        <Text key={key} style={styles.inlineCode}>
          [{label}]
        </Text>
      );
    }
    case 'raw': {
      // Inline HTML (e.g. `<span>`). RN can't render arbitrary HTML inline, and
      // dropping the tag keeps the surrounding text in one <Text> flow — see
      // issue #125. Block-level raw HTML falls through renderNode's default.
      return null;
    }
    default:
      return null;
  }
}

function headingStyle(
  depth: SupramarkHeadingNode['depth'],
  styles: ReturnType<typeof mergeStyles>
) {
  switch (depth) {
    case 1:
      return styles.h1;
    case 2:
      return styles.h2;
    case 3:
      return styles.h3;
    case 4:
      return styles.h4;
    case 5:
      return styles.h5;
    case 6:
      return styles.h6;
    default:
      return styles.h4;
  }
}

/**
 * Determines whether a group of feature IDs is enabled.
 *
 * Convention:
 * - No config, or an empty config.features → treated as all enabled;
 * - If config doesn't mention any of these IDs at all → treated as default
 *   behavior (enabled);
 * - Once any of these IDs is explicitly configured, config wins: as long as
 *   one of them has enabled:true, the group is considered enabled.
 */
function isFeatureGroupEnabled(config: SupramarkConfig | undefined, ids: string[]): boolean {
  if (!config || !config.features || config.features.length === 0) {
    return true;
  }

  const hasAny = ids.some(id => config.features!.some(f => f.id === id));
  if (!hasAny) {
    return true;
  }

  return ids.some(id => isFeatureEnabled(config, id));
}

function collectCodeHighlightTasks(
  nodes: SupramarkNode[],
  config?: SupramarkConfig,
  theme?: string
): Array<{ key: string; code: string; lang?: string; meta?: string; theme?: string }> {
  if (!isFeatureGroupEnabled(config, ['@supramark/feature-code-highlight'])) {
    return [];
  }

  const tasks: Array<{ key: string; code: string; lang?: string; meta?: string; theme?: string }> =
    [];

  function walk(list: SupramarkNode[]) {
    for (const node of list) {
      if (node.type === 'code') {
        const code = node;
        tasks.push({
          key: buildCodeHighlightKey(code.value, code.lang, code.meta),
          code: code.value,
          lang: code.lang,
          meta: code.meta,
          theme,
        });
      }

      if ('children' in node && Array.isArray((node as { children?: SupramarkNode[] }).children)) {
        walk((node as { children: SupramarkNode[] }).children);
      }
    }
  }

  walk(nodes);
  return tasks;
}

async function preHighlightAll(
  tasks: Array<{ key: string; code: string; lang?: string; meta?: string; theme?: string }>,
  highlighter?: SupramarkCodeHighlighter
): Promise<Map<string, SupramarkCodeHighlightResult>> {
  if (!highlighter || tasks.length === 0) {
    return new Map();
  }

  const unique = new Map<
    string,
    { key: string; code: string; lang?: string; meta?: string; theme?: string }
  >();
  for (const task of tasks) {
    if (!unique.has(task.key)) {
      unique.set(task.key, task);
    }
  }

  const entries = await Promise.all(
    [...unique.values()].map(async task => {
      try {
        const result = await highlighter({
          code: task.code,
          lang: task.lang,
          meta: task.meta,
          theme: task.theme,
        });
        return result ? ([task.key, result] as const) : null;
      } catch {
        return null;
      }
    })
  );

  return new Map(
    entries.filter(
      (entry): entry is readonly [string, SupramarkCodeHighlightResult] => entry !== null
    )
  );
}

function buildCodeHighlightKey(code: string, lang?: string, meta?: string): string {
  return `code:${lang ?? ''}:${meta ?? ''}:${code}`;
}

function renderDisabledDiagram(
  diagram: SupramarkDiagramNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>
): RenderedNode {
  const header = `[diagram engine="${diagram.engine}" disabled]\n\n`;
  return (
    <View key={key} style={styles.codeBlock}>
      <Text style={styles.code}>{header + diagram.code}</Text>
    </View>
  );
}

function renderDisabledMathBlock(
  math: SupramarkMathBlockNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>
): RenderedNode {
  const header = '[math disabled]\n\n';
  return (
    <View key={key} style={styles.codeBlock}>
      <Text style={styles.code}>{header + math.value}</Text>
    </View>
  );
}

function renderMapNodeFromContainer(
  container: SupramarkContainerNode,
  key: number,
  styles: ReturnType<typeof mergeStyles>,
  _config?: SupramarkConfig
): RenderedNode {
  // Extract map data from container.data
  const data = container.data || {};
  const center = (data.center as [number, number]) || [0, 0];
  const zoom = (data.zoom as number) || 12;
  const marker = data.marker as { lat: number; lng: number } | undefined;

  // Try using the real react-native-maps
  try {
    // react-native-maps is an optional dependency; keep it lazy-loaded.
    // Cast the untyped require() result to a minimal local module shape so the
    // downstream JSX usage stays type-safe without depending on the package types.
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const maps = require('react-native-maps') as ReactNativeMapsModule;
    const MapView = maps.default;
    const { Marker } = maps;

    const { width } = Dimensions.get('window');

    // Parse coordinates
    const latitude = center[0] || 0;
    const longitude = center[1] || 0;

    // Compute the map region — adjust the viewport based on zoom
    const latitudeDelta = Math.max(0.001, 0.1 * Math.pow(0.5, zoom - 8));
    const longitudeDelta = Math.max(0.001, 0.1 * Math.pow(0.5, zoom - 8));

    const region = {
      latitude,
      longitude,
      latitudeDelta,
      longitudeDelta,
    };

    const hasMarker = marker && typeof marker.lat === 'number' && typeof marker.lng === 'number';

    return (
      <View key={key} style={styles.mapCard}>
        <View style={styles.mapCardHeader}>
          <Text style={styles.mapCardTitle}>🗺️ Real Map</Text>
          <Text style={styles.mapCardSubtitle}>Powered by React Native Maps</Text>
        </View>

        <View style={styles.mapContainer}>
          <MapView
            style={[styles.map, { width: width - 32 }]}
            region={region}
            mapType="standard"
            showsUserLocation={false}
            showsMyLocationButton={false}
            zoomEnabled={true}
            scrollEnabled={true}
            rotateEnabled={true}
            pitchEnabled={false}
          >
            {/* Center marker */}
            <Marker
              coordinate={{ latitude, longitude }}
              title="Center"
              description={`Coordinates: ${latitude}, ${longitude}`}
              pinColor="red"
            />

            {/* Extra marker */}
            {hasMarker && (
              <Marker
                coordinate={{
                  latitude: marker.lat,
                  longitude: marker.lng,
                }}
                title="Marker"
                description={`Location: ${marker.lat}, ${marker.lng}`}
                pinColor="blue"
              />
            )}
          </MapView>
        </View>

        <View style={styles.mapCardContent}>
          <Text style={styles.mapCardInfo}>
            📍 Center: {latitude.toFixed(4)}, {longitude.toFixed(4)}
          </Text>
          <Text style={styles.mapCardInfo}>🔍 Zoom level: {zoom}</Text>
          {hasMarker && (
            <Text style={styles.mapCardInfo}>
              Marker: {marker.lat}, {marker.lng}
            </Text>
          )}
          <Text style={[styles.mapCardInfo, { color: '#28a745', fontWeight: '500' }]}>
            ✅ Real map enabled
          </Text>
        </View>
      </View>
    );
  } catch (error) {
    // If react-native-maps is unavailable, show a smart placeholder card
    const { width } = Dimensions.get('window');
    const centerText = center ? `${center[0]}, ${center[1]}` : 'Unspecified';
    const hasMarkerFallback =
      marker && typeof marker.lat === 'number' && typeof marker.lng === 'number';

    return (
      <View key={key} style={styles.mapCard}>
        <View style={styles.mapCardHeader}>
          <Text style={styles.mapCardTitle}>🗺️ Smart Map Card</Text>
          <Text style={styles.mapCardSubtitle}>
            Visual placeholder (react-native-maps not ready)
          </Text>
        </View>

        {/* Smart map placeholder area */}
        <View style={styles.mapContainer}>
          <View style={[styles.map, { width: width - 32 }]}>
            {/* Simulated map grid */}
            <View style={styles.mapGridOverlay}>
              {Array.from({ length: 4 }, (_, i) => (
                <View key={`h-${i}`} style={[styles.mapGridLine, { top: `${(i + 1) * 20}%` }]} />
              ))}
              {Array.from({ length: 4 }, (_, i) => (
                <View
                  key={`v-${i}`}
                  style={[
                    styles.mapGridLine,
                    styles.mapGridLineVertical,
                    { left: `${(i + 1) * 20}%` },
                  ]}
                />
              ))}
            </View>

            {/* Center marker */}
            <View style={styles.mapCenterMarker}>
              <Text style={styles.mapCenterMarkerText}>📍</Text>
            </View>

            {/* Extra marker */}
            {hasMarkerFallback && (
              <View
                style={[
                  styles.mapMarker,
                  {
                    top: '30%',
                    left: '60%',
                  },
                ]}
              >
                <Text style={styles.mapMarkerText}>📌</Text>
              </View>
            )}

            {/* Map info overlay */}
            <View style={styles.mapOverlay}>
              <Text style={styles.mapOverlayText}>Simulated {zoom}x</Text>
            </View>
          </View>
        </View>

        <View style={styles.mapCardContent}>
          <Text style={styles.mapCardInfo}>📍 Center: {centerText}</Text>
          <Text style={styles.mapCardInfo}>🔍 Zoom level: {zoom}</Text>
          {hasMarkerFallback && (
            <Text style={styles.mapCardInfo}>
              Marker: {marker.lat}, {marker.lng}
            </Text>
          )}
          <Text style={[styles.mapCardInfo, { color: '#ffc107', fontStyle: 'italic' }]}>
            ⚠️ Install react-native-maps to enable the real map
          </Text>
        </View>
      </View>
    );
  }
}
