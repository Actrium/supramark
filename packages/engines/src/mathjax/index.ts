import { mathjax } from 'mathjax-full/js/mathjax.js';
import { TeX } from 'mathjax-full/js/input/tex.js';
import { SVG } from 'mathjax-full/js/output/svg.js';
import { liteAdaptor } from 'mathjax-full/js/adaptors/liteAdaptor.js';
import { RegisterHTMLHandler } from 'mathjax-full/js/handlers/html.js';
import { AllPackages } from 'mathjax-full/js/input/tex/AllPackages.js';

type MathJaxDocumentLike = ReturnType<typeof mathjax.document>;

interface MathJaxRenderer {
  adaptor: ReturnType<typeof liteAdaptor>;
  document: MathJaxDocumentLike;
}

let rendererPromise: Promise<MathJaxRenderer> | null = null;

async function ensureRenderer(): Promise<MathJaxRenderer> {
  if (rendererPromise) {
    return rendererPromise;
  }

  rendererPromise = Promise.resolve().then(() => {
    const adaptor = liteAdaptor();
    RegisterHTMLHandler(adaptor);

    const tex = new TeX({ packages: AllPackages });
    const svg = new SVG({ fontCache: 'none' });
    const document = mathjax.document('', {
      InputJax: tex,
      OutputJax: svg,
    });

    return { adaptor, document };
  });

  return rendererPromise;
}

function extractSvg(adaptor: MathJaxRenderer['adaptor'], node: unknown): string {
  type AdaptorNode = Parameters<MathJaxRenderer['adaptor']['outerHTML']>[0];
  // Required by the full project-references build (where `outerHTML`'s param
  // resolves to MathJax's `LiteElement`); the isolated type-aware lint pass
  // sees the adaptor as `any` and flags these as unnecessary.
  // eslint-disable-next-line @typescript-eslint/no-unnecessary-type-assertion
  const adaptorNode = node as AdaptorNode;
  const svgNode =
    adaptorNode && typeof adaptor.firstChild === 'function' ? adaptor.firstChild(adaptorNode) : null;

  // eslint-disable-next-line @typescript-eslint/no-unnecessary-type-assertion
  const target = (svgNode ?? adaptorNode) as AdaptorNode;
  const svg = adaptor.outerHTML(target);
  if (!svg || !svg.includes('<svg')) {
    throw new Error('MathJax did not produce SVG output');
  }

  return svg.replace(/<mjx-container[^>]*>/g, '').replace(/<\/mjx-container>/g, '');
}

function normalizeMathSvg(svg: string): string {
  let normalized = svg.replace(
    /<rect\b([^>]*)\s*(?:\/>|><\/rect>)/gi,
    (match, rawAttrs) => {
      const attrs = String(rawAttrs).replace(/\s+/g, ' ').trim();
      const xMatch = attrs.match(/\bx="([^"]+)"/i);
      const yMatch = attrs.match(/\by="([^"]+)"/i);
      const widthMatch = attrs.match(/\bwidth="([^"]+)"/i);
      const heightMatch = attrs.match(/\bheight="([^"]+)"/i);
      if (!xMatch || !yMatch || !widthMatch || !heightMatch) {
        return match;
      }

      const x = xMatch[1];
      const y = yMatch[1];
      const width = widthMatch[1];
      const height = heightMatch[1];
      const xNum = parseFloat(x);
      const yNum = parseFloat(y);
      const widthNum = parseFloat(width);
      const heightNum = parseFloat(height);
      if (
        !Number.isFinite(xNum) ||
        !Number.isFinite(yNum) ||
        !Number.isFinite(widthNum) ||
        !Number.isFinite(heightNum)
      ) {
        return match;
      }

      const d = [
        `M ${xNum} ${yNum}`,
        `L ${xNum + widthNum} ${yNum}`,
        `L ${xNum + widthNum} ${yNum + heightNum}`,
        `L ${xNum} ${yNum + heightNum}`,
        'Z',
      ].join(' ');

      const passthroughAttrs = attrs
        .replace(/\bx="[^"]*"/gi, '')
        .replace(/\by="[^"]*"/gi, '')
        .replace(/\bwidth="[^"]*"/gi, '')
        .replace(/\bheight="[^"]*"/gi, '')
        .replace(/\s+/g, ' ')
        .trim();

      return `<path${passthroughAttrs ? ` ${passthroughAttrs}` : ''} d="${d}"></path>`;
    }
  );

  normalized = normalized.replace(
    /<svg\b((?=[^>]*\b(?:x|y)=")[^>]*)>([\s\S]*?)<\/svg>/gi,
    (_match, rawAttrs, inner) => {
      const attrs = String(rawAttrs);
      const x = parseFloat(attrs.match(/\bx="([^"]+)"/i)?.[1] ?? '0');
      const y = parseFloat(attrs.match(/\by="([^"]+)"/i)?.[1] ?? '0');
      const width = parseFloat(attrs.match(/\bwidth="([^"]+)"/i)?.[1] ?? '0');
      const height = parseFloat(attrs.match(/\bheight="([^"]+)"/i)?.[1] ?? '0');
      const viewBox = attrs.match(/\bviewBox="([^"]+)"/i)?.[1] ?? '';
      const viewBoxParts = viewBox.trim().split(/[\s,]+/).map(Number);

      const hasViewBox = viewBoxParts.length === 4 && viewBoxParts.every(value => Number.isFinite(value));

      const transformParts: string[] = [];
      if (Number.isFinite(x) || Number.isFinite(y)) {
        transformParts.push(
          `translate(${Number.isFinite(x) ? x : 0}, ${Number.isFinite(y) ? y : 0})`
        );
      }

      if (hasViewBox) {
        const [, , vbWidth, vbHeight] = viewBoxParts;
        if (vbWidth > 0 && vbHeight > 0 && width > 0 && height > 0) {
          const sx = width / vbWidth;
          const sy = height / vbHeight;
          if (Math.abs(sx - 1) > 1e-6 || Math.abs(sy - 1) > 1e-6) {
            transformParts.push(`scale(${sx}, ${sy})`);
          }
        }

        const [vbX, vbY] = viewBoxParts;
        if (vbX !== 0 || vbY !== 0) {
          transformParts.push(`translate(${-vbX}, ${-vbY})`);
        }
      }

      const transformAttr = transformParts.length > 0
        ? ` transform="${transformParts.join(' ')}"`
        : '';
      return `<g${transformAttr}>${inner}</g>`;
    }
  );

  const rootSvgMatch = normalized.match(/^<svg\b([^>]*)>/i);
  if (!rootSvgMatch) {
    return normalized;
  }

  const rootAttrs = rootSvgMatch[1];
  const viewBoxMatch = rootAttrs.match(/\bviewBox="([^"]+)"/i);
  const widthMatch = rootAttrs.match(/\bwidth="([^"]+)"/i);
  const heightMatch = rootAttrs.match(/\bheight="([^"]+)"/i);

  if (viewBoxMatch) {
    return normalized.replace(/^<svg\b([^>]*)>/i, (_match, attrs) => {
      const cleanedAttrs = String(attrs)
        .replace(/\swidth="[^"]*"/i, '')
        .replace(/\sheight="[^"]*"/i, '');
      return `<svg${cleanedAttrs}>`;
    });
  }

  if (!widthMatch || !heightMatch) {
    return normalized;
  }

  const width = parseFloat(widthMatch[1]);
  const height = parseFloat(heightMatch[1]);
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return normalized.replace(/^<svg\b([^>]*)>/i, (_match, attrs) => {
      const cleanedAttrs = String(attrs)
        .replace(/\swidth="[^"]*"/i, '')
        .replace(/\sheight="[^"]*"/i, '');
      return `<svg${cleanedAttrs}>`;
    });
  }

  return normalized.replace(/^<svg\b([^>]*)>/i, (_match, attrs) => {
    const cleanedAttrs = String(attrs)
      .replace(/\swidth="[^"]*"/i, '')
      .replace(/\sheight="[^"]*"/i, '')
      .replace(/\sviewBox="[^"]*"/i, '');
    return `<svg${cleanedAttrs} viewBox="0 0 ${width} ${height}">`;
  });
}

export async function renderMathJaxSvg(
  code: string,
  options?: { displayMode?: boolean }
): Promise<string> {
  const { adaptor, document } = await ensureRenderer();
  const node = document.convert(code, {
    display: options?.displayMode === true,
  });
  return normalizeMathSvg(extractSvg(adaptor, node));
}

export function getSvgViewBoxSize(svg: string): { width: number; height: number } | null {
  const viewBoxMatch = svg.match(/viewBox="([^"]+)"/);
  if (!viewBoxMatch) {
    return null;
  }

  const parts = viewBoxMatch[1].trim().split(/[\s,]+/);
  if (parts.length !== 4) {
    return null;
  }

  const width = parseFloat(parts[2]);
  const height = parseFloat(parts[3]);
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return null;
  }

  return { width, height };
}

// ============================================================================
// v0.2 unified engine factory
// ============================================================================

import type { RenderOptions } from '../types.js';
import { DiagramRenderError } from '../types.js';

/** MathJax engine 的渲染选项。 */
export interface Options extends RenderOptions {
  /** 是否为块级（$$…$$），默认 false（行内 $…$） */
  displayMode?: boolean;
}

/**
 * MathJax engine 工厂（用于 math_inline / math_block 节点）。
 *
 * @example
 * ```ts
 * import mathjax from '@supramark/engines/mathjax';
 * const render = mathjax();
 * const svg = await render('E = mc^2', { displayMode: false });
 * ```
 */
function mathjaxFactory(_modules?: unknown[]) {
  return async (code: string, options?: Options): Promise<string> => {
    options?.signal?.throwIfAborted();
    try {
      return await renderMathJaxSvg(code, { displayMode: options?.displayMode });
    } catch (e) {
      throw new DiagramRenderError(
        `MathJax render failed: ${e instanceof Error ? e.message : String(e)}`,
        {
          engine: 'math',
          code: 'render_error',
          input: code.slice(0, 200),
          cause: e,
        }
      );
    }
  };
}

export default mathjaxFactory;
