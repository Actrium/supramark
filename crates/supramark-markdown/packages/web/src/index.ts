import { parse_json, version } from './wasm/supramark_markdown_web.js';

export { version };

export function parse(source: string): unknown {
  return JSON.parse(parse_json(source));
}

export function parseJson(source: string): string {
  return parse_json(source);
}
