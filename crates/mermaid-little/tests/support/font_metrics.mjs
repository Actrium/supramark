// Node-side font metrics mirror of src/font_metrics.rs.
//
// Both sides read the SAME DejaVu Sans / DejaVu Sans Bold TTFs. Same
// inputs → same outputs. This is the anchor that lets the reference
// pipeline and the Rust renderer agree byte-for-byte without running
// a shared JS runtime.
//
// API mirrors the Rust module:
//   charWidth(ch, family, size, bold)
//   textWidth(text, family, size, bold)
//   lineHeight(family, size)
//   ascent(family, size)
//   descent(family, size)
//
// Font family resolution: Java's logical "SansSerif" / "Dialog" and
// anything not explicitly monospaced → DejaVu Sans. "Monospaced" /
// "Courier" (Java logical name, no " New") / CSS "monospace" → DejaVu
// Sans Mono. mermaid's default font list starts with "trebuchet ms" —
// uninstalled on the reference machine → falls back to sans.

import opentype from 'opentype.js';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const FONTS_DIR = join(HERE, 'fonts');

function loadFace(fileName) {
  const buf = readFileSync(join(FONTS_DIR, fileName));
  const font = opentype.parse(buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength));
  // Extract the bits we actually need so we don't carry opentype
  // objects across hot paths.
  const upem = font.unitsPerEm;
  const os2 = font.tables.os2 ?? {};
  const hhea = font.tables.hhea ?? {};
  // DejaVu reports ascender/descender via hhea in design units.
  const ascender = hhea.ascender ?? os2.sTypoAscender ?? 0;
  const descender = hhea.descender ?? os2.sTypoDescender ?? 0; // negative
  const typoAscender = os2.sTypoAscender ?? ascender;
  return {
    upem,
    ascender,
    descender,
    typoAscender,
    glyphAdvance(codepoint) {
      const glyph = font.charToGlyph(String.fromCodePoint(codepoint));
      if (!glyph || glyph.index === 0) return null;
      return glyph.advanceWidth;
    },
  };
}

const FACES = {
  sans: loadFace('DejaVuSans.ttf'),
  sansBold: loadFace('DejaVuSans-Bold.ttf'),
};

function resolveFace(family, bold) {
  const primary = (family ?? '').split(',')[0].trim().toLowerCase();
  // Mono family not included yet — mermaid default never resolves to it.
  // If a future fixture forces us into mono, we'll add DejaVuSansMono.
  const _isMono =
    primary === 'monospaced' || primary === 'monospace' || primary === 'courier';
  // For MVP we treat mono as sans too (both Rust and here) — the sister
  // project bakes all four faces; we'll add the mono ones when a real
  // fixture needs them.
  return bold ? FACES.sansBold : FACES.sans;
}

export function charWidth(ch, family, size, bold = false) {
  if (ch === '\n' || ch === '\r') return 0;
  const face = resolveFace(family, bold);
  const cp = ch.codePointAt(0);
  const adv = face.glyphAdvance(cp);
  if (adv != null) return (adv / face.upem) * size;
  // Fallback: space advance
  const sp = face.glyphAdvance(0x20);
  if (sp != null) return (sp / face.upem) * size;
  return size * 0.6;
}

export function textWidth(text, family, size, bold = false) {
  let w = 0;
  for (const ch of text ?? '') w += charWidth(ch, family, size, bold);
  return w;
}

export function lineHeight(family, size) {
  const face = resolveFace(family, false);
  return ((face.ascender + Math.abs(face.descender)) / face.upem) * size;
}

export function ascent(family, size) {
  const face = resolveFace(family, false);
  return (face.ascender / face.upem) * size;
}

export function descent(family, size) {
  const face = resolveFace(family, false);
  return (Math.abs(face.descender) / face.upem) * size;
}

// Convenience: measure multi-line text exactly the way mermaid's
// getBBox shim needs it.
export function measureTextBlock(text, family, size, bold = false) {
  const lines = (text ?? '').split('\n');
  let maxW = 0;
  for (const l of lines) {
    const w = textWidth(l, family, size, bold);
    if (w > maxW) maxW = w;
  }
  const h = lineHeight(family, size) * lines.length;
  return { width: maxW, height: h };
}
