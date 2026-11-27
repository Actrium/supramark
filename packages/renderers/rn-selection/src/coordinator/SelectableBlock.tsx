import React, { useContext, useEffect, useRef, useCallback } from 'react';
import {
  Platform,
  StyleSheet,
  Text,
  View,
  type GestureResponderEvent,
  type LayoutChangeEvent,
  type NativeSyntheticEvent,
  type StyleProp,
  type TextLayoutEventData,
  type TextStyle,
  type ViewStyle,
} from 'react-native';
import { buildTextMetrics, type SegmentTextMetrics } from '../metrics';
import type { SelectionNodeId } from '../model';
import type { SelectionUnitIndex } from '../resolve';
import type { ContentOffset, LayoutRect } from './registry';
import { SelectionMouseEnabledContext, SelectionViewportContext } from './SelectionContext';
import { InlineObjectLayoutContext, SelectableInlineObject } from './SelectableInlineObject';
import { withInlineObjectMetrics } from './inlineMetrics';
import { useNativeLineMetrics } from './useNativeLineMetrics';
import { useSelectionContext } from './useDocumentSelection';

declare const __DEV__: boolean | undefined;

// Desktop Fabric supports the CSS text cursor. Core RN's mobile typings omit it.
const textCursor: TextStyle = { cursor: 'text' as TextStyle['cursor'] };

/**
 * Concatenated string content of a React subtree. Only literal string/number
 * leaves count — which is exactly the invariant we are checking, since those
 * are what the text engine will lay out as plain text.
 */
function flattenRenderedText(node: React.ReactNode): string {
  if (node === null || node === undefined || typeof node === 'boolean') return '';
  if (typeof node === 'string') return node;
  if (typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(flattenRenderedText).join('');
  if (React.isValidElement(node)) {
    const { children } = node.props as { children?: React.ReactNode };
    return flattenRenderedText(children);
  }
  return '';
}

/**
 * Sum of the text this block's units hold, or `undefined` while the stream has
 * not indexed them all yet (the block re-rendered before the root's `setUnits`
 * effect, so there is nothing to compare against).
 */
function sumUnitTextLength(
  index: SelectionUnitIndex,
  unitIds: readonly SelectionNodeId[]
): number | undefined {
  let total = 0;
  for (const unitId of unitIds) {
    const at = index.byUnitId.get(unitId);
    if (at === undefined) return undefined;
    // Native attachments occupy one Text offset even though they carry no selectable text.
    total += index.entries[at].unit.kind === 'atom' ? 1 : index.entries[at].textLength;
  }
  return total;
}

/** Native case changes are safe only when each character keeps its UTF-16 offsets. */
function matchesTextCase(source: string, native: string): boolean {
  if (source.length !== native.length) return false;
  const nativeChars = Array.from(native);
  return Array.from(source).every((char, i) => {
    const measured = nativeChars[i];
    return (
      measured?.length === char.length &&
      (measured === char ||
        measured.toLocaleUpperCase() === char.toLocaleUpperCase() ||
        measured.toLocaleLowerCase() === char.toLocaleLowerCase())
    );
  });
}

function matchesStyledText(
  source: string,
  native: string,
  transform: TextStyle['textTransform']
): boolean {
  if (source === native) return true;
  if (!transform || transform === 'none' || !matchesTextCase(source, native)) return false;
  if (transform === 'uppercase') {
    return native === source.toUpperCase() || native === source.toLocaleUpperCase();
  }
  if (transform === 'lowercase') {
    return native === source.toLowerCase() || native === source.toLocaleLowerCase();
  }
  // Native capitalization differs by platform and locale.
  return true;
}

interface RenderedText {
  children: React.ReactNode;
  style: StyleProp<TextStyle>;
}

function matchesRenderedLayout(
  { children, style }: RenderedText,
  text: string,
  indexedText?: string
): boolean {
  let offset = 0;
  function match(node: React.ReactNode, transform: TextStyle['textTransform']): boolean {
    if (typeof node === 'string' || typeof node === 'number') {
      const source = String(node);
      const start = offset;
      offset += source.length;
      return (
        (indexedText === undefined || indexedText.startsWith(source, start)) &&
        matchesStyledText(source, text.slice(start, offset), transform)
      );
    }
    if (Array.isArray(node)) return node.every((child: React.ReactNode) => match(child, transform));
    if (!React.isValidElement(node)) return true;
    if (node.type === SelectableInlineObject) {
      if (indexedText === undefined) offset += 1;
      return true;
    }
    const props = node.props as { style?: StyleProp<TextStyle>; children?: React.ReactNode };
    return match(props.children, StyleSheet.flatten(props.style)?.textTransform ?? transform);
  }

  return match(children, StyleSheet.flatten(style)?.textTransform) && offset === text.length;
}

interface NativeLayout {
  text: string;
  metrics: SegmentTextMetrics;
}

function matchesUnitText(
  index: SelectionUnitIndex,
  unitIds: readonly SelectionNodeId[],
  text: string,
  rendered?: RenderedText
): boolean {
  let offset = 0;
  let sourceText = '';
  let nativeText = '';
  for (const unitId of unitIds) {
    const at = index.byUnitId.get(unitId);
    if (at === undefined) return false;
    const { unit } = index.entries[at];
    if (unit.kind === 'atom') {
      // Native platforms use different one-character attachment placeholders.
      offset += 1;
    } else if (unit.kind === 'text' || unit.kind === 'break') {
      const end = offset + unit.text.length;
      if (rendered) {
        sourceText += unit.text;
        nativeText += text.slice(offset, end);
      } else if (!text.startsWith(unit.text, offset)) {
        return false;
      }
      offset = end;
    }
  }
  return (
    offset === text.length && (!rendered || matchesRenderedLayout(rendered, nativeText, sourceText))
  );
}

function hasSameUnitGeometry(
  previous: SelectionUnitIndex,
  next: SelectionUnitIndex,
  unitIds: readonly SelectionNodeId[]
): boolean {
  return unitIds.every(id => {
    const previousAt = previous.byUnitId.get(id);
    const nextAt = next.byUnitId.get(id);
    if (previousAt === undefined || nextAt === undefined) return false;
    const before = previous.entries[previousAt];
    const after = next.entries[nextAt];
    return before.unit.kind === after.unit.kind && before.textLength === after.textLength;
  });
}

export interface SelectableBlockProps {
  nodeId: SelectionNodeId;
  /**
   * Native display units: text, visible hard breaks and inline attachment atoms.
   * Exclude syntax markers and trailing block separators. Every atom must render
   * through SelectableInlineObject so both platforms reserve one native offset.
   */
  unitIds: readonly SelectionNodeId[];
  /** The text subtree and explicitly measured native inline attachments. */
  children?: React.ReactNode;
  /** Text styling. Put box styling (padding, background) on `containerStyle`. */
  style?: StyleProp<TextStyle>;
  containerStyle?: StyleProp<ViewStyle>;
}

/**
 * One selectable text block.
 *
 * It renders a plain React Native `<Text>` — no native selection component.
 * With the selection UI self-drawn (see `SELECTION_PLAN.md`), the only thing
 * this block owes the coordinator is geometry, and `onTextLayout` supplies it:
 * one entry per laid-out line, as public RN API on both platforms, on Paper and
 * Fabric alike. That is what removed the vendored native dependency, its
 * Fabric-only / Android >= 0.85 floors, and the second selection state that
 * used to live inside it.
 *
 * Two measurements are published upward:
 *
 * - `onLayout` on the wrapper triggers a native measurement against
 *   `SelectionRoot`, so nested parents such as FlatList cells do not leak their
 *   local coordinate space into the registry;
 * - `onLayout` on the `<Text>` gives the content offset inside it, so padding
 *   on `containerStyle` does not shift every highlight rectangle.
 */
export const SelectableBlock: React.FC<SelectableBlockProps> = ({
  nodeId,
  unitIds,
  children,
  style,
  containerStyle,
}) => {
  const ctx = useSelectionContext();
  const mouseEnabled = useContext(SelectionMouseEnabledContext);
  const desktop = Platform.OS === 'macos' || Platform.OS === 'windows';
  const nativeId = React.useId();
  const viewportId = useContext(SelectionViewportContext);
  const blockRef = useRef<View | null>(null);
  const layoutRef = useRef<LayoutRect | null>(null);
  const metricsRef = useRef<SegmentTextMetrics | null>(null);
  // Native attachment frames and the raw line table can arrive in either order.
  const objectLayoutsRef = useRef(new Map<string, LayoutRect>());
  const nativeLayoutRef = useRef<NativeLayout | null>(null);
  const pendingLayoutsRef = useRef(new Map<string, NativeLayout>());
  const contentOffsetRef = useRef<ContentOffset | null>(null);

  // Latest unitIds, read by the mount effect so its initial registration is not
  // frozen to the first render's value.
  const unitIdsRef = useRef(unitIds);
  unitIdsRef.current = unitIds;
  const renderedRef = useRef({ children, style });
  renderedRef.current = { children, style };

  const matchesLayout = useCallback(
    (text: string) => {
      const index = ctx.registry.index;
      const ids = unitIdsRef.current;
      return (
        matchesUnitText(index, ids, text) || matchesUnitText(index, ids, text, renderedRef.current)
      );
    },
    [ctx]
  );
  const publishMetrics = useCallback(() => {
    const layout = nativeLayoutRef.current;
    const metrics =
      layout && matchesLayout(layout.text)
        ? withInlineObjectMetrics(
            layout.metrics,
            unitIdsRef.current,
            ctx.registry.index,
            objectLayoutsRef.current
          )
        : undefined;
    if (metricsRef.current === (metrics ?? null)) return;
    metricsRef.current = metrics ?? null;
    ctx.setMetrics(nodeId, metrics);
  }, [ctx, nodeId, matchesLayout]);
  // Keep this callback stable so measuring an attachment does not restart native layout.
  const onObjectLayout = useCallback(
    (id: string, rect: LayoutRect) => {
      objectLayoutsRef.current.set(id, rect);
      publishMetrics();
    },
    [publishMetrics]
  );

  useEffect(() => {
    const dispose = ctx.registerBlock({
      nodeId,
      unitIds: [...unitIdsRef.current],
      kind: 'text',
      // Nested onLayout coordinates are local to a FlatList cell, not to the
      // SelectionRoot. Preserve/await the native root-space measurement there.
      rect: viewportId === undefined ? (layoutRef.current ?? undefined) : undefined,
      metrics: metricsRef.current ?? undefined,
      contentOffset: contentOffsetRef.current ?? undefined,
      viewportId,
    });
    // Android can deliver the native onLayout before this passive effect has
    // registered the block. In that ordering the first root-space measurement
    // is correctly computed but registry.updateLayout has nowhere to store it.
    // Measure once more after registration whenever onLayout already ran.
    const layout = layoutRef.current;
    if (layout !== null && ctx.measureLayout) {
      ctx.measureLayout(nodeId, blockRef.current, layout);
    }
    return dispose;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ctx, nodeId, viewportId]);

  // Push unitId changes in place (streaming markdown grows a paragraph) without
  // re-registering, so the measured rect and line table survive. Keyed on the
  // unit contents, not identity, so a fresh array with equal ids is a no-op.
  const unitIdsKey = unitIds.join(' ');
  useEffect(() => {
    ctx.updateUnits(nodeId, [...unitIdsRef.current]);
    publishMetrics();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ctx, nodeId, unitIdsKey, publishMetrics]);

  useEffect(() => {
    let previousIndex = ctx.registry.index;
    return ctx.registry.subscribeToUnitIndex(() => {
      const index = ctx.registry.index;
      const pending = pendingLayoutsRef.current;
      const current = nativeLayoutRef.current;
      const currentMatches = current !== null && matchesLayout(current.text);
      if (!currentMatches) {
        for (const candidate of pending.values()) {
          if (matchesLayout(candidate.text)) nativeLayoutRef.current = candidate;
        }
      }
      // Unmatched layouts belong to the interval before this index update.
      pending.clear();
      // The existing line table and attachment measurements still apply when the spans match.
      const reuseMetrics =
        currentMatches &&
        metricsRef.current !== null &&
        hasSameUnitGeometry(previousIndex, index, unitIdsRef.current);
      previousIndex = index;
      if (!reuseMetrics) publishMetrics();
    });
  }, [ctx, matchesLayout, publishMetrics]);

  // Dev-only guard for the props invariant: summed unit text length must equal
  // the string content this block renders. Runs after every content change, so
  // a host that renders a marker or collapses whitespace inside the block hears
  // about it at the point of the mistake rather than through mysteriously
  // off-by-N selections later. Silent in production, where the concatenation
  // below is never built.
  const renderedText =
    typeof __DEV__ !== 'undefined' && __DEV__ ? flattenRenderedText(children) : '';
  useEffect(() => {
    if (typeof __DEV__ === 'undefined' || !__DEV__) return;
    // Attachment placeholders are native content, not literal React string children.
    if (
      unitIds.some(
        id =>
          ctx.registry.index.entries[ctx.registry.index.byUnitId.get(id) ?? -1]?.unit.kind ===
          'atom'
      )
    )
      return;
    const expected = sumUnitTextLength(ctx.registry.index, unitIds);
    if (expected === undefined || expected === renderedText.length) return;
    console.warn(
      `[supramark] SelectableBlock "${nodeId}": unitIds sum to ${expected} UTF-16 units ` +
        `but the block renders ${renderedText.length} (${JSON.stringify(renderedText.slice(0, 40))}). ` +
        'Selection offsets will be wrong past the first divergence. ' +
        "The children must render exactly the units' concatenated text — no markers, " +
        'no extra whitespace, no text from nodes outside unitIds.'
    );
  }, [ctx, nodeId, unitIdsKey, renderedText, unitIds]);

  const onBlockLayout = (e: LayoutChangeEvent) => {
    const { x, y, width, height } = e.nativeEvent.layout;
    const fallback = { x, y, w: width, h: height };
    layoutRef.current = fallback;
    // A nested cell may re-fire onLayout during virtualization or scrolling.
    // Publishing that cell-local fallback would overwrite the synchronously
    // translated root-space rect and make the highlight jump toward y=0 until
    // the asynchronous native measure arrives.
    if (viewportId === undefined || !ctx.measureLayout) {
      ctx.updateLayout(nodeId, fallback);
    }
    if (ctx.measureLayout) {
      ctx.measureLayout(nodeId, blockRef.current, fallback);
    }
  };

  const onTextBoxLayout = (e: LayoutChangeEvent) => {
    const { x, y } = e.nativeEvent.layout;
    const offset = { x, y };
    contentOffsetRef.current = offset;
    ctx.setContentOffset(nodeId, offset);
  };

  const publishLines = useNativeLineMetrics(lines => {
    const text = lines.map(line => line.text).join('');
    const previous = nativeLayoutRef.current;
    const rendered = renderedRef.current;
    const matchesRender = (layoutText: string) =>
      matchesRenderedLayout(rendered, layoutText) ||
      matchesUnitText(ctx.registry.index, unitIdsRef.current, layoutText, rendered);
    // Keep a measurement of the current text when a different revision arrives late.
    if (
      previous &&
      previous.text !== text &&
      matchesRender(previous.text) &&
      !matchesRender(text)
    ) {
      return;
    }

    const metrics = buildTextMetrics(lines);
    const layout = { text, metrics };
    // Keep a layout that arrived before the index, even if an old layout follows it.
    if (!matchesLayout(text)) {
      const pending = pendingLayoutsRef.current;
      pending.delete(text);
      pending.set(text, layout);
    }
    nativeLayoutRef.current = layout;
    publishMetrics();
    if (typeof __DEV__ !== 'undefined' && __DEV__) {
      const expected = sumUnitTextLength(ctx.registry.index, unitIdsRef.current);
      if (expected !== undefined && metrics.textLength !== expected) {
        console.warn(
          `[supramark] SelectableBlock "${nodeId}": the text engine laid out ` +
            `${metrics.textLength} UTF-16 units but the selection units hold ${expected}. ` +
            'Hit testing falls back to the whole block until the rendered text matches the units.'
        );
      }
    }
  });
  const onTextLayout = (e: NativeSyntheticEvent<TextLayoutEventData>) =>
    publishLines(e.nativeEvent.target, e.nativeEvent.lines);

  // Android can hand a nested ScrollView the native responder before the
  // bubbling root timer fires, so Text supplies a local fallback there. iOS
  // does not need it, and attaching `onLongPress` to Text produces an unwanted
  // grey pressed-state flash in a FlatList cell.
  const onNestedLongPress =
    viewportId === undefined || Platform.OS !== 'android'
      ? undefined
      : (e: GestureResponderEvent) => {
          const { locationX, locationY } = e.nativeEvent;
          ctx.selectWordInBlock(nodeId, { x: locationX, y: locationY });
        };

  return (
    <InlineObjectLayoutContext.Provider value={onObjectLayout}>
      <View ref={blockRef} collapsable={false} style={containerStyle} onLayout={onBlockLayout}>
        <Text
          nativeID={Platform.OS === 'windows' ? `supramark-selection-${nativeId}` : undefined}
          style={desktop && mouseEnabled ? [style, textCursor] : style}
          onLayout={onTextBoxLayout}
          onTextLayout={onTextLayout}
          onLongPress={onNestedLongPress}
        >
          {/* Cast: react-native resolves its own copy of @types/react, whose
            ReactNode differs from ours by `bigint`. Structurally identical for
            everything a text block can hold. */}
          {children as React.ComponentProps<typeof Text>['children']}
        </Text>
      </View>
    </InlineObjectLayoutContext.Provider>
  );
};
