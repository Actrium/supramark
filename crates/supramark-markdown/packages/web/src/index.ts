import { parse_json, parse_json_with_options, version } from './wasm/supramark_markdown_web.js';

export { version };

export interface MarkdownParseOptions {
  /** Enable WikiLink parsing (`[[target]]`, `[[target|label]]`, `[[target#section]]`). Default: off. */
  wikilink?: boolean;
}

export function parse(source: string, options?: MarkdownParseOptions): unknown {
  return JSON.parse(parseJson(source, options));
}

export function parseJson(source: string, options?: MarkdownParseOptions): string {
  if (options === undefined) {
    return parse_json(source);
  }
  return parse_json_with_options(source, options);
}

export function parseWithOptions(source: string, options: MarkdownParseOptions): unknown {
  return JSON.parse(parseJson(source, options));
}

export function parseJsonWithOptions(source: string, options: MarkdownParseOptions): string {
  return parse_json_with_options(source, options);
}
