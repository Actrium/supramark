import React, { useContext, useEffect, useRef, useCallback } from 'react';
import {
  Platform,
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
import { SelectionViewportContext } from './SelectionContext';
import { InlineObjectLayoutContext } from './SelectableInlineObject';
import { withInlineObjectMetrics } from './inlineMetrics';
import { useSelectionContext } from './useDocumentSelection';

declare const __DEV__: boolean | undefined;

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
  const viewportId = useContext(SelectionViewportContext);
  const blockRef = useRef<View | null>(null);
  const layoutRef = useRef<LayoutRect | null>(null);
  const metricsRef = useRef<SegmentTextMetrics | null>(null);
  // Native attachment frames and the raw line table can arrive in either order.
  const objectLayoutsRef = useRef(new Map<string, LayoutRect>());
  const rawMetricsRef = useRef<SegmentTextMetrics | null>(null);
  const contentOffsetRef = useRef<ContentOffset | null>(null);

  // Latest unitIds, read by the mount effect so its initial registration is not
  // frozen to the first render's value.
  const unitIdsRef = useRef(unitIds);
  unitIdsRef.current = unitIds;

  // Recompute when either the text engine or an attachment finishes layout.
  const publishMetrics = useCallback(() => {
    const raw = rawMetricsRef.current;
    if (raw === null) return;
    const metrics = withInlineObjectMetrics(
      raw,
      unitIdsRef.current,
      ctx.registry.index,
      objectLayoutsRef.current
    );
    metricsRef.current = metrics ?? null;
    ctx.setMetrics(nodeId, metrics);
  }, [ctx, nodeId]);
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ctx, nodeId, unitIdsKey]);

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

  const onTextLayout = (e: NativeSyntheticEvent<TextLayoutEventData>) => {
    const metrics = buildTextMetrics(e.nativeEvent.lines);
    const expected = sumUnitTextLength(ctx.registry.index, unitIdsRef.current);
    if (expected !== undefined && metrics.textLength !== expected) {
      // The platform laid out different text than the model holds — a marker, a
      // collapsed whitespace run, a fallback label. Every offset past the
      // divergence would resolve wrongly, so drop the table: the block degrades
      // to whole-rect hit testing and copying keeps using the units' text,
      // never a wrong index. Dev names the node so the divergence is locatable.
      rawMetricsRef.current = null;
      metricsRef.current = null;
      ctx.setMetrics(nodeId, undefined);
      if (typeof __DEV__ !== 'undefined' && __DEV__) {
        console.warn(
          `[supramark] SelectableBlock "${nodeId}": the text engine laid out ` +
            `${metrics.textLength} UTF-16 units but the selection units hold ${expected}. ` +
            'Hit testing falls back to the whole block until the rendered text matches the units.'
        );
      }
      return;
    }
    rawMetricsRef.current = metrics;
    publishMetrics();
  };

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
          style={style}
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
