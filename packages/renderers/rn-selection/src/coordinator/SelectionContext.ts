import { createContext } from 'react';
import type { SegmentTextMetrics } from '../metrics';
import type { SelectionNodeId } from '../model';
import type { ContentOffset, LayoutRect, RegisteredBlock, SelectionRegistry } from './registry';
import type { Point } from './hitTest';
import type { SelectionStore } from './state';
import type { SelectionToolbarItem } from './toolbar';

/**
 * Contract a block component uses to plug itself into the document selection.
 *
 * All real logic lives in `SelectionRegistry` / `SelectionStore`; this context
 * is only wiring. The registry and store are exposed directly so the overlay,
 * the toolbar and host controls under the root need no prop drilling.
 *
 * Blocks contribute three things: their identity and units (`registerBlock` /
 * `updateUnits`), their box (`updateLayout`), and their text geometry
 * (`setMetrics` / `setContentOffset`) — the last of which is what makes a
 * self-drawn, text-precision selection possible at all.
 */
export interface SelectionContextValue {
  // Property-style signatures throughout, matching `SelectionStore`: every
  // member is a closure with no `this`, so consumers can destructure it off the
  // context and pass it straight to a JSX prop.
  registry: SelectionRegistry;
  store: SelectionStore;
  /** Register a block; the returned disposer unregisters it. Call from useEffect. */
  registerBlock: (block: RegisteredBlock) => () => void;
  updateLayout: (nodeId: SelectionNodeId, rect: LayoutRect) => void;
  /**
   * Publish a block layout measured against SelectionRoot when the platform
   * exposes native measurement. Falls back to `updateLayout` in tests or hosts
   * without a native view handle.
   */
  measureLayout?: (nodeId: SelectionNodeId, node: unknown, fallback: LayoutRect) => void;
  /**
   * Re-measure blocks whose native handles have been seen. Hosts should call
   * this from ScrollView / FlatList `onScroll`, because scrolling changes a
   * block's root-space position without firing `onLayout`.
   */
  refreshLayouts?: () => void;
  /** Register a nested scroll viewport; the returned disposer unregisters it. */
  registerViewport: (viewportId: string, initialOffset?: ContentOffset) => () => void;
  /** Publish a viewport box measured against SelectionRoot. */
  measureViewportLayout: (viewportId: string, node: unknown, fallback: LayoutRect) => void;
  /** Publish the current native content offset for synchronous block movement. */
  updateViewportScroll: (viewportId: string, offset: ContentOffset) => void;
  /** Keep an enclosing host scroller paused while this nested viewport owns a touch/scroll. */
  setViewportInteractionActive: (viewportId: string, active: boolean) => void;
  /** Abandon a press that has become a native scroll before long-press activation. */
  cancelPendingGesture: () => void;
  /** Select the word at a Text-local point without relying on a swallowed root touch end. */
  selectWordInBlock: (nodeId: SelectionNodeId, point: Point) => void;
  /** Update a registered block's unit ids in place (streaming markdown growth). */
  updateUnits: (nodeId: SelectionNodeId, unitIds: readonly SelectionNodeId[]) => void;
  /** Publish the line table a block laid out (`onTextLayout`). */
  setMetrics: (nodeId: SelectionNodeId, metrics: SegmentTextMetrics) => void;
  /** Publish where a block's text box sits inside the block's own box. */
  setContentOffset: (nodeId: SelectionNodeId, offset: ContentOffset) => void;
  /** Items the selection toolbar offers; resolved once on the root. */
  toolbarItems: readonly SelectionToolbarItem[];
  /** Run a toolbar action against the current selection. */
  runToolbarItem: (item: SelectionToolbarItem) => void;
}

export const SelectionContext = createContext<SelectionContextValue | null>(null);

/** Nearest nested scroll viewport inherited by SelectableBlock registrations. */
export const SelectionViewportContext = createContext<string | undefined>(undefined);

/** True only while selection owns a drag; viewports pause native scrolling. */
export const SelectionGestureActivityContext = createContext(false);
