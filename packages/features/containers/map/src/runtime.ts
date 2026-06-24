import type { ContainerHookContext } from '@supramark/core';
import { registerContainerHook } from '@supramark/core';
import type { SupramarkContainerNode } from '@supramark/core';

interface ParsedMapConfig {
  center?: [number, number];
  zoom?: number;
  marker?: { lat?: number; lng?: number };
  meta?: Record<string, unknown>;
}

function parseTuple2(valueRaw: string): [number, number] | undefined {
  const trimmed = valueRaw.trim();
  const withoutBrackets = trimmed.replace(/^\[/, '').replace(/\]$/, '');
  const parts = withoutBrackets.split(',').map((p) => p.trim()).filter(Boolean);
  if (parts.length !== 2) return undefined;
  const a = Number.parseFloat(parts[0]);
  const b = Number.parseFloat(parts[1]);
  if (Number.isNaN(a) || Number.isNaN(b)) return undefined;
  return [a, b];
}

/**
 * 解析 :::map 容器内部的配置文本。
 *
 * 当前版本实现的是一个"贴近 YAML 但更宽容"的迷你语法：
 *
 * ```text
 * center: [34.05, -118.24]
 * zoom: 12
 * marker:
 *   lat: 34.05
 *   lng: -118.24
 * ```
 */
function parseMapConfig(raw: string): ParsedMapConfig {
  const lines = raw.split(/\r?\n/);
  const result: ParsedMapConfig = {};
  let currentSection: 'root' | 'marker' = 'root';

  for (const line of lines) {
    if (!line.trim()) continue;

    const indentMatch = /^(\s*)/.exec(line);
    const indent = indentMatch ? indentMatch[1].length : 0;
    const trimmed = line.trim();

    if (indent === 0) {
      const idx = trimmed.indexOf(':');
      if (idx === -1) continue;
      const key = trimmed.slice(0, idx).trim();
      const valueRaw = trimmed.slice(idx + 1).trim();

      switch (key) {
        case 'center': {
          const center = parseTuple2(valueRaw);
          if (center) {
            result.center = center;
          }
          currentSection = 'root';
          break;
        }
        case 'zoom': {
          const zoom = Number.parseFloat(valueRaw);
          if (!Number.isNaN(zoom)) {
            result.zoom = zoom;
          }
          currentSection = 'root';
          break;
        }
        case 'marker': {
          if (!result.marker) {
            result.marker = {};
          }
          currentSection = 'marker';
          break;
        }
        default: {
          if (!result.meta) result.meta = {};
          result.meta[key] = valueRaw;
          currentSection = 'root';
        }
      }
    } else if (currentSection === 'marker') {
      const idx = trimmed.indexOf(':');
      if (idx === -1) continue;
      const key = trimmed.slice(0, idx).trim();
      const valueRaw = trimmed.slice(idx + 1).trim();
      const num = Number.parseFloat(valueRaw);
      if (Number.isNaN(num)) continue;

      if (!result.marker) result.marker = {};
      if (key === 'lat' || key === 'lng') {
        result.marker[key] = num;
      } else {
        if (!result.meta) result.meta = {};
        result.meta[`marker.${key}`] = num;
      }
    }
  }

  return result;
}

function extractInnerText(ctx: ContainerHookContext): string {
  const { token, sourceLines } = ctx;
  if (!token.map || token.map.length !== 2) return '';
  const [start, end] = token.map;
  const innerStart = start + 1;
  const innerEnd = end - 1 > innerStart ? end - 1 : end;
  return sourceLines.slice(innerStart, innerEnd).join('\n');
}

// 注册 Map 容器 hook：
// - name: 'map'
// - opaque: true（容器内部 token 不再进入默认 AST 构建流程）
registerContainerHook({
  name: 'map',
  opaque: true,
  onOpen(ctx: ContainerHookContext) {
    const raw = extractInnerText(ctx);
    const parsed = parseMapConfig(raw);
    const marker =
      parsed.marker &&
      typeof parsed.marker.lat === 'number' &&
      typeof parsed.marker.lng === 'number'
        ? { lat: parsed.marker.lat, lng: parsed.marker.lng }
        : undefined;
    const resolvedCenter =
      parsed.center ??
      (marker ? [marker.lat, marker.lng] : undefined) ??
      [0, 0];

    const mapNode: SupramarkContainerNode = {
      type: 'container',
      name: 'map',
      mode: 'opaque',
      value: raw,
      data: {
        center: resolvedCenter,
        zoom: parsed.zoom,
        markers: marker ? [marker] : undefined,
        meta: parsed.meta,
      },
      children: [],
    };

    const parentForMap = ctx.stack[ctx.stack.length - 1];
    parentForMap.children.push(mapNode);
  },
});
