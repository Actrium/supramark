#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';

const { PNG } = createRequire(import.meta.url)('pngjs');

const [mode, file] = process.argv.slice(2);

if (!mode || !file || !['gesture', 'viewport', 'cjk'].includes(mode)) {
  console.error(
    'Usage: node scripts/assert-selection-visual.mjs <gesture|viewport|cjk> <screenshot.png>'
  );
  process.exit(2);
}

const png = PNG.sync.read(readFileSync(file));
const image = { width: png.width, height: png.height, data: png.data };

function pixelAt(index) {
  const offset = index * 4;
  return {
    r: image.data[offset],
    g: image.data[offset + 1],
    b: image.data[offset + 2],
    a: image.data[offset + 3],
  };
}

function boxWidth(box) {
  return box.x1 - box.x0 + 1;
}

function boxHeight(box) {
  return box.y1 - box.y0 + 1;
}

function intersects(a, b) {
  return a.x0 <= b.x1 && a.x1 >= b.x0 && a.y0 <= b.y1 && a.y1 >= b.y0;
}

function unionBoxes(boxes) {
  return boxes.reduce(
    (box, at) => ({
      x0: Math.min(box.x0, at.x0),
      y0: Math.min(box.y0, at.y0),
      x1: Math.max(box.x1, at.x1),
      y1: Math.max(box.y1, at.y1),
    }),
    { x0: image.width, y0: image.height, x1: 0, y1: 0 }
  );
}

function boxDistance(a, b) {
  const dx = a.x1 < b.x0 ? b.x0 - a.x1 : b.x1 < a.x0 ? a.x0 - b.x1 : 0;
  const dy = a.y1 < b.y0 ? b.y0 - a.y1 : b.y1 < a.y0 ? a.y0 - b.y1 : 0;
  return Math.hypot(dx, dy);
}

function inflate(box, amount) {
  return {
    x0: Math.max(0, box.x0 - amount),
    y0: Math.max(0, box.y0 - amount),
    x1: Math.min(image.width - 1, box.x1 + amount),
    y1: Math.min(image.height - 1, box.y1 + amount),
  };
}

function fail(message) {
  throw new Error(`${message} (${file})`);
}

function assert(condition, message) {
  if (!condition) fail(message);
}

function findComponents(predicate) {
  const total = image.width * image.height;
  const mask = new Uint8Array(total);
  for (let i = 0; i < total; i += 1) {
    if (predicate(pixelAt(i), i)) mask[i] = 1;
  }

  const components = [];
  const stack = new Int32Array(total);

  for (let i = 0; i < total; i += 1) {
    if (mask[i] === 0) continue;
    let top = 0;
    let area = 0;
    let x0 = image.width;
    let y0 = image.height;
    let x1 = 0;
    let y1 = 0;

    mask[i] = 0;
    stack[top] = i;
    top += 1;

    while (top > 0) {
      top -= 1;
      const at = stack[top];
      area += 1;

      const x = at % image.width;
      const y = Math.floor(at / image.width);
      if (x < x0) x0 = x;
      if (x > x1) x1 = x;
      if (y < y0) y0 = y;
      if (y > y1) y1 = y;

      const left = at - 1;
      const right = at + 1;
      const up = at - image.width;
      const down = at + image.width;

      if (x > 0 && mask[left] === 1) {
        mask[left] = 0;
        stack[top] = left;
        top += 1;
      }
      if (x < image.width - 1 && mask[right] === 1) {
        mask[right] = 0;
        stack[top] = right;
        top += 1;
      }
      if (y > 0 && mask[up] === 1) {
        mask[up] = 0;
        stack[top] = up;
        top += 1;
      }
      if (y < image.height - 1 && mask[down] === 1) {
        mask[down] = 0;
        stack[top] = down;
        top += 1;
      }
    }

    components.push({ area, x0, y0, x1, y1 });
  }

  return components.sort((a, b) => b.area - a.area);
}

function isHandleBlue({ r, g, b, a }) {
  return a > 200 && r >= 20 && r <= 90 && g >= 115 && g <= 190 && b >= 215;
}

function isHighlightBlue({ r, g, b, a }) {
  return a > 200 && r >= 135 && r <= 230 && g >= 180 && g <= 245 && b >= 225 && b - r >= 25;
}

function isDark({ r, g, b, a }) {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  return a > 200 && max <= 90 && max - min <= 30;
}

function isGlyphInk(pixel) {
  const luminance = pixel.r * 0.299 + pixel.g * 0.587 + pixel.b * 0.114;
  return pixel.a > 200 && luminance < 130 && !isHandleBlue(pixel);
}

function estimateScale(handles) {
  const widths = handles
    .map(handle => boxWidth(handle))
    .filter(width => width >= 8 && width <= 80)
    .sort((a, b) => a - b);
  if (widths.length > 0) {
    return widths[Math.floor(widths.length / 2)] / 12;
  }
  return Math.max(1, image.width / 430);
}

function summarize(box) {
  return `${boxWidth(box)}x${boxHeight(box)}@${box.x0},${box.y0}`;
}

function assertGestureVisual(minimumHandles = 2) {
  const highlights = findComponents(isHighlightBlue).filter(component => component.area > 150);
  const handles = findComponents(isHandleBlue).filter(component => component.area > 80);
  const darks = findComponents(isDark).filter(
    component => component.area > 1000 && boxWidth(component) > 80 && boxHeight(component) > 20
  );

  assert(highlights.length > 0, 'expected visible selection highlight pixels');
  assert(
    handles.length >= minimumHandles,
    `expected at least ${minimumHandles} visible selection handle(s), found ${handles.length}`
  );
  assert(darks.length > 0, 'expected visible toolbar background pixels');

  const scale = estimateScale(handles.slice(0, 4));
  const pad = Math.max(4, Math.round(scale * 8));
  const visibleHandles = handles.slice(0, Math.max(2, minimumHandles));
  const handlesUnion = unionBoxes(visibleHandles);
  const toolbar = darks
    .filter(component => boxDistance(component, handlesUnion) < image.height * 0.35)
    .sort((a, b) => boxDistance(a, handlesUnion) - boxDistance(b, handlesUnion))[0];

  assert(toolbar !== undefined, 'expected toolbar pixels near the selection handles');

  for (const handle of visibleHandles) {
    assert(
      !intersects(toolbar, inflate(handle, pad)),
      `toolbar overlaps a handle: toolbar ${summarize(toolbar)}, handle ${summarize(handle)}`
    );
  }

  const highlightArea = highlights.reduce((sum, component) => sum + component.area, 0);
  assert(highlightArea > image.width * image.height * 0.0002, 'selection highlight is too small');

  console.log(
    `[selection-visual] gesture ok: highlights=${highlights.length}, handles=${visibleHandles
      .map(summarize)
      .join(' ')}, toolbar=${summarize(toolbar)}`
  );
}

function collectDarkTextBox(y0, y1, x0, x1) {
  let count = 0;
  const box = { x0: image.width, y0: image.height, x1: 0, y1: 0 };
  for (let y = Math.max(0, y0); y <= Math.min(image.height - 1, y1); y += 1) {
    for (let x = Math.max(0, x0); x <= Math.min(image.width - 1, x1); x += 1) {
      const pixel = pixelAt(y * image.width + x);
      if (!isGlyphInk(pixel)) continue;
      count += 1;
      if (x < box.x0) box.x0 = x;
      if (x > box.x1) box.x1 = x;
      if (y < box.y0) box.y0 = y;
      if (y > box.y1) box.y1 = y;
    }
  }
  return count === 0 ? null : { ...box, count };
}

function assertCjkVisual() {
  const highlights = findComponents(isHighlightBlue).filter(
    component => component.area > 200 && boxWidth(component) > 20 && boxHeight(component) > 10
  );
  const handles = findComponents(isHandleBlue).filter(component => component.area > 80);

  assert(highlights.length > 0, 'expected CJK selection highlight pixels');
  const highlight = highlights[0];
  const scale = estimateScale(handles.slice(0, 4));
  const pad = Math.max(6, Math.round(scale * 8));
  const searchRight = highlight.x0 + Math.round(boxWidth(highlight) * 3.2);
  const textBox = collectDarkTextBox(highlight.y0, highlight.y1, highlight.x0 - pad, searchRight);

  assert(textBox !== null && textBox.count > 80, 'expected dark CJK glyph pixels near highlight');

  const textWidth = boxWidth(textBox);
  const highlightWidth = boxWidth(highlight);
  const ratio = highlightWidth / textWidth;
  const textMidY = (textBox.y0 + textBox.y1) / 2;
  const expectedRight = textBox.x0 + textWidth / 2;
  const rightDelta = Math.abs(highlight.x1 - expectedRight) / textWidth;

  assert(ratio >= 0.42 && ratio <= 0.64, `CJK half-selection width ratio is ${ratio.toFixed(3)}`);
  assert(
    rightDelta <= 0.14,
    `CJK highlight right edge drift is ${(rightDelta * 100).toFixed(1)}% of text width`
  );
  assert(
    highlight.x1 < textBox.x1 - textWidth * 0.2,
    'CJK highlight appears to cover the whole text row'
  );
  assert(
    textMidY >= highlight.y0 && textMidY <= highlight.y1,
    `CJK highlight vertical span misses glyph center: highlight ${summarize(
      highlight
    )}, text ${summarize(textBox)}`
  );

  console.log(
    `[selection-visual] cjk ok: highlight=${summarize(highlight)}, text=${summarize(
      textBox
    )}, ratio=${ratio.toFixed(3)}`
  );
}

try {
  if (mode === 'gesture') assertGestureVisual();
  else if (mode === 'viewport') assertGestureVisual(1);
  else assertCjkVisual();
} catch (err) {
  console.error(`[selection-visual] ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
