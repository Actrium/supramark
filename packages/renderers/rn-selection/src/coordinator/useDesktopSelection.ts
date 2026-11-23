import { useCallback, useLayoutEffect, useMemo, useState } from 'react';
import {
  Platform,
  type GestureResponderEvent,
  type PointerEvent,
  type ViewProps,
} from 'react-native';
import type { SelectionPoint } from '../model';
import type { Point } from './hitTest';
import { createMouseSelection, type PointerPosition } from './mouse';
import type { SelectionStore } from './state';

export type ContactEvent = { nativeEvent: PointerPosition };
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
  startAt(this: void, point: Point): SelectionPoint | null;
  pointAt(this: void, point: Point): SelectionPoint | null;
  local(this: void, event: ContactEvent): Point;
  macLocal(this: void, event: GestureResponderEvent): Point | null;
  active(this: void, value: boolean): void;
  cancelTouch(this: void): void;
}

export function useDesktopSelection(options: Options, touch: Contacts) {
  const { store, enabled, revision, local, macLocal, startAt, pointAt, active } = options;
  const mouse = useMemo(() => createMouseSelection(store), [store]);
  const [mouseUI, setMouseUI] = useState(Platform.OS === 'macos');
  const cancel = useCallback(() => {
    const tracking = mouse.tracking();
    mouse.cancel();
    if (tracking) active(false);
  }, [mouse, active]);
  useLayoutEffect(() => cancel, [enabled, revision, cancel]);

  function down(
    id: string | number,
    contact: PointerPosition & { button?: number },
    locate: () => Point | null
  ) {
    if (!enabled || contact.button !== 0) return;
    cancel();
    options.cancelTouch();
    setMouseUI(true);
    const point = locate();
    mouse.down(id, point && startAt(point), contact);
  }
  function update(
    id: string | number,
    contact: PointerPosition,
    locate: () => Point | null,
    released: boolean
  ) {
    if (!enabled || !mouse.owns(id)) return;
    const localPoint = locate();
    const point = localPoint && pointAt(localPoint);
    if (released) {
      mouse.up(id, point, contact);
      active(false);
    } else {
      mouse.move(id, point, contact);
      active(store.getSnapshot().phase === 'selecting');
    }
  }

  const suppressLinkPress = useCallback(
    (event: GestureResponderEvent) => {
      if (!enabled || !event?.nativeEvent) return false;
      const contact = event.nativeEvent as GestureResponderEvent['nativeEvent'] & {
        button?: number;
      };
      // RNW's responder touch identifier is also its Fabric pointerId. Keyboard
      // and accessibility activations carry no touch contact.
      if (
        contact.identifier == null ||
        !Number.isFinite(contact.pageX) ||
        !Number.isFinite(contact.pageY)
      )
        return false;
      if (Platform.OS === 'macos' && contact.button !== 0) return false;
      if (Platform.OS !== 'macos' && Platform.OS !== 'windows') return false;
      return mouse.suppressesPress(contact.identifier, contact);
    },
    [enabled, mouse]
  );

  let handlers: ViewProps | undefined;
  if (Platform.OS === 'macos') {
    handlers = {
      onTouchStart: event =>
        down(event.nativeEvent.identifier, event.nativeEvent, () => macLocal(event)),
      onTouchMove: event =>
        update(event.nativeEvent.identifier, event.nativeEvent, () => macLocal(event), false),
      onTouchEnd: event =>
        update(event.nativeEvent.identifier, event.nativeEvent, () => macLocal(event), true),
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
        if (mouseEvent(e)) down(e.nativeEvent.pointerId, e.nativeEvent, () => local(e));
        else if (!mouse.tracking()) {
          mouse.cancel();
          setMouseUI(false);
          touch.onTouchStart(e);
        }
      },
      onPointerMove: e => {
        if (mouseEvent(e)) {
          if ((e.nativeEvent.buttons & 1) !== 0)
            update(e.nativeEvent.pointerId, e.nativeEvent, () => local(e), false);
        } else if (!mouse.tracking()) touch.onTouchMove(e);
      },
      onPointerUp: e => {
        if (mouseEvent(e)) update(e.nativeEvent.pointerId, e.nativeEvent, () => local(e), true);
        else if (!mouse.tracking()) touch.onTouchEnd(e);
      },
      onPointerCancel: e => {
        if (mouse.owns(e.nativeEvent.pointerId)) cancel();
        else if (!mouseEvent(e)) touch.onTouchCancel();
      },
      onBlur: cancel,
    };
  }
  return { handlers, mouseUI, cancel, suppressLinkPress };
}
