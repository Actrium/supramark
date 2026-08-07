import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  PanResponder,
  View,
  type GestureResponderEvent,
  type LayoutChangeEvent,
} from 'react-native';
import { lineAtY, offsetInsideLineX } from '../metrics';
import type { SelectionNodeId, SelectionRange, SelectionUnit } from '../model';
import {
  buildSegmentSpans,
  segmentSelectionToRange,
  segmentTextFromSpans,
} from '../native/segmentAdapter';
import { buildCopyRequest, type SelectionCopyRequest } from './actions';
import { wordBoundsAt } from '../words';
import {
  SelectionContext,
  SelectionGestureActivityContext,
  type SelectionContextValue,
} from './SelectionContext';
import { createSelectionGesture, DEFAULT_LONG_PRESS_MS, type SelectionGesture } from './gesture';
import { computeHandles, hitTestHandle, type HandleEdge } from './handles';
import { containingBlock, resolvePointToSelection, type Point } from './hitTest';
import { computeSelectionRects, type OverlayRect } from './overlay';
import { SelectionRegistry, type LayoutRect } from './registry';
import { SelectionHandles } from './SelectionHandles';
import { SelectionOverlay, useSelectionRects } from './SelectionOverlay';
import { SelectionToolbar } from './SelectionToolbar';
import { createSelectionStore } from './state';
import { DEFAULT_TOOLBAR_ITEMS, type Size, type SelectionToolbarItem } from './toolbar';
import { useSelectionContext, useSelectionSnapshot } from './useDocumentSelection';

type NativeViewHandle = {
  measure?: (
    callback: (
      x: number,
      y: number,
      width: number,
      height: number,
      pageX: number,
      pageY: number
    ) => void
  ) => void;
  measureInWindow?: (
    callback: (x: number, y: number, width: number, height: number) => void
  ) => void;
  measureLayout?: (
    relativeToNativeNode: unknown,
    onSuccess: (x: number, y: number, width: number, height: number) => void,
    onFail?: () => void
  ) => void;
};

interface LayoutTarget {
  node: unknown;
  fallback: LayoutRect;
}

function eventTime(e: GestureResponderEvent): number {
  const timestamp = (e.nativeEvent as GestureResponderEvent['nativeEvent'] & { timestamp?: number })
    .timestamp;
  return Number.isFinite(timestamp) ? Number(timestamp) : Date.now();
}

/** What a host `renderToolbar` receives; enough to place and drive its own bar. */
export interface SelectionToolbarRenderProps {
  visible: boolean;
  items: readonly SelectionToolbarItem[];
  /** Current highlight rectangles, in root space — anchor the bar to these. */
  rects: readonly OverlayRect[];
  /** Root-space visible area, for clamping. */
  viewport: Size;
  run(item: SelectionToolbarItem): void;
}

export interface SelectionRootProps {
  units: readonly SelectionUnit[];
  children?: React.ReactNode;
  onSelectionChange?(range: SelectionRange | null): void;
  onCopy?(request: SelectionCopyRequest): void;
  /** Toolbar actions. Defaults to Copy + Copy as Markdown. */
  toolbarItems?: readonly SelectionToolbarItem[];
  /** Replace the built-in bar entirely. */
  renderToolbar?(props: SelectionToolbarRenderProps): React.ReactNode;
  overlay?: boolean;
  handles?: boolean;
  toolbar?: boolean;
  /** Disable the built-in gestures; the host drives the store itself. */
  gestures?: boolean;
  longPressMs?: number;
  moveTolerance?: number;
  /** Notifies an enclosing host scroller while selection or a nested viewport owns the drag. */
  onGestureActiveChange?(active: boolean): void;
}

/**
 * Map a root-coordinate point to a document `SelectionPoint` through a registry.
 * Thin so gesture code stays declarative; all logic lives in `hitTest`.
 */
export function pointToSelectionForRoot(registry: SelectionRegistry, point: Point) {
  return resolvePointToSelection(registry.getBlocks(), point, registry.index);
}

/**
 * Owner of the selection: registry, store, gestures, and the three self-drawn
 * layers (highlight, handles, action bar).
 *
 * Everything that used to be split between us and the platform now lives under
 * this component. A long press starts a selection here, a drag extends it here,
 * a handle moves it here, a tap clears it here, and the bar that appears is a
 * React component. The store is the single source of truth throughout — there
 * is no second selection state anywhere in the tree to reconcile with.
 *
 * Touch points are converted to root space through the root's native page
 * origin, so they stay correct wherever the root sits on screen and match the
 * coordinate system RN reports on `pageX/pageY`. Gesture *wiring* (touch
 * dispatch, responder negotiation with an enclosing ScrollView) is the one part
 * of this layer that only a device can really exercise; the decision-making it
 * feeds is the pure machine in `gesture.ts`.
 */
export const SelectionRoot: React.FC<SelectionRootProps> = ({
  units,
  children,
  onSelectionChange,
  onCopy,
  toolbarItems,
  renderToolbar,
  overlay,
  handles,
  toolbar,
  gestures,
  longPressMs,
  moveTolerance,
  onGestureActiveChange,
}) => {
  // Latest units are read lazily by the store so streaming updates are visible.
  const unitsRef = useRef(units);
  unitsRef.current = units;

  const registry = useMemo(() => new SelectionRegistry(unitsRef.current), []);
  const store = useMemo(() => createSelectionStore(() => unitsRef.current), []);

  // Host callbacks are read through refs so `ctx` stays reference-stable even
  // when the host passes fresh inline identities on every render. A churning
  // `ctx` would re-run every block's registration effect (unregister +
  // re-register), which previously wiped measured rects and blanked the
  // overlay/hit-test.
  const onCopyRef = useRef(onCopy);
  onCopyRef.current = onCopy;

  const onGestureActiveChangeRef = useRef(onGestureActiveChange);
  onGestureActiveChangeRef.current = onGestureActiveChange;

  const items = useMemo(() => toolbarItems ?? DEFAULT_TOOLBAR_ITEMS, [toolbarItems]);
  const itemsRef = useRef(items);
  itemsRef.current = items;

  const rootRef = useRef<View | null>(null);
  const originRef = useRef<Point>({ x: 0, y: 0 });
  const layoutTargetsRef = useRef(new Map<SelectionNodeId, LayoutTarget>());
  const [viewport, setViewport] = useState<Size>({ w: 0, h: 0 });
  const [gestureActive, setGestureActive] = useState(false);
  const gestureActiveRef = useRef(false);
  const viewportInteractionsRef = useRef(new Set<string>());
  const hostScrollLockedRef = useRef(false);

  const publishHostScrollLock = useCallback(() => {
    const locked = gestureActiveRef.current || viewportInteractionsRef.current.size > 0;
    if (hostScrollLockedRef.current === locked) return;
    hostScrollLockedRef.current = locked;
    onGestureActiveChangeRef.current?.(locked);
  }, []);

  const publishGestureActivity = useCallback(
    (active: boolean) => {
      if (gestureActiveRef.current === active) return;
      gestureActiveRef.current = active;
      setGestureActive(active);
      publishHostScrollLock();
    },
    [publishHostScrollLock]
  );

  const setViewportInteractionActive = useCallback(
    (viewportId: string, active: boolean) => {
      if (active) viewportInteractionsRef.current.add(viewportId);
      else viewportInteractionsRef.current.delete(viewportId);
      publishHostScrollLock();
    },
    [publishHostScrollLock]
  );

  useEffect(
    () => () => {
      if (hostScrollLockedRef.current) onGestureActiveChangeRef.current?.(false);
    },
    []
  );

  // Re-index when the unit stream changes (streaming markdown).
  useEffect(() => {
    registry.setUnits(units);
  }, [registry, units]);

  // Surface range changes; covered units live on the store snapshot.
  useEffect(() => {
    if (!onSelectionChange) return undefined;
    return store.subscribe(() => onSelectionChange(store.getSnapshot().range));
  }, [store, onSelectionChange]);

  const runToolbarItem = useCallback(
    (item: SelectionToolbarItem) => {
      const snapshot = store.getSnapshot();
      if (snapshot.range === null) return;
      onCopyRef.current?.(buildCopyRequest(item, snapshot.units, snapshot.range));
      store.clear();
    },
    [store]
  );

  /** Highlight rectangles as the gesture layer needs them, computed on demand. */
  const currentRects = useCallback((): OverlayRect[] => {
    const snapshot = store.getSnapshot();
    return computeSelectionRects({
      blocks: registry.getBlocks(),
      range: snapshot.range,
      units: snapshot.units,
      index: registry.index,
    });
  }, [registry, store]);

  /**
   * The word under a root-space point, as a document range. A block that has
   * not been measured yet has no notion of words, so a long press on it selects
   * the whole block — coarse, but never nothing.
   */
  const wordAt = useCallback(
    (point: Point): SelectionRange | null => {
      const block = containingBlock(registry.getBlocks(), point);
      if (block === null || block.rect === undefined) return null;
      const spans = buildSegmentSpans(block, registry.index);
      if (spans.length === 0) return null;
      const wholeBlock = () => segmentSelectionToRange(spans, 0, spans[spans.length - 1].end);

      const metrics = block.metrics;
      if (!metrics || metrics.lines.length === 0) return wholeBlock();

      const origin = block.contentOffset ?? { x: 0, y: 0 };
      const localX = point.x - block.rect.x - origin.x;
      const localY = point.y - block.rect.y - origin.y;
      const line = lineAtY(metrics, localY);
      if (
        line === null ||
        localY < line.y ||
        localY > line.y + line.h ||
        localX < line.x ||
        localX > line.x + line.w
      ) {
        return null;
      }
      const offset = offsetInsideLineX(line, localX);
      const bounds = wordBoundsAt(segmentTextFromSpans(registry.index, spans), offset);
      if (bounds.end <= bounds.start) return wholeBlock();
      return segmentSelectionToRange(spans, bounds.start, bounds.end);
    },
    [registry]
  );

  const handleAt = useCallback(
    (point: Point): HandleEdge | null => {
      if (store.getSnapshot().range === null) return null;
      const handles = computeHandles(currentRects());
      return hitTestHandle(point, handles);
    },
    [currentRects, store]
  );

  const pointAt = useCallback(
    (point: Point) => resolvePointToSelection(registry.getBlocks(), point, registry.index),
    [registry]
  );

  const gesture = useMemo<SelectionGesture>(
    () =>
      createSelectionGesture({
        store,
        pointAt,
        wordAt,
        handleAt,
        onPhaseChange: phase => publishGestureActivity(phase === 'extending' || phase === 'handle'),
        config: { longPressMs, moveTolerance },
      }),
    [store, pointAt, wordAt, handleAt, publishGestureActivity, longPressMs, moveTolerance]
  );

  // The long-press threshold has to fire while the finger is still, i.e. with
  // no further touch events to hang it off. The gesture machine stays free of
  // timers; this is the one place a real clock enters.
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingTouchCancelledRef = useRef(false);
  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);
  useEffect(() => clearTimer, [clearTimer]);

  const refreshRootOrigin = useCallback(() => {
    const root = rootRef.current as NativeViewHandle | null;
    if (root?.measure) {
      root.measure((_x, _y, _width, _height, pageX, pageY) => {
        if (Number.isFinite(pageX) && Number.isFinite(pageY)) {
          originRef.current = { x: pageX, y: pageY };
        }
      });
      return;
    }
    root?.measureInWindow?.((x, y) => {
      if (Number.isFinite(x) && Number.isFinite(y)) {
        originRef.current = { x, y };
      }
    });
  }, []);

  const pointFromPage = useCallback((page: Point, origin: Point = originRef.current): Point => {
    // RN's locationX/Y are target-relative and may be in a child Text's space.
    // Everything downstream is in SelectionRoot space, so use page coordinates.
    return { x: page.x - origin.x, y: page.y - origin.y };
  }, []);

  const toRootSpace = useCallback(
    (e: GestureResponderEvent): Point => {
      const { pageX, pageY } = e.nativeEvent;
      return pointFromPage({ x: pageX, y: pageY });
    },
    [pointFromPage]
  );

  const enabled = gestures !== false;
  const longPressDelay = longPressMs ?? DEFAULT_LONG_PRESS_MS;

  const onTouchStart = useCallback(
    (e: GestureResponderEvent) => {
      if (!enabled) return;
      refreshRootOrigin();
      if (gesture.isActive() || gesture.isPending()) return;
      pendingTouchCancelledRef.current = false;
      const point = toRootSpace(e);
      gesture.touchStart(point, eventTime(e));
      clearTimer();
      if (gesture.isPending()) {
        timerRef.current = setTimeout(() => {
          timerRef.current = null;
          gesture.tick(Date.now());
          // A nested Android ScrollView can cancel the bubbling JS touch even
          // while a stationary long press remains inside it. If no onScroll
          // followed, keep the selection created by the timer but release
          // gesture ownership because the matching touch-end was swallowed.
          if (pendingTouchCancelledRef.current) {
            pendingTouchCancelledRef.current = false;
            gesture.cancel();
          }
        }, longPressDelay);
      }
    },
    [enabled, refreshRootOrigin, gesture, toRootSpace, clearTimer, longPressDelay]
  );

  const onResponderGrant = onTouchStart;

  const onResponderMove = useCallback(
    (e: GestureResponderEvent) => {
      if (!enabled) return;
      refreshRootOrigin();
      const point = toRootSpace(e);
      gesture.touchMove(point, eventTime(e));
      if (!gesture.isPending()) clearTimer();
    },
    [enabled, refreshRootOrigin, gesture, toRootSpace, clearTimer]
  );

  const onResponderRelease = useCallback(
    (e: GestureResponderEvent) => {
      if (!enabled) return;
      refreshRootOrigin();
      clearTimer();
      pendingTouchCancelledRef.current = false;
      const point = toRootSpace(e);
      gesture.touchEnd(point, eventTime(e));
    },
    [enabled, refreshRootOrigin, gesture, toRootSpace, clearTimer]
  );

  const onResponderTerminate = useCallback(() => {
    clearTimer();
    pendingTouchCancelledRef.current = false;
    gesture.cancel();
  }, [clearTimer, gesture]);

  const onTouchCancel = useCallback(() => {
    if (gesture.isPending()) {
      pendingTouchCancelledRef.current = true;
      return;
    }
    onResponderTerminate();
  }, [gesture, onResponderTerminate]);

  const cancelPendingGesture = useCallback(() => {
    if (!gesture.isPending()) return;
    clearTimer();
    pendingTouchCancelledRef.current = false;
    gesture.cancel();
  }, [clearTimer, gesture]);

  const selectWordInBlock = useCallback(
    (nodeId: SelectionNodeId, localPoint: Point) => {
      const block = registry.getBlock(nodeId);
      if (block?.rect === undefined) return;
      const contentOffset = block.contentOffset ?? { x: 0, y: 0 };
      const point = {
        x: block.rect.x + contentOffset.x + localPoint.x,
        y: block.rect.y + contentOffset.y + localPoint.y,
      };
      const range = wordAt(point);
      if (range === null) return;

      // Android can let Text's native long-press responder fire while its
      // nested ScrollView swallows the root touch end/cancel. Finish any root
      // gesture first, then publish the same committed range directly so the
      // nested scroller never remains locked after the finger lifts.
      clearTimer();
      pendingTouchCancelledRef.current = false;
      gesture.cancel();
      store.beginAt(range.anchor);
      store.extendTo(range.focus);
      store.commit();
    },
    [clearTimer, gesture, registry, store, wordAt]
  );

  const onHandleGrant = useCallback(
    (edge: HandleEdge) => {
      if (!enabled) return;
      refreshRootOrigin();
      clearTimer();
      gesture.handleStart(edge, Date.now());
    },
    [enabled, refreshRootOrigin, clearTimer, gesture]
  );

  const onHandleMove = useCallback(
    (point: Point) => {
      if (!enabled) return;
      gesture.touchMove(point, Date.now());
    },
    [enabled, gesture]
  );

  const onHandleRelease = useCallback(
    (point: Point) => {
      if (!enabled) return;
      gesture.touchEnd(point, Date.now());
    },
    [enabled, gesture]
  );

  const onLayout = useCallback(
    (e: LayoutChangeEvent) => {
      const { width, height } = e.nativeEvent.layout;
      setViewport(prev => (prev.w === width && prev.h === height ? prev : { w: width, h: height }));
      refreshRootOrigin();
    },
    [refreshRootOrigin]
  );

  const onStartShouldSetResponderCapture = useCallback(
    (e: GestureResponderEvent) => {
      if (!enabled) return false;
      const point = toRootSpace(e);
      if (handleAt(point) !== null) return false;
      return gesture.isActive();
    },
    [enabled, gesture, handleAt, toRootSpace]
  );

  const onStartShouldSetResponder = useCallback(
    () => enabled && gesture.isActive(),
    [enabled, gesture]
  );

  const onResponderTerminationRequest = useCallback(
    () => !enabled || !gesture.isActive(),
    [enabled, gesture]
  );

  const rootPanResponder = useMemo(
    () =>
      PanResponder.create({
        onStartShouldSetPanResponder: onStartShouldSetResponder,
        onStartShouldSetPanResponderCapture: onStartShouldSetResponderCapture,
        onMoveShouldSetPanResponder: () => enabled && gesture.isActive(),
        onMoveShouldSetPanResponderCapture: () => enabled && gesture.isActive(),
        onPanResponderGrant: onResponderGrant,
        onPanResponderMove: onResponderMove,
        onPanResponderRelease: onResponderRelease,
        onPanResponderTerminate: onResponderTerminate,
        onPanResponderTerminationRequest: onResponderTerminationRequest,
        onShouldBlockNativeResponder: () => gesture.isActive(),
      }),
    [
      enabled,
      gesture,
      onResponderGrant,
      onResponderMove,
      onResponderRelease,
      onResponderTerminate,
      onResponderTerminationRequest,
      onStartShouldSetResponder,
      onStartShouldSetResponderCapture,
    ]
  );

  const measureTargetLayout = useCallback(
    (node: unknown, fallback: LayoutRect, update: (rect: LayoutRect) => void) => {
      const target = node as NativeViewHandle | null;
      const root = rootRef.current as NativeViewHandle | null;
      const updateFallback = () => update(fallback);
      const updateMeasured = (x: number, y: number, width: number, height: number) => {
        if (
          Number.isFinite(x) &&
          Number.isFinite(y) &&
          Number.isFinite(width) &&
          Number.isFinite(height)
        ) {
          update({ x, y, w: width, h: height });
        } else {
          updateFallback();
        }
      };
      const measureAgainstRoot = () => {
        if (!target?.measureLayout || root === null) return false;
        target.measureLayout(
          root,
          (x, y, width, height) => {
            updateMeasured(x, y, width, height);
          },
          updateFallback
        );
        return true;
      };
      const measureTargetInRootWindow = (origin: Point) => {
        if (target?.measureInWindow) {
          target.measureInWindow((x, y, width, height) => {
            updateMeasured(x - origin.x, y - origin.y, width, height);
          });
          return true;
        }
        if (target?.measure) {
          target.measure((_x, _y, width, height, pageX, pageY) => {
            updateMeasured(pageX - origin.x, pageY - origin.y, width, height);
          });
          return true;
        }
        return false;
      };
      const afterRootMeasured = (x: number, y: number) => {
        if (Number.isFinite(x) && Number.isFinite(y)) {
          const origin = { x, y };
          originRef.current = origin;
          if (measureTargetInRootWindow(origin)) return;
        }
        if (!measureAgainstRoot()) updateFallback();
      };
      const canMeasureTargetInWindow = Boolean(target?.measureInWindow || target?.measure);
      if (canMeasureTargetInWindow && root?.measureInWindow) {
        root.measureInWindow((x, y) => afterRootMeasured(x, y));
        return;
      }
      if (canMeasureTargetInWindow && root?.measure) {
        root.measure((_x, _y, _width, _height, pageX, pageY) => afterRootMeasured(pageX, pageY));
        return;
      }
      if (!measureAgainstRoot()) updateFallback();
    },
    []
  );

  const measureBlockLayout = useCallback(
    (nodeId: SelectionNodeId, node: unknown, fallback: LayoutRect) => {
      measureTargetLayout(node, fallback, rect => registry.updateLayout(nodeId, rect));
    },
    [measureTargetLayout, registry]
  );

  const measureLayout = useCallback(
    (nodeId: SelectionNodeId, node: unknown, fallback: LayoutRect) => {
      layoutTargetsRef.current.set(nodeId, { node, fallback });
      measureBlockLayout(nodeId, node, fallback);
    },
    [measureBlockLayout]
  );

  const refreshLayouts = useCallback(() => {
    refreshRootOrigin();
    for (const [nodeId, target] of layoutTargetsRef.current) {
      measureBlockLayout(nodeId, target.node, target.fallback);
    }
  }, [measureBlockLayout, refreshRootOrigin]);

  const measureViewportLayout = useCallback(
    (viewportId: string, node: unknown, fallback: LayoutRect) => {
      measureTargetLayout(node, fallback, rect => registry.updateViewportLayout(viewportId, rect));
    },
    [measureTargetLayout, registry]
  );

  // Reference-stable: depends only on the memoized registry + store.
  const ctx = useMemo<SelectionContextValue>(
    () => ({
      registry,
      store,
      registerBlock: block => {
        // Capture what was actually stored and hand it back on unregister, so
        // a stale instance's cleanup cannot delete a live registration that
        // reused the same nodeId. See `SelectionRegistry.unregister`.
        const registered = registry.register(block);
        return () => {
          if (registry.getBlock(block.nodeId) === registered) {
            layoutTargetsRef.current.delete(block.nodeId);
          }
          registry.unregister(block.nodeId, registered);
        };
      },
      updateLayout: (nodeId, rect) => registry.updateLayout(nodeId, rect),
      measureLayout,
      refreshLayouts,
      registerViewport: (viewportId, initialOffset) =>
        registry.registerViewport(viewportId, initialOffset),
      measureViewportLayout,
      updateViewportScroll: (viewportId, offset) =>
        registry.updateViewportScroll(viewportId, offset),
      setViewportInteractionActive,
      cancelPendingGesture,
      selectWordInBlock,
      updateUnits: (nodeId, unitIds) => registry.updateUnits(nodeId, unitIds),
      setMetrics: (nodeId, metrics) => registry.setMetrics(nodeId, metrics),
      setContentOffset: (nodeId, offset) => registry.setContentOffset(nodeId, offset),
      get toolbarItems() {
        return itemsRef.current;
      },
      runToolbarItem,
    }),
    [
      registry,
      store,
      runToolbarItem,
      measureLayout,
      refreshLayouts,
      measureViewportLayout,
      setViewportInteractionActive,
      cancelPendingGesture,
      selectWordInBlock,
    ]
  );

  return (
    <View
      ref={rootRef}
      collapsable={false}
      onLayout={onLayout}
      onTouchStart={onTouchStart}
      onTouchMove={onResponderMove}
      onTouchEnd={onResponderRelease}
      onTouchCancel={onTouchCancel}
      // Observe a pending press through bubbling touch events. PanResponder
      // claims only after the long press becomes a selection, leaving an inner
      // ScrollView / FlatList free to own ordinary swipes from the first frame.
      {...rootPanResponder.panHandlers}
    >
      <SelectionGestureActivityContext.Provider value={gestureActive}>
        <SelectionContext.Provider value={ctx}>
          {children}
          {overlay !== false && <SelectionOverlay />}
          {handles !== false && (
            <SelectionHandles
              onHandleGrant={onHandleGrant}
              onHandleMove={onHandleMove}
              onHandleRelease={onHandleRelease}
              onHandleCancel={() => gesture.cancel()}
              toRootPoint={pointFromPage}
            />
          )}
          {toolbar !== false &&
            (renderToolbar ? (
              <ToolbarSlot viewport={viewport} render={renderToolbar} />
            ) : (
              <SelectionToolbar viewport={viewport} />
            ))}
        </SelectionContext.Provider>
      </SelectionGestureActivityContext.Provider>
    </View>
  );
};

/**
 * Adapts a host `renderToolbar` to the same subscriptions the built-in bar
 * uses, so a custom bar repaints on exactly the same events. Split out because
 * hooks cannot be called conditionally in `SelectionRoot`.
 */
const ToolbarSlot: React.FC<{
  viewport: Size;
  render(props: SelectionToolbarRenderProps): React.ReactNode;
}> = ({ viewport, render }) => {
  const { store, toolbarItems, runToolbarItem } = useSelectionContext();
  const snapshot = useSelectionSnapshot(store);
  const rects = useSelectionRects();
  return (
    <>
      {render({
        visible: snapshot.phase === 'selected' && rects.length > 0,
        items: toolbarItems,
        rects,
        viewport,
        run: runToolbarItem,
      })}
    </>
  );
};
