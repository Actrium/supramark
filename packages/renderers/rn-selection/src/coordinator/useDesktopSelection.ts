import { useCallback, useLayoutEffect, useMemo, useState } from 'react';
import {
  Platform,
  type GestureResponderEvent,
  type PointerEvent,
  type ViewProps,
} from 'react-native';
import type { SelectionPoint } from '../model';
import type { Point } from './hitTest';
import { createMouseSelection } from './mouse';
import type { SelectionStore } from './state';

export type ContactEvent = { nativeEvent: { pageX: number; pageY: number; timestamp?: number } };
type Contacts = {
  onTouchStart(event: ContactEvent): void;
  onTouchMove(event: ContactEvent): void;
  onTouchEnd(event: ContactEvent): void;
  onTouchCancel(): void;
};
interface Options {
  store: SelectionStore;
  enabled: boolean;
  revision: unknown;
  startAt(point: Point): SelectionPoint | null;
  pointAt(point: Point): SelectionPoint | null;
  local(event: ContactEvent): Point;
  active(value: boolean): void;
  cancelTouch(): void;
}

export function useDesktopSelection(options: Options, touch: Contacts) {
  const { store, enabled, revision, local, startAt, pointAt, active } = options;
  const mouse = useMemo(() => createMouseSelection(store), [store]);
  const [mouseUI, setMouseUI] = useState(Platform.OS === 'macos');
  const cancel = useCallback(() => {
    if (mouse.tracking()) {
      mouse.cancel();
      active(false);
    }
  }, [mouse, active]);
  useLayoutEffect(() => cancel, [enabled, revision, cancel]);

  function down(id: string | number, button: number, event: ContactEvent) {
    if (!enabled || button !== 0) return;
    cancel();
    options.cancelTouch();
    setMouseUI(true);
    mouse.down(id, startAt(local(event)));
  }
  function update(id: string | number, event: ContactEvent, released: boolean) {
    if (!enabled || !mouse.owns(id)) return;
    const point = pointAt(local(event));
    if (released) {
      mouse.up(id, point);
      active(false);
    } else {
      mouse.move(id, point);
      active(store.getSnapshot().phase === 'selecting');
    }
  }

  let handlers: ViewProps | undefined;
  if (Platform.OS === 'macos') {
    const button = (event: GestureResponderEvent) =>
      (event.nativeEvent as GestureResponderEvent['nativeEvent'] & { button?: number }).button;
    handlers = {
      onTouchStart: event => {
        const pressed = button(event);
        if (pressed !== undefined) down(event.nativeEvent.identifier, pressed, event);
      },
      onTouchMove: event => update(event.nativeEvent.identifier, event, false),
      onTouchEnd: event => update(event.nativeEvent.identifier, event, true),
      onTouchCancel: event => {
        if (mouse.owns(event.nativeEvent.identifier)) cancel();
      },
      onBlur: cancel,
    };
  } else if (Platform.OS === 'windows') {
    // Use one input stream: RNW also synthesizes touch events for every mouse press.
    const mouseEvent = (e: PointerEvent) => e.nativeEvent.pointerType === 'mouse';
    handlers = {
      onPointerDown: e => {
        if (mouseEvent(e)) down(e.nativeEvent.pointerId, e.nativeEvent.button, e);
        else if (!mouse.tracking()) {
          setMouseUI(false);
          touch.onTouchStart(e);
        }
      },
      onPointerMove: e => {
        if (mouseEvent(e)) {
          if ((e.nativeEvent.buttons & 1) !== 0) update(e.nativeEvent.pointerId, e, false);
        } else if (!mouse.tracking()) touch.onTouchMove(e);
      },
      onPointerUp: e => {
        if (mouseEvent(e)) update(e.nativeEvent.pointerId, e, true);
        else if (!mouse.tracking()) touch.onTouchEnd(e);
      },
      onPointerCancel: e => {
        if (mouse.owns(e.nativeEvent.pointerId)) cancel();
        else if (!mouseEvent(e)) touch.onTouchCancel();
      },
      onBlur: cancel,
    };
  }
  return { handlers, mouseUI, cancel };
}
