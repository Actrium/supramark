import React, { useMemo } from 'react';
import {
  PanResponder,
  StyleSheet,
  View,
  type GestureResponderEvent,
  type PanResponderGestureState,
} from 'react-native';
import {
  computeHandles,
  HANDLE_KNOB_RADIUS,
  HANDLE_TOUCH_RADIUS,
  type HandleEdge,
  type HandleGeometry,
} from './handles';
import { useSelectionRects } from './SelectionOverlay';
import type { Point } from './hitTest';

export interface SelectionHandlesProps {
  color?: string; // default '#3399ff'
  zIndex?: number; // default 11
  /** Width of the vertical caret bar at each selection edge. */
  barWidth?: number;
  onHandleGrant?(edge: HandleEdge): void;
  onHandleMove?(point: Point): void;
  onHandleRelease?(point: Point): void;
  onHandleCancel?(): void;
  toRootPoint?(page: Point): Point;
}

/**
 * The two drag handles at the ends of the selection.
 *
 * These used to be drawn by UIKit / Android as a side effect of pushing a range
 * into a native text view, which is also why they only ever appeared for
 * single-block selections. Drawn here they look the same on both platforms and
 * work across blocks.
 *
 * The visible knob owns the responder and expands its touch target with
 * `hitSlop`. Android can optimize or reorder fully transparent hit views in a
 * way that lets the underlying Text win the press; tying the responder to the
 * painted knob keeps the native hit target concrete while still giving the user
 * the larger 44pt-ish grab area.
 */
export const SelectionHandles: React.FC<SelectionHandlesProps> = ({
  color = '#3399ff',
  zIndex = 11,
  barWidth = 2,
  onHandleGrant,
  onHandleMove,
  onHandleRelease,
  onHandleCancel,
  toRootPoint,
}) => {
  const rects = useSelectionRects();
  const handles = computeHandles(rects);
  if (handles === null) return null;

  const diameter = HANDLE_KNOB_RADIUS * 2;
  const touchSlop = HANDLE_TOUCH_RADIUS - HANDLE_KNOB_RADIUS;
  return (
    <View pointerEvents="box-none" style={[StyleSheet.absoluteFill, { zIndex }]}>
      {[handles.start, handles.end]
        .filter(handle => handle.visible)
        .map(handle => (
          <React.Fragment key={handle.edge}>
            <View
              pointerEvents="none"
              style={{
                position: 'absolute',
                left: handle.x - barWidth / 2,
                top: handle.y,
                width: barWidth,
                height: handle.h,
                backgroundColor: color,
              }}
            />
            <HandleKnob
              color={color}
              diameter={diameter}
              handle={handle}
              hitSlop={touchSlop}
              onHandleCancel={onHandleCancel}
              onHandleGrant={onHandleGrant}
              onHandleMove={onHandleMove}
              onHandleRelease={onHandleRelease}
              toRootPoint={toRootPoint}
            />
          </React.Fragment>
        ))}
    </View>
  );
};

const HandleKnob: React.FC<{
  color: string;
  diameter: number;
  handle: HandleGeometry;
  hitSlop: number;
  onHandleGrant?(edge: HandleEdge): void;
  onHandleMove?(point: Point): void;
  onHandleRelease?(point: Point): void;
  onHandleCancel?(): void;
  toRootPoint?(page: Point): Point;
}> = ({
  color,
  diameter,
  handle,
  hitSlop,
  onHandleCancel,
  onHandleGrant,
  onHandleMove,
  onHandleRelease,
  toRootPoint,
}) => {
  const origin = useMemo(
    () => ({
      x: handle.knobX - HANDLE_KNOB_RADIUS,
      y: handle.knobY - HANDLE_KNOB_RADIUS,
    }),
    [handle.knobX, handle.knobY]
  );
  const panResponder = useMemo(
    () =>
      PanResponder.create({
        onStartShouldSetPanResponder: () => true,
        onStartShouldSetPanResponderCapture: () => true,
        onMoveShouldSetPanResponder: () => true,
        onMoveShouldSetPanResponderCapture: () => true,
        onPanResponderGrant: () => onHandleGrant?.(handle.edge),
        onPanResponderMove: (event, gestureState) =>
          onHandleMove?.(gestureToRootPoint(event, handle, gestureState, toRootPoint)),
        onPanResponderRelease: (event, gestureState) =>
          onHandleRelease?.(gestureToRootPoint(event, handle, gestureState, toRootPoint)),
        onPanResponderTerminate: onHandleCancel,
        onPanResponderTerminationRequest: () => false,
        onShouldBlockNativeResponder: () => true,
      }),
    [handle, onHandleCancel, onHandleGrant, onHandleMove, onHandleRelease, toRootPoint]
  );

  return (
    <View
      collapsable={false}
      hitSlop={hitSlop}
      style={{
        position: 'absolute',
        left: origin.x,
        top: origin.y,
        width: diameter,
        height: diameter,
        borderRadius: HANDLE_KNOB_RADIUS,
        backgroundColor: color,
      }}
      {...panResponder.panHandlers}
    />
  );
};

function gestureToRootPoint(
  event: GestureResponderEvent,
  handle: HandleGeometry,
  gestureState: PanResponderGestureState,
  toRootPoint?: (page: Point) => Point
): Point {
  const { pageX, pageY } = event.nativeEvent;
  if (toRootPoint && Number.isFinite(pageX) && Number.isFinite(pageY)) {
    return toRootPoint({ x: pageX, y: pageY });
  }
  return { x: handle.knobX + gestureState.dx, y: handle.knobY + gestureState.dy };
}
