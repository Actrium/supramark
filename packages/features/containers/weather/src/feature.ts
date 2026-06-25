/**
 * Weather Feature 定义
 *
 * 实现 ContainerFeature 接口，提供天气卡片容器。
 *
 * @example
 * ```markdown
 * :::weather json
 * {
 *   "location": "Beijing",
 *   "units": "metric"
 * }
 * :::
 *
 * :::weather yaml
 * location: Tokyo
 * units: imperial
 * :::
 *
 * :::weather toon
 * location: London
 * units: metric
 * :::
 * ```
 *
 * @packageDocumentation
 */

import {
  registerContainerHook,
  extractContainerInnerText,
  type ContainerFeature,
  type ContainerHook,
  type ContainerHookContext,
  type SupramarkContainerNode,
} from '@supramark/core';

// ============================================================================
// 容器名称定义（唯一事实来源）
// ============================================================================

/**
 * Weather 支持的容器名称
 */
export const WEATHER_CONTAINER_NAMES = ['weather'] as const;

export type WeatherContainerName = (typeof WEATHER_CONTAINER_NAMES)[number];

/**
 * 支持的配置格式
 *
 * - json: 标准 JSON 格式
 * - yaml: YAML 格式（默认，最友好）
 * - toon: 紧凑表格式格式，如 `key[n]{fields}: val1,val2,...`
 */
export type WeatherConfigFormat = 'json' | 'yaml' | 'toon';

/**
 * Weather 节点数据结构
 */
export interface WeatherData {
  /** 配置格式 */
  format: WeatherConfigFormat;
  /** 位置/城市 */
  location?: string;
  /** 温度单位: metric(摄氏) / imperial(华氏) */
  units?: 'metric' | 'imperial';
  /** 是否显示预报 */
  showForecast?: boolean;
  /** 天数（预报） */
  days?: number;
  /** 原始配置文本（解析失败时保留） */
  rawConfig?: string;
  /** 解析错误信息 */
  parseError?: string;
}

// ============================================================================
// 配置解析
// ============================================================================

/**
 * 解析 JSON 配置
 */
function parseJsonConfig(content: string): Partial<WeatherData> {
  try {
    const obj = JSON.parse(content) as Record<string, unknown>;
    return {
      location: obj.location as string | undefined,
      units: obj.units as 'metric' | 'imperial' | undefined,
      showForecast: (obj.showForecast ?? obj.show_forecast) as boolean | undefined,
      days: obj.days as number | undefined,
    };
  } catch (e) {
    return { parseError: `JSON 解析错误: ${(e as Error).message}` };
  }
}

/**
 * 解析 YAML 配置（简单实现，支持基本 key: value 格式）
 */
function parseYamlConfig(content: string): Partial<WeatherData> {
  try {
    const result: Record<string, unknown> = {};
    for (const line of content.split('\n')) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith('#')) continue;

      const match = trimmed.match(/^([\w_]+):\s*(.*)$/);
      if (match) {
        const [, key, rawValue] = match;
        let value: unknown = rawValue;

        // 类型转换
        if (rawValue === 'true') value = true;
        else if (rawValue === 'false') value = false;
        else if (/^-?\d+$/.test(rawValue)) value = parseInt(rawValue, 10);
        else if (/^-?\d+\.\d+$/.test(rawValue)) value = parseFloat(rawValue);
        else if ((rawValue.startsWith('"') && rawValue.endsWith('"')) ||
                 (rawValue.startsWith("'") && rawValue.endsWith("'"))) {
          value = rawValue.slice(1, -1);
        }

        result[key] = value;
      }
    }

    return {
      location: result.location as string | undefined,
      units: result.units as 'metric' | 'imperial' | undefined,
      showForecast: (result.showForecast ?? result.show_forecast) as boolean | undefined,
      days: result.days as number | undefined,
    };
  } catch (e) {
    return { parseError: `YAML 解析错误: ${(e as Error).message}` };
  }
}

/**
 * 解析 TOON 配置
 *
 * TOON 是一种紧凑的表格式数据格式：
 * - 简单 key:value 格式（每行一个）
 * - 数组格式: `key[count]{field1,field2}: val1,val2`
 *
 * @example
 * ```
 * location: Beijing
 * units: metric
 * ```
 *
 * @example
 * ```
 * forecast[3]{day,high,low}:
 *   Mon,25,18
 *   Tue,27,19
 *   Wed,24,17
 * ```
 */
function parseToonConfig(content: string): Partial<WeatherData> {
  try {
    const result: Record<string, unknown> = {};
    const lines = content.split('\n');
    let i = 0;

    while (i < lines.length) {
      const line = lines[i].trim();
      i++;

      if (!line || line.startsWith('#')) continue;

      // 尝试匹配数组格式: key[count]{fields}:
      const arrayMatch = line.match(/^([\w_]+)\[(\d+)\]\{([^}]+)\}:\s*$/);
      if (arrayMatch) {
        const [, key, countStr, fieldsStr] = arrayMatch;
        const count = parseInt(countStr, 10);
        const fields = fieldsStr.split(',').map(f => f.trim());
        const items: Record<string, unknown>[] = [];

        // 读取接下来的 count 行数据
        for (let j = 0; j < count && i < lines.length; j++) {
          const dataLine = lines[i].trim();
          i++;
          if (!dataLine) {
            j--; // 跳过空行
            continue;
          }

          const values = dataLine.split(',').map(v => v.trim());
          const item: Record<string, unknown> = {};
          fields.forEach((field, idx) => {
            let val: unknown = values[idx] ?? '';
            // 类型转换
            if (val === 'true') val = true;
            else if (val === 'false') val = false;
            else if (typeof val === 'string' && /^-?\d+$/.test(val)) val = parseInt(val, 10);
            else if (typeof val === 'string' && /^-?\d+\.\d+$/.test(val)) val = parseFloat(val);
            item[field] = val;
          });
          items.push(item);
        }

        result[key] = items;
        continue;
      }

      // 简单 key: value 格式
      const kvMatch = line.match(/^([\w_]+):\s*(.*)$/);
      if (kvMatch) {
        const [, key, rawValue] = kvMatch;
        let value: unknown = rawValue;

        // 类型转换
        if (rawValue === 'true') value = true;
        else if (rawValue === 'false') value = false;
        else if (/^-?\d+$/.test(rawValue)) value = parseInt(rawValue, 10);
        else if (/^-?\d+\.\d+$/.test(rawValue)) value = parseFloat(rawValue);

        result[key] = value;
      }
    }

    return {
      location: result.location as string | undefined,
      units: result.units as 'metric' | 'imperial' | undefined,
      showForecast: (result.showForecast ?? result.show_forecast) as boolean | undefined,
      days: result.days as number | undefined,
    };
  } catch (e) {
    return { parseError: `TOON 解析错误: ${(e as Error).message}` };
  }
}

/**
 * 根据格式解析配置内容
 */
function parseConfig(content: string, format: WeatherConfigFormat): Partial<WeatherData> {
  switch (format) {
    case 'json':
      return parseJsonConfig(content);
    case 'yaml':
      return parseYamlConfig(content);
    case 'toon':
      return parseToonConfig(content);
    default:
      return { parseError: `Unsupported format: ${String(format)}` };
  }
}

/**
 * 从 token.info 解析格式参数
 */
function parseFormat(info: string): WeatherConfigFormat {
  const parts = (info || '').trim().split(/\s+/).filter(Boolean);
  if (parts.length > 1) {
    const format = parts[1].toLowerCase();
    if (format === 'json' || format === 'yaml' || format === 'toon') {
      return format;
    }
  }
  // 默认 yaml（最友好）
  return 'yaml';
}

// ============================================================================
// 解析逻辑
// ============================================================================

function createWeatherContainerHook(name: string): ContainerHook {
  return {
    name,
    opaque: true,
    onOpen(ctx: ContainerHookContext) {
      const { token, stack, sourceLines } = ctx;
      const format = parseFormat(token.info || '');

      // 提取容器内容
      const innerText = extractContainerInnerText(token, sourceLines);

      // 解析配置
      const parsed = parseConfig(innerText, format);

      const data: WeatherData = {
        format,
        location: parsed.location,
        units: parsed.units,
        showForecast: parsed.showForecast,
        days: parsed.days,
        parseError: parsed.parseError,
        rawConfig: parsed.parseError ? innerText : undefined,
      };

      const node: SupramarkContainerNode = {
        type: 'container' as const,
        name: 'weather',
        params: token.info ? String(token.info) : undefined,
        data: { ...data },
        children: [],
      };

      const parent = stack[stack.length - 1];
      parent.children.push(node);
      stack.push(node);
    },
    onClose(ctx: ContainerHookContext) {
      const top = ctx.stack[ctx.stack.length - 1] as SupramarkContainerNode;
      if (top && top.type === 'container' && top.name === 'weather') {
        ctx.stack.pop();
      }
    },
  };
}

/**
 * 注册 Weather 解析器
 */
function registerWeatherParser(): void {
  for (const name of WEATHER_CONTAINER_NAMES) {
    registerContainerHook(createWeatherContainerHook(name));
  }
}

// ============================================================================
// Feature 定义（实现 ContainerFeature 接口）
// ============================================================================

/**
 * Weather Feature
 *
 * 天气卡片容器，支持 JSON/YAML/TOML 配置格式
 */
export const weatherFeature: ContainerFeature = {
  id: '@supramark/feature-weather',
  name: 'Weather',
  version: '0.1.0',
  description: '天气卡片容器，支持 JSON/YAML/TOML 配置格式',

  containerNames: [...WEATHER_CONTAINER_NAMES],

  registerParser: registerWeatherParser,

  webRendererExport: 'renderWeatherContainerWeb',
  rnRendererExport: 'renderWeatherContainerRN',
};
