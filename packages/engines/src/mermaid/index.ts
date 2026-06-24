import { installHostMetricsBridge } from '../host-bridge.js';

let renderFn:
  | ((code: string, options?: Record<string, unknown>) => Promise<string> | string)
  | null = null;

type MermaidThemeVars = Record<string, string>;

function parseStyleAttribute(style: string): MermaidThemeVars {
  const vars: MermaidThemeVars = {};
  for (const part of style.split(';')) {
    const idx = part.indexOf(':');
    if (idx <= 0) continue;
    const key = part.slice(0, idx).trim();
    const value = part.slice(idx + 1).trim();
    if (!key || !value) continue;
    vars[key] = value;
  }
  return vars;
}

function extractFontFamily(cssText: string, selector: string): string | null {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = cssText.match(new RegExp(`${escaped}\\s*\\{([\\s\\S]*?)\\}`, 'i'));
  if (!match) return null;
  const body = match[1];
  const fontMatch = body.match(/font-family\s*:\s*([^;}\n]+)/i);
  return fontMatch ? fontMatch[1].trim() : null;
}

function clampChannel(n: number): number {
  return Math.max(0, Math.min(255, Math.round(n)));
}

function parseColor(value: string | null | undefined): { r: number; g: number; b: number } | null {
  if (!value) return null;
  const normalized = String(value).trim();
  const hex = normalized.match(/^#([0-9a-f]{3}|[0-9a-f]{6})$/i);
  if (hex) {
    const raw = hex[1];
    if (raw.length === 3) {
      return {
        r: parseInt(raw[0] + raw[0], 16),
        g: parseInt(raw[1] + raw[1], 16),
        b: parseInt(raw[2] + raw[2], 16),
      };
    }
    return {
      r: parseInt(raw.slice(0, 2), 16),
      g: parseInt(raw.slice(2, 4), 16),
      b: parseInt(raw.slice(4, 6), 16),
    };
  }

  const rgb = normalized.match(/^rgba?\(([^)]+)\)$/i);
  if (!rgb) return null;
  const parts = rgb[1].split(',').map(part => parseFloat(part.trim()));
  if (parts.length < 3 || parts.some(Number.isNaN)) return null;
  return { r: parts[0], g: parts[1], b: parts[2] };
}

function colorToHex(color: { r: number; g: number; b: number }): string {
  const toHex = (n: number) => clampChannel(n).toString(16).padStart(2, '0');
  return `#${toHex(color.r)}${toHex(color.g)}${toHex(color.b)}`;
}

function mixColors(
  fg: { r: number; g: number; b: number },
  bg: { r: number; g: number; b: number },
  fgPercent: number
): { r: number; g: number; b: number } {
  return {
    r: fg.r * fgPercent + bg.r * (1 - fgPercent),
    g: fg.g * fgPercent + bg.g * (1 - fgPercent),
    b: fg.b * fgPercent + bg.b * (1 - fgPercent),
  };
}

function splitTopLevel(input: string): string[] {
  const parts: string[] = [];
  let current = '';
  let depth = 0;

  for (let i = 0; i < input.length; i++) {
    const ch = input[i];
    if (ch === '(') depth++;
    if (ch === ')') depth = Math.max(0, depth - 1);
    if (ch === ',' && depth === 0) {
      parts.push(current.trim());
      current = '';
      continue;
    }
    current += ch;
  }

  if (current.trim()) {
    parts.push(current.trim());
  }
  return parts;
}

function resolveExpression(value: string, vars: MermaidThemeVars): string {
  let resolved = value.trim();

  const varPattern = /var\((--[\w-]+)(?:,\s*([^()]+|var\([^()]+\)|color-mix\([^()]+\)))?\)/g;
  let previous = '';
  while (resolved !== previous && resolved.includes('var(')) {
    previous = resolved;
    resolved = resolved.replace(varPattern, (_match, name: string, fallback?: string) => {
      const hit = vars[name];
      if (hit) return hit;
      return fallback ? resolveExpression(String(fallback), vars) : '';
    });
  }

  const colorMix = resolved.match(/^color-mix\(in\s+srgb,\s*(.+)\)$/i);
  if (colorMix) {
    const args = splitTopLevel(colorMix[1]);
    if (args.length >= 2) {
      const first = args[0].match(/^(.*?)(?:\s+(\d+(?:\.\d+)?)%)?$/);
      const second = args[1].match(/^(.*?)(?:\s+(\d+(?:\.\d+)?)%)?$/);
      const firstExpr = first?.[1]?.trim() ?? args[0];
      const secondExpr = second?.[1]?.trim() ?? args[1];
      const firstPercent = first?.[2]
        ? parseFloat(first[2]) / 100
        : second?.[2]
          ? 1 - parseFloat(second[2]) / 100
          : 0.5;

      const fg = parseColor(resolveExpression(firstExpr, vars));
      const bg = parseColor(resolveExpression(secondExpr, vars));
      if (fg && bg) {
        return colorToHex(mixColors(fg, bg, firstPercent));
      }
    }
  }

  return resolved;
}

function buildThemeVars(
  rootStyle: MermaidThemeVars,
  options?: Record<string, unknown>
): MermaidThemeVars {
  const vars: MermaidThemeVars = {};
  const copyKeys = ['bg', 'fg', 'line', 'accent', 'muted', 'surface', 'border'] as const;

  for (const key of copyKeys) {
    const optionValue = typeof options?.[key] === 'string' ? String(options[key]).trim() : '';
    const styleValue = rootStyle[`--${key}`];
    if (optionValue) {
      vars[`--${key}`] = optionValue;
    } else if (styleValue) {
      vars[`--${key}`] = styleValue;
    }
  }

  const bg = parseColor(vars['--bg'] ?? '#ffffff') ?? { r: 255, g: 255, b: 255 };
  const fg = parseColor(vars['--fg'] ?? '#27272a') ?? { r: 39, g: 39, b: 42 };
  const line = parseColor(vars['--line']);
  const accent = parseColor(vars['--accent']);
  const muted = parseColor(vars['--muted']);
  const surface = parseColor(vars['--surface']);
  const border = parseColor(vars['--border']);

  vars['--_text'] = vars['--fg'] ?? colorToHex(fg);
  vars['--_text-sec'] = colorToHex(muted ?? mixColors(fg, bg, 0.6));
  vars['--_text-muted'] = colorToHex(muted ?? mixColors(fg, bg, 0.4));
  vars['--_text-faint'] = colorToHex(mixColors(fg, bg, 0.25));
  vars['--_line'] = colorToHex(line ?? mixColors(fg, bg, 0.3));
  vars['--_arrow'] = colorToHex(accent ?? mixColors(fg, bg, 0.5));
  vars['--_node-fill'] = colorToHex(surface ?? mixColors(fg, bg, 0.03));
  vars['--_node-stroke'] = colorToHex(border ?? mixColors(fg, bg, 0.2));
  vars['--_group-fill'] = vars['--bg'] ?? colorToHex(bg);
  vars['--_group-hdr'] = colorToHex(mixColors(fg, bg, 0.05));
  vars['--_inner-stroke'] = colorToHex(mixColors(fg, bg, 0.12));
  vars['--_key-badge'] = colorToHex(mixColors(fg, bg, 0.1));

  return vars;
}

function rewriteStyleValue(value: string, vars: MermaidThemeVars): string {
  return value.replace(
    /var\([^)]*\)|color-mix\([^)]*\)|rgba?\([^)]*\)|#[0-9a-fA-F]{3,6}/g,
    match => {
      const resolved = resolveExpression(match, vars).trim();
      return resolved || match;
    }
  );
}

function applyFontFamilies(
  svg: string,
  textFontFamily: string | null,
  monoFontFamily: string | null
): string {
  let next = svg;

  if (monoFontFamily) {
    next = next.replace(/<text\b([^>]*?)>/gi, (match, attrs) => {
      const hasMonoClass = /\sclass="[^"]*\bmono\b[^"]*"/i.test(match);
      const cleanedAttrs = attrs.replace(/\sclass="[^"]*\bmono\b[^"]*"/gi, '');
      if (!hasMonoClass) {
        return `<text${cleanedAttrs}>`;
      }
      if (/font-family=/.test(match) || /style="[^"]*font-family:/.test(match)) {
        return `<text${cleanedAttrs}>`;
      }
      return `<text${cleanedAttrs} font-family="${monoFontFamily.replace(/"/g, '&quot;')}">`;
    });
  }

  if (textFontFamily) {
    next = next.replace(/<text\b([^>]*?)>/gi, (match, attrs) => {
      if (/font-family=/.test(match) || /style="[^"]*font-family:/.test(match)) {
        return match;
      }
      return `<text${attrs} font-family="${textFontFamily.replace(/"/g, '&quot;')}">`;
    });
  }

  return next;
}

function normalizeMermaidHtmlLabels(svg: string): string {
  const withVisibleLabels = svg.replace(/<foreignObject\b([^>]*)>/gi, (match, attrs: string) => {
    if (/\soverflow=/.test(match)) {
      return match;
    }
    return `<foreignObject overflow="visible"${attrs}>`;
  });

  return withVisibleLabels.replace(/<p\b([^>]*)>/gi, (match, attrs: string) => {
    const styleMatch = attrs.match(/\sstyle="([^"]*)"/i);
    if (!styleMatch) {
      return `<p${attrs} style="margin:0">`;
    }

    const styleValue = styleMatch[1];
    if (/(^|;)\s*margin\s*:/.test(styleValue)) {
      return match;
    }

    return `<p${attrs.replace(styleMatch[0], ` style="margin:0; ${styleValue}"`)}>`;
  });
}

function inlineMermaidSvg(svg: string, options?: Record<string, unknown>): string {
  const styleMatch = svg.match(/<style[^>]*>([\s\S]*?)<\/style>/i);
  const cssText = styleMatch?.[1] ?? '';
  const textFontFamily = extractFontFamily(cssText, 'text');
  const monoFontFamily = extractFontFamily(cssText, '.mono');

  const rootStyleMatch = svg.match(/<svg\b[^>]*\sstyle="([^"]*)"/i);
  const rootStyle = rootStyleMatch ? parseStyleAttribute(rootStyleMatch[1]) : {};
  const vars = buildThemeVars(rootStyle, options);

  let next = svg.replace(/<style[^>]*>([\s\S]*?)<\/style>/gi, (_match, styleBody: string) => {
    const rewritten = rewriteStyleValue(styleBody, vars).trim();
    return rewritten ? `<style>${rewritten}</style>` : '';
  });
  next = next.replace(/\sstyle="([^"]*)"/gi, (_match, styleValue: string) => {
    const rewritten = rewriteStyleValue(styleValue, vars)
      .split(';')
      .map(part => part.trim())
      .filter(Boolean)
      .filter(part => !part.startsWith('--') && !part.startsWith('background:'));
    return rewritten.length > 0 ? ` style="${rewritten.join(';')}"` : '';
  });

  next = next.replace(
    /\s(fill|stroke|color|stop-color)="([^"]*)"/gi,
    (match, attr: string, value: string) => {
      const rewritten = resolveExpression(value, vars).trim();
      return rewritten ? ` ${attr}="${rewritten}"` : match;
    }
  );

  next = next.replace(/<svg\b([^>]*)>/i, (_match, attrs: string) => {
    const cleaned = attrs.replace(/\sclass="[^"]*"/gi, '');
    return `<svg${cleaned}>`;
  });

  next = applyFontFamilies(next, textFontFamily, monoFontFamily);
  next = normalizeMermaidHtmlLabels(next);
  return next;
}

async function ensureLoaded(): Promise<
  (code: string, options?: Record<string, unknown>) => Promise<string> | string
> {
  if (renderFn) return renderFn;

  // Install the host text-metrics bridge before loading the wasm so the
  // wasm's metrics-host-callback impl can resolve `supramark.measureText`
  // on first render. Idempotent.
  installHostMetricsBridge();

  // mermaid-little-web is a wasm-bindgen wrapper around the
  // Rust crate `mermaid-little`. It produces SVG without DOM, headless
  // browsers, or the upstream JS Mermaid bundle. The wasm initialises
  // as a side effect of the ESM import (`import * as wasm from "./*.wasm"`).
  // Some wasm-bindgen builds still ship a default `init()` — probe
  // defensively so a re-init does not throw.
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const mod: any = await import('@actrium/mermaid-little-web' as string);

  const init =
    (typeof mod.default === 'function' && mod.default) ||
    (typeof mod.init === 'function' && mod.init) ||
    null;
  if (init) {
    try {
      await init();
    } catch {
      // Already initialised via the module-import side effect — ignore.
    }
  }

  const convert =
    (typeof mod.convert === 'function' && mod.convert) ||
    (typeof mod.render === 'function' && mod.render) ||
    null;
  if (!convert) {
    throw new Error(
      '`@actrium/mermaid-little-web` is missing a convert / render entry. ' +
        'Did `bun run build:wasm` complete?'
    );
  }

  renderFn = (code: string) => {
    // Underlying wasm `convert(code)` is synchronous; await tolerates
    // both sync and async returns.
    return convert(code);
  };
  return renderFn;
}

export async function renderMermaidSvg(
  code: string,
  options?: Record<string, unknown>
): Promise<string> {
  const render = await ensureLoaded();
  const svg = await render(code, options);
  const normalized = String(svg || '');
  if (!normalized.includes('<svg')) {
    throw new Error('Mermaid renderer did not return SVG output');
  }
  return inlineMermaidSvg(normalized, options);
}

// ============================================================================
// v0.2 unified engine factory
// ============================================================================

import type { RenderOptions } from '../types.js';
import { DiagramRenderError } from '../types.js';

/** Mermaid engine 的渲染选项。 */
export interface Options extends RenderOptions {
  /** Mermaid 主题（mermaid 自家枚举），默认 'default' */
  mermaidTheme?: 'default' | 'dark' | 'neutral' | 'forest';
  /** 传给 mermaid 的其它 style/theme 变量（如 fontFamily, primaryColor 等） */
  [key: string]: unknown;
}

/**
 * Mermaid engine 工厂。Mermaid 无装配期依赖，`modules` 保留为占位让形态统一。
 *
 * @example
 * ```ts
 * import mermaid from '@supramark/engines/mermaid';
 * const render = mermaid();
 * const svg = await render('graph TD\n  A --> B');
 * ```
 */
export default function mermaid(_modules?: unknown[]) {
  return async (code: string, options?: Options): Promise<string> => {
    options?.signal?.throwIfAborted();
    try {
      return await renderMermaidSvg(code, options as Record<string, unknown> | undefined);
    } catch (e) {
      throw new DiagramRenderError(
        `Mermaid render failed: ${e instanceof Error ? e.message : String(e)}`,
        {
          engine: 'mermaid',
          code: 'render_error',
          input: code.slice(0, 200),
          cause: e,
        }
      );
    }
  };
}
