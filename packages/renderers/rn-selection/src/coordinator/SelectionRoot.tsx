import React, {
  useCallback,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import {
  PanResponder,
  View,
  type GestureResponderEvent,
  type LayoutChangeEvent,
} from 'react-native';
import { lineAtY, offsetInsideLineX } from '../metrics';
import type { SelectionNodeId, SelectionRange, SelectionUnit } from '../model';
import { orderSelectionRange } from '../resolve';
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
import {
  blocksBounds,
  containingBlock,
  nearestBlockWithin,
  paragraphRangeAt,
  resolvePointToSelection,
  resolvePointToSelectionInViewport,
  resolvePointToSelectionOutsideViewports,
  type Point,
} from './hitTest';
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
  /**
   * Whether selection may be entered at all (default: true). `false` clears the
   * current selection and refuses every entry point — gestures, handles,
   * toolbar actions, and a host driving the store directly. Distinct from
   * `gestures`, which only turns off the built-in touch handling and cannot
   * stand in for a full disable.
   */
  enabled?: boolean;
  /**
   * The built-in long-press word selection (default: true). `false` keeps
   * tap-to-clear, handle drags and the toolbar, and leaves starting a selection
   * to the host — a long-press menu whose "select text" item calls
   * `handleRef.trySelectParagraphAt`. The long press then never competes with
   * the host's own long-press gesture.
   */
  longPress?: boolean;
  /**
   * Receives imperative commands for host-driven entries (see
   * `SelectionRootHandle`). Set through a regular prop rather than `ref` so the
   * same code works on React 18 and 19.
   */
  handleRef?: React.Ref<SelectionRootHandle>;
  longPressMs?: number;
  moveTolerance?: number;
  /** Notifies an enclosing host scroller while selection or a nested viewport owns the drag. */
  onGestureActiveChange?(active: boolean): void;
}

/**
 * Imperative commands for hosts that start or end a selection from their own
 * UI — a long-press menu's "select text" entry, a tap on the page background.
 * The built-in gestures stay available unless `longPress` / `gestures` say
 * otherwise; these are additional entry points.
 */
export interface SelectionRootHandle {
  /**
   * Select the block under a press, resolved within this root's own block
   * extent. Pass RN page coordinates (`nativeEvent.pageX/pageY`, or the last
   * two values of `View.measure`) relative to the same React root. Do not pass
   * `measureInWindow` coordinates or raw RNGH `absoluteX/absoluteY`.
   * Resolves with the REAL outcome: false when
   * selection is disabled, the root is out of reach, or nothing is selectable
   * under the press.
   */
  trySelectParagraphAt(pageX: number, pageY: number): Promise<boolean>;
  /** Clear the current selection (an outside entry, e.g. a background tap). */
  clearSelection(): void;
  /**
   * This root's live RN page-space box, measured on demand (never cached), for a
   * host that must pick ONE root among several before asking it to select —
   * the nearest to the press wins. Null when it cannot be measured.
   */
  measurePageRect(): Promise<{ x: number; y: number; w: number; h: number } | null>;
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
  enabled = true,
  longPress,
  handleRef,
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

  // Read by callbacks that must keep a stable identity (they are part of `ctx`,
  // whose churn would re-register every block) while still observing the
  // current gate.
  const interactionEnabled = enabled !== false;
  const interactionEnabledRef = useRef(interactionEnabled);
  interactionEnabledRef.current = interactionEnabled;

  // Every host request belongs to one committed document/enablement lifetime.
  const hostRequestGenerationRef = useRef(0);
  // Retained handles and callbacks must refuse work after the root unmounts.
  const mountedRef = useRef(false);
  useLayoutEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      hostRequestGenerationRef.current += 1;
    };
  }, []);
  // Layout effects invalidate requests before passive document clearing can run.
  useLayoutEffect(() => {
    hostRequestGenerationRef.current += 1;
  }, [units, interactionEnabled]);

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
  // `undefined` means that no block owned the gesture start, `null` is the
  // explicit scope for root content outside nested viewports, and a string is
  // one particular nested viewport. Keeping `null` distinct prevents an
  // outside handle from crossing into a FlatList (the reverse of the already
  // constrained inside-to-outside direction).
  const gestureViewportIdRef = useRef<string | null | undefined>(undefined);

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
      // While selection is off, nothing may take the host scroll lock: there is
      // nothing to protect, and a lock taken here would freeze the host list
      // with no gesture able to release it (every entry point is refused).
      if (active && !interactionEnabledRef.current) return;
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
      if (!interactionEnabledRef.current) return;
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
      // An unmeasured attachment region cannot safely fall back to the whole paragraph.
      if (
        !metrics &&
        block.unitIds.some(
          id => registry.index.entries[registry.index.byUnitId.get(id) ?? -1]?.unit.kind === 'atom'
        )
      )
        return null;
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
    (point: Point) => {
      const blocks = registry.getBlocks();
      const viewportId = gestureViewportIdRef.current;
      if (viewportId === undefined) {
        return resolvePointToSelection(blocks, point, registry.index);
      }
      return viewportId === null
        ? resolvePointToSelectionOutsideViewports(blocks, point, registry.index)
        : resolvePointToSelectionInViewport(blocks, point, registry.index, viewportId);
    },
    [registry]
  );

  const fixedPointForHandle = useCallback(
    (edge: HandleEdge, range: SelectionRange) => {
      const ordered = orderSelectionRange(registry.index, range);
      return edge === 'start' ? ordered.end : ordered.start;
    },
    [registry]
  );

  const scopeGestureToHandle = useCallback(
    (edge: HandleEdge) => {
      const range = store.getSnapshot().range;
      if (range === null) {
        gestureViewportIdRef.current = undefined;
        return;
      }
      const ordered = orderSelectionRange(registry.index, range);
      const moving = edge === 'start' ? ordered.start : ordered.end;
      const block =
        moving.unitId === undefined
          ? registry.getBlock(moving.nodeId)
          : registry.getBlockForUnit(moving.unitId);
      gestureViewportIdRef.current = block === undefined ? undefined : (block.viewportId ?? null);
    },
    [registry, store]
  );

  // `gestures` only turns off the built-in touch handling; `enabled` turns
  // selection off entirely — current selection cleared, every entry point
  // (gestures, handles, toolbar, a host driving the store) refused, host scroll
  // lock released. See the disabling effect further down.
  const gestureEnabled = interactionEnabled && gestures !== false;
  const longPressEnabled = longPress !== false;
  // The Android Text entry shares the live gesture gates without changing context identity.
  const nativeLongPressEnabledRef = useRef(gestureEnabled && longPressEnabled);
  nativeLongPressEnabledRef.current = gestureEnabled && longPressEnabled;
  const longPressDelay = longPressMs ?? DEFAULT_LONG_PRESS_MS;

  const gesture = useMemo<SelectionGesture>(
    () =>
      createSelectionGesture({
        store,
        pointAt,
        wordAt,
        handleAt,
        // Preserve the grab offset against the visible edge's own text line.
        handlePoint: edge => {
          const handle = computeHandles(currentRects())?.[edge];
          return handle ? { x: handle.x, y: handle.y + handle.h / 2 } : null;
        },
        fixedPointForHandle,
        onPhaseChange: phase => publishGestureActivity(phase === 'extending' || phase === 'handle'),
        config: { longPressMs, moveTolerance, longPress: longPressEnabled },
      }),
    [
      store,
      pointAt,
      wordAt,
      handleAt,
      currentRects,
      fixedPointForHandle,
      publishGestureActivity,
      longPressMs,
      moveTolerance,
      longPressEnabled,
    ]
  );

  const cancelGesture = useCallback(() => {
    gesture.cancel();
    gestureViewportIdRef.current = undefined;
  }, [gesture]);

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
    root?.measure?.((_x, _y, _width, _height, pageX, pageY) => {
      if (Number.isFinite(pageX) && Number.isFinite(pageY)) {
        originRef.current = { x: pageX, y: pageY };
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

  const onTouchStart = useCallback(
    (e: GestureResponderEvent) => {
      if (!gestureEnabled) return;
      refreshRootOrigin();
      if (gesture.isActive() || gesture.isPending()) return;
      pendingTouchCancelledRef.current = false;
      const point = toRootSpace(e);
      const edge = handleAt(point);
      if (edge === null) {
        const block = containingBlock(registry.getBlocks(), point);
        gestureViewportIdRef.current = block === null ? undefined : (block.viewportId ?? null);
      } else {
        scopeGestureToHandle(edge);
      }
      gesture.touchStart(point, eventTime(e));
      clearTimer();
      if (longPressEnabled && gesture.isPending()) {
        timerRef.current = setTimeout(() => {
          timerRef.current = null;
          gesture.tick(Date.now());
          // A nested Android ScrollView can cancel the bubbling JS touch even
          // while a stationary long press remains inside it. If no onScroll
          // followed, keep the selection created by the timer but release
          // gesture ownership because the matching touch-end was swallowed.
          if (pendingTouchCancelledRef.current) {
            pendingTouchCancelledRef.current = false;
            cancelGesture();
          }
        }, longPressDelay);
      }
    },
    [
      gestureEnabled,
      refreshRootOrigin,
      gesture,
      toRootSpace,
      handleAt,
      registry,
      scopeGestureToHandle,
      clearTimer,
      cancelGesture,
      longPressDelay,
      longPressEnabled,
    ]
  );

  const onResponderGrant = onTouchStart;

  const onResponderMove = useCallback(
    (e: GestureResponderEvent) => {
      if (!gestureEnabled) return;
      refreshRootOrigin();
      const point = toRootSpace(e);
      gesture.touchMove(point, eventTime(e));
      if (!gesture.isPending()) clearTimer();
    },
    [gestureEnabled, refreshRootOrigin, gesture, toRootSpace, clearTimer]
  );

  const onResponderRelease = useCallback(
    (e: GestureResponderEvent) => {
      if (!gestureEnabled) return;
      refreshRootOrigin();
      clearTimer();
      pendingTouchCancelledRef.current = false;
      const point = toRootSpace(e);
      gesture.touchEnd(point, eventTime(e));
      gestureViewportIdRef.current = undefined;
    },
    [gestureEnabled, refreshRootOrigin, gesture, toRootSpace, clearTimer]
  );

  const onResponderTerminate = useCallback(() => {
    clearTimer();
    pendingTouchCancelledRef.current = false;
    cancelGesture();
  }, [clearTimer, cancelGesture]);

  const onTouchCancel = useCallback(() => {
    if (!gestureEnabled) return;
    if (gesture.isPending()) {
      // With the long press off, no timer arms to consume the cancel flag, so
      // leaving the phase pending would wedge it forever (and swallow the next
      // tap-clear). A cancelled touch is not a tap: reset outright.
      if (!longPressEnabled) {
        cancelGesture();
        return;
      }
      pendingTouchCancelledRef.current = true;
      return;
    }
    onResponderTerminate();
  }, [gestureEnabled, gesture, longPressEnabled, cancelGesture, onResponderTerminate]);

  const cancelPendingGesture = useCallback(() => {
    if (!gesture.isPending()) return;
    clearTimer();
    pendingTouchCancelledRef.current = false;
    cancelGesture();
  }, [clearTimer, cancelGesture, gesture]);

  const selectWordInBlock = useCallback(
    (nodeId: SelectionNodeId, localPoint: Point) => {
      // Native Text long presses must honor both host-menu and gesture opt-outs.
      if (!nativeLongPressEnabledRef.current) return;
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
      cancelGesture();
      store.beginAt(range.anchor);
      store.extendTo(range.focus);
      store.commit();
    },
    [cancelGesture, clearTimer, registry, store, wordAt]
  );

  // A new unit stream is a new content revision. The stored range points into a
  // document that no longer exists, and the first-version rule is to clear it
  // outright rather than migrate offsets (a re-parsed message, an edit, a
  // regenerated answer). Layout-only changes — width, font, async diagram
  // height — arrive as layout updates instead and keep the selection.
  const lastUnitsRef = useRef(units);
  useEffect(() => {
    if (lastUnitsRef.current === units) return;
    lastUnitsRef.current = units;
    clearTimer();
    pendingTouchCancelledRef.current = false;
    cancelGesture();
    store.clear();
  }, [units, clearTimer, cancelGesture, store]);

  // Disabling selection must cancel the live interaction, not merely stop new
  // ones: clear the selection, cancel a pending long press or drag, drop nested
  // viewport interactions, and release the host scroll lock. Any selection that
  // still appears while disabled (a host driving the store directly) is cleared
  // on the spot — the store is the single source of truth, so this guard lives
  // here instead of at each call site (toolbar, gestures, Android block press).
  useEffect(() => {
    if (interactionEnabled) return undefined;
    clearTimer();
    pendingTouchCancelledRef.current = false;
    cancelGesture();
    store.clear();
    viewportInteractionsRef.current.clear();
    publishHostScrollLock();
    return store.subscribe(() => {
      if (store.getSnapshot().range !== null) store.clear();
    });
  }, [interactionEnabled, clearTimer, cancelGesture, store, publishHostScrollLock]);

  // Host-driven entries (a long-press menu's "select text", a background tap)
  // share the same store and gesture teardown as the built-in ones, so all
  // entries stay governed by one state machine.
  useImperativeHandle(
    handleRef,
    () => ({
      trySelectParagraphAt(pageX: number, pageY: number): Promise<boolean> {
        // Host commands and RN touch events share page space. Measure the live
        // page origin so a host scroll does not leave this entry using a stale origin.
        if (!mountedRef.current || !interactionEnabledRef.current) return Promise.resolve(false);
        // New host commands supersede older native measurements, even if callbacks reorder.
        const requestGeneration = ++hostRequestGenerationRef.current;
        // Stable unit ids can be reused by a replacement document; retain the actual stream.
        const requestUnits = unitsRef.current;
        const selectAt = (localX: number, localY: number): boolean => {
          const point = { x: localX, y: localY };
          const blocks = registry.getBlocks();
          const bounds = blocksBounds(blocks);
          if (bounds === null) return false;
          // Ownership: within tight reach of the blocks' own extent, on BOTH
          // axes. The union rect may span diagrams/images between blocks, so a
          // large slack would let a press on a non-text object select text;
          // 12 units covers only the real gaps (paragraph spacing, line ends).
          const slack = 12;
          if (
            point.x < bounds.x - slack ||
            point.x > bounds.x + bounds.w + slack ||
            point.y < bounds.y - slack ||
            point.y > bounds.y + bounds.h + slack
          ) {
            return false;
          }
          const block = containingBlock(blocks, point) ?? nearestBlockWithin(blocks, point, slack);
          // Resolve the region at the actual press, including inline object boundaries.
          const range = block === null ? null : paragraphRangeAt(block, point, registry.index);
          if (range === null) return false;
          clearTimer();
          pendingTouchCancelledRef.current = false;
          cancelGesture();
          store.beginAt(range.anchor);
          store.extendTo(range.focus);
          store.commit();
          return true;
        };
        const root = rootRef.current as NativeViewHandle | null;
        // A detached root has no live page origin; never guess it from cached geometry.
        if (!root?.measure) return Promise.resolve(false);
        return new Promise<boolean>(resolve => {
          root.measure?.((_x, _y, _width, _height, rootX, rootY) => {
            // Only the current live request may publish into the current document's store.
            if (
              !mountedRef.current ||
              !interactionEnabledRef.current ||
              requestGeneration !== hostRequestGenerationRef.current ||
              requestUnits !== unitsRef.current ||
              rootRef.current !== root ||
              !Number.isFinite(rootX) ||
              !Number.isFinite(rootY)
            ) {
              resolve(false);
              return;
            }
            resolve(selectAt(pageX - rootX, pageY - rootY));
          });
        });
      },
      clearSelection() {
        // Clearing selection also cancels requests whose native measurement is still pending.
        hostRequestGenerationRef.current += 1;
        clearTimer();
        pendingTouchCancelledRef.current = false;
        cancelGesture();
        store.clear();
      },
      async measurePageRect(): Promise<{ x: number; y: number; w: number; h: number } | null> {
        const root = rootRef.current as NativeViewHandle | null;
        // Only mounted native roots can participate in the host's page-space arbitration.
        if (!root?.measure) return null;
        return new Promise(resolve => {
          root.measure?.((_x, _y, w, h, x, y) => {
            resolve(
              Number.isFinite(x) && Number.isFinite(y) && w > 0 && h > 0 ? { x, y, w, h } : null
            );
          });
        });
      },
    }),
    [registry, store, clearTimer, cancelGesture]
  );

  const onHandleGrant = useCallback(
    (edge: HandleEdge, point?: Point) => {
      if (!gestureEnabled) return;
      refreshRootOrigin();
      clearTimer();
      scopeGestureToHandle(edge);
      gesture.handleStart(edge, Date.now(), point);
    },
    [gestureEnabled, refreshRootOrigin, clearTimer, scopeGestureToHandle, gesture]
  );

  const onHandleMove = useCallback(
    (point: Point) => {
      if (!gestureEnabled) return;
      gesture.touchMove(point, Date.now());
    },
    [gestureEnabled, gesture]
  );

  const onHandleRelease = useCallback(
    (point: Point) => {
      if (!gestureEnabled) return;
      gesture.touchEnd(point, Date.now());
      gestureViewportIdRef.current = undefined;
    },
    [gestureEnabled, gesture]
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
      if (!gestureEnabled) return false;
      const point = toRootSpace(e);
      if (handleAt(point) !== null) return true;
      return gesture.isActive();
    },
    [gestureEnabled, gesture, handleAt, toRootSpace]
  );

  const onStartShouldSetResponder = useCallback(
    () => gestureEnabled && gesture.isActive(),
    [gestureEnabled, gesture]
  );

  const onResponderTerminationRequest = useCallback(
    () => !gestureEnabled || !gesture.isActive(),
    [gestureEnabled, gesture]
  );

  const rootPanResponder = useMemo(
    () =>
      PanResponder.create({
        onStartShouldSetPanResponder: onStartShouldSetResponder,
        onStartShouldSetPanResponderCapture: onStartShouldSetResponderCapture,
        onMoveShouldSetPanResponder: () => gestureEnabled && gesture.isActive(),
        onMoveShouldSetPanResponderCapture: () => gestureEnabled && gesture.isActive(),
        onPanResponderGrant: onResponderGrant,
        onPanResponderMove: onResponderMove,
        onPanResponderRelease: onResponderRelease,
        onPanResponderTerminate: onResponderTerminate,
        onPanResponderTerminationRequest: onResponderTerminationRequest,
        onShouldBlockNativeResponder: () => gesture.isActive(),
      }),
    [
      gestureEnabled,
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
      // Measure both nodes in RN page space. This includes nested scroll offsets
      // without mixing an Android window inset into either side of the subtraction.
      if (target?.measure && root?.measure) {
        root.measure((_x, _y, _width, _height, rootX, rootY) => {
          // Invalid native geometry cannot be used as a coordinate origin.
          if (!Number.isFinite(rootX) || !Number.isFinite(rootY)) {
            if (!measureAgainstRoot()) updateFallback();
            return;
          }
          target.measure?.((_targetX, _targetY, width, height, pageX, pageY) => {
            updateMeasured(pageX - rootX, pageY - rootY, width, height);
          });
        });
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
          {interactionEnabled && overlay !== false && <SelectionOverlay />}
          {interactionEnabled && handles !== false && (
            <SelectionHandles
              onHandleGrant={onHandleGrant}
              onHandleMove={onHandleMove}
              onHandleRelease={onHandleRelease}
              onHandleCancel={cancelGesture}
              toRootPoint={pointFromPage}
            />
          )}
          {interactionEnabled &&
            toolbar !== false &&
            (renderToolbar ? (
              <ToolbarSlot viewport={viewport} render={renderToolbar} />
            ) : (
              <SelectionToolbar viewport={viewport} anchor={rootRef} />
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
