import { describe, expect, test } from 'bun:test';
import {
  computeToolbarPlacement,
  unionRect,
  DEFAULT_TOOLBAR_ITEMS,
  TOOLBAR_ARROW_INSET,
  TOOLBAR_GAP,
  TOOLBAR_MARGIN,
} from '../../coordinator/toolbar';

const BAR = { w: 200, h: 40 };
const SCREEN = { w: 375, h: 800 };

describe('unionRect', () => {
  test('no rects yields null', () => {
    expect(unionRect([])).toBeNull();
  });

  test('bounds every rect', () => {
    expect(
      unionRect([
        { x: 30, y: 100, w: 70, h: 20 },
        { x: 0, y: 120, w: 100, h: 20 },
      ])
    ).toEqual({ x: 0, y: 100, w: 100, h: 40 });
  });
});

describe('computeToolbarPlacement', () => {
  test('no selection means no bar', () => {
    expect(computeToolbarPlacement([], BAR, SCREEN)).toBeNull();
  });

  test('sits above the selection by default', () => {
    const placement = computeToolbarPlacement([{ x: 100, y: 300, w: 100, h: 20 }], BAR, SCREEN);
    expect(placement).toEqual({
      x: 150 - BAR.w / 2,
      y: 300 - TOOLBAR_GAP - BAR.h,
      side: 'above',
      arrowX: BAR.w / 2,
    });
  });

  test('flips below when the selection is near the top', () => {
    const placement = computeToolbarPlacement([{ x: 100, y: 4, w: 100, h: 20 }], BAR, SCREEN);
    expect(placement?.side).toBe('below');
    expect(placement?.y).toBe(4 + 20 + TOOLBAR_GAP);
  });

  test('clamps into the viewport when neither side fits', () => {
    // A selection taller than the screen: the bar must still be reachable
    // rather than positioned off-screen.
    const tall = [{ x: 0, y: 0, w: 375, h: 800 }];
    const placement = computeToolbarPlacement(tall, BAR, SCREEN);
    expect(placement?.y).toBe(TOOLBAR_MARGIN);
  });

  test('clamps horizontally at the left edge and keeps the arrow on the text', () => {
    const placement = computeToolbarPlacement([{ x: 0, y: 300, w: 20, h: 20 }], BAR, SCREEN);
    expect(placement?.x).toBe(TOOLBAR_MARGIN);
    // The bar moved right but the arrow still points at the selection centre.
    expect(placement?.arrowX).toBe(TOOLBAR_ARROW_INSET);
  });

  test('clamps horizontally at the right edge', () => {
    const placement = computeToolbarPlacement([{ x: 355, y: 300, w: 20, h: 20 }], BAR, SCREEN);
    expect(placement?.x).toBe(SCREEN.w - TOOLBAR_MARGIN - BAR.w);
    expect(placement?.arrowX).toBe(BAR.w - TOOLBAR_ARROW_INSET);
  });

  test('a bar wider than the viewport still gets a usable position', () => {
    const placement = computeToolbarPlacement(
      [{ x: 100, y: 300, w: 100, h: 20 }],
      { w: 900, h: 40 },
      SCREEN
    );
    expect(placement?.x).toBe(TOOLBAR_MARGIN);
  });

  test('honours custom gap and margin', () => {
    const placement = computeToolbarPlacement([{ x: 100, y: 300, w: 100, h: 20 }], BAR, SCREEN, {
      gap: 20,
    });
    expect(placement?.y).toBe(300 - 20 - BAR.h);
  });

  test('keeps an above bar clear of the start handle knob', () => {
    const placement = computeToolbarPlacement([{ x: 100, y: 300, w: 100, h: 20 }], BAR, SCREEN, {
      avoidRects: [{ x: 94, y: 288, w: 12, h: 12 }],
    });

    expect(placement?.side).toBe('above');
    expect((placement?.y ?? 0) + BAR.h).toBe(288 - TOOLBAR_GAP);
  });

  test('keeps a below bar clear of the end handle knob after flipping', () => {
    const placement = computeToolbarPlacement([{ x: 100, y: 56, w: 100, h: 20 }], BAR, SCREEN, {
      avoidRects: [
        { x: 94, y: 44, w: 12, h: 12 },
        { x: 194, y: 76, w: 12, h: 12 },
      ],
    });

    expect(placement?.side).toBe('below');
    expect(placement?.y).toBe(88 + TOOLBAR_GAP);
  });

  test('chooses the clamped side with less handle overlap when neither side fits', () => {
    const placement = computeToolbarPlacement(
      [{ x: 100, y: 20, w: 100, h: 70 }],
      BAR,
      {
        w: 375,
        h: 80,
      },
      {
        avoidRects: [
          { x: 78, y: -8, w: 44, h: 44 },
          { x: 178, y: 68, w: 44, h: 44 },
        ],
      }
    );

    expect(placement?.side).toBe('below');
    expect(placement?.y).toBe(80 - TOOLBAR_MARGIN - BAR.h);
  });

  test('anchors to the union of a multi-line selection', () => {
    const placement = computeToolbarPlacement(
      [
        { x: 100, y: 300, w: 100, h: 20 },
        { x: 0, y: 320, w: 300, h: 20 },
      ],
      BAR,
      SCREEN
    );
    // Union spans x 0..300, so the bar centres on 150 and sits above y=300.
    expect(placement?.x).toBe(50);
    expect(placement?.y).toBe(300 - TOOLBAR_GAP - BAR.h);
  });
});

describe('DEFAULT_TOOLBAR_ITEMS', () => {
  test('offers the two actions the model can always satisfy', () => {
    expect(DEFAULT_TOOLBAR_ITEMS.map(i => [i.id, i.format])).toEqual([
      ['copy', 'plainText'],
      ['copy-markdown', 'markdown'],
    ]);
  });
});
