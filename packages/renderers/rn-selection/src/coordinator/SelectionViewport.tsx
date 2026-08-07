import React, { useCallback, useContext, useEffect, useId, useRef } from 'react';
import {
  View,
  type LayoutChangeEvent,
  type NativeScrollEvent,
  type NativeSyntheticEvent,
  type StyleProp,
  type ViewStyle,
} from 'react-native';
import { SelectionGestureActivityContext, SelectionViewportContext } from './SelectionContext';
import type { ContentOffset, LayoutRect } from './registry';
import { useSelectionContext } from './useDocumentSelection';

interface ScrollableChildProps {
  onScroll?: (event: NativeSyntheticEvent<NativeScrollEvent>) => void;
  onScrollBeginDrag?: (event: NativeSyntheticEvent<NativeScrollEvent>) => void;
  onScrollEndDrag?: (event: NativeSyntheticEvent<NativeScrollEvent>) => void;
  onMomentumScrollBegin?: (event: NativeSyntheticEvent<NativeScrollEvent>) => void;
  onMomentumScrollEnd?: (event: NativeSyntheticEvent<NativeScrollEvent>) => void;
  scrollEnabled?: boolean;
  scrollEventThrottle?: number;
}

export interface SelectionViewportProps {
  /** A single ScrollView or FlatList whose content contains SelectableBlocks. */
  children: React.ReactElement<ScrollableChildProps>;
  /** Own the scrollable's visible box here (height, border, radius, and so on). */
  style?: StyleProp<ViewStyle>;
}

/**
 * Gives a nested ScrollView / FlatList an explicit selection viewport.
 *
 * React Native reports a scroll offset synchronously but native `measure`
 * callbacks arrive later. This wrapper uses the offset delta to move mounted
 * block geometry in the same frame, clips highlights to the visible box, and
 * pauses this scroller while a selection or handle drag owns the gesture.
 */
export const SelectionViewport: React.FC<SelectionViewportProps> = ({ children, style }) => {
  const ctx = useSelectionContext();
  const gestureActive = useContext(SelectionGestureActivityContext);
  const reactId = useId();
  const viewportIdRef = useRef(`selection-viewport-${reactId}`);
  const viewportId = viewportIdRef.current;
  const viewportRef = useRef<View | null>(null);
  const layoutRef = useRef<LayoutRect | null>(null);
  const offsetRef = useRef<ContentOffset>({ x: 0, y: 0 });
  const touchingRef = useRef(false);
  const draggingRef = useRef(false);
  const momentumRef = useRef(false);
  const releaseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearReleaseTimer = useCallback(() => {
    if (releaseTimerRef.current === null) return;
    clearTimeout(releaseTimerRef.current);
    releaseTimerRef.current = null;
  }, []);

  const activateInteraction = useCallback(() => {
    clearReleaseTimer();
    ctx.setViewportInteractionActive(viewportId, true);
  }, [clearReleaseTimer, ctx, viewportId]);

  const releaseIfIdle = useCallback(() => {
    if (touchingRef.current || draggingRef.current || momentumRef.current) return;
    ctx.setViewportInteractionActive(viewportId, false);
  }, [ctx, viewportId]);

  const scheduleReleaseIfIdle = useCallback(() => {
    clearReleaseTimer();
    releaseTimerRef.current = setTimeout(() => {
      releaseTimerRef.current = null;
      releaseIfIdle();
    }, 0);
  }, [clearReleaseTimer, releaseIfIdle]);

  useEffect(() => {
    const dispose = ctx.registerViewport(viewportId, offsetRef.current);
    const layout = layoutRef.current;
    if (layout !== null) {
      ctx.measureViewportLayout(viewportId, viewportRef.current, layout);
    }
    ctx.updateViewportScroll(viewportId, offsetRef.current);
    return () => {
      clearReleaseTimer();
      ctx.setViewportInteractionActive(viewportId, false);
      dispose();
    };
  }, [clearReleaseTimer, ctx, viewportId]);

  const onLayout = useCallback(
    (event: LayoutChangeEvent) => {
      const { x, y, width, height } = event.nativeEvent.layout;
      const fallback = { x, y, w: width, h: height };
      layoutRef.current = fallback;
      ctx.measureViewportLayout(viewportId, viewportRef.current, fallback);
    },
    [ctx, viewportId]
  );

  const childOnScroll = children.props.onScroll;
  const onScroll = useCallback(
    (event: NativeSyntheticEvent<NativeScrollEvent>) => {
      // Android may cancel the bubbling JS touch as soon as its native
      // ScrollView takes ownership. A real content offset is the unambiguous
      // signal that the pending hold became a scroll, so do not let its timer
      // create a selection after the finger has moved away.
      const { x, y } = event.nativeEvent.contentOffset;
      const offset = { x, y };
      if (x !== offsetRef.current.x || y !== offsetRef.current.y) {
        ctx.cancelPendingGesture();
      }
      offsetRef.current = offset;
      ctx.updateViewportScroll(viewportId, offset);
      childOnScroll?.(event);
    },
    [childOnScroll, ctx, viewportId]
  );

  const onTouchStart = useCallback(() => {
    touchingRef.current = true;
    activateInteraction();
  }, [activateInteraction]);

  const onTouchFinish = useCallback(() => {
    touchingRef.current = false;
    // Android can emit the JS cancel immediately before onScrollBeginDrag.
    // Defer the release one turn so the native scroller can establish ownership.
    scheduleReleaseIfIdle();
  }, [scheduleReleaseIfIdle]);

  const childOnScrollBeginDrag = children.props.onScrollBeginDrag;
  const onScrollBeginDrag = useCallback(
    (event: NativeSyntheticEvent<NativeScrollEvent>) => {
      draggingRef.current = true;
      activateInteraction();
      childOnScrollBeginDrag?.(event);
    },
    [activateInteraction, childOnScrollBeginDrag]
  );

  const childOnScrollEndDrag = children.props.onScrollEndDrag;
  const onScrollEndDrag = useCallback(
    (event: NativeSyntheticEvent<NativeScrollEvent>) => {
      draggingRef.current = false;
      scheduleReleaseIfIdle();
      childOnScrollEndDrag?.(event);
    },
    [childOnScrollEndDrag, scheduleReleaseIfIdle]
  );

  const childOnMomentumScrollBegin = children.props.onMomentumScrollBegin;
  const onMomentumScrollBegin = useCallback(
    (event: NativeSyntheticEvent<NativeScrollEvent>) => {
      momentumRef.current = true;
      activateInteraction();
      childOnMomentumScrollBegin?.(event);
    },
    [activateInteraction, childOnMomentumScrollBegin]
  );

  const childOnMomentumScrollEnd = children.props.onMomentumScrollEnd;
  const onMomentumScrollEnd = useCallback(
    (event: NativeSyntheticEvent<NativeScrollEvent>) => {
      momentumRef.current = false;
      scheduleReleaseIfIdle();
      childOnMomentumScrollEnd?.(event);
    },
    [childOnMomentumScrollEnd, scheduleReleaseIfIdle]
  );

  const child = React.cloneElement(children, {
    onScroll,
    onScrollBeginDrag,
    onScrollEndDrag,
    onMomentumScrollBegin,
    onMomentumScrollEnd,
    scrollEnabled: children.props.scrollEnabled !== false && !gestureActive,
    scrollEventThrottle: children.props.scrollEventThrottle ?? 16,
  });

  return (
    <SelectionViewportContext.Provider value={viewportId}>
      <View
        ref={viewportRef}
        collapsable={false}
        onLayout={onLayout}
        onTouchStart={onTouchStart}
        onTouchEnd={onTouchFinish}
        onTouchCancel={onTouchFinish}
        style={[{ overflow: 'hidden' }, style]}
      >
        {child as React.ComponentProps<typeof View>['children']}
      </View>
    </SelectionViewportContext.Provider>
  );
};
