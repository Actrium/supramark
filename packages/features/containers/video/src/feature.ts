/**
 * Video Feature definition
 *
 * Implements the ContainerFeature interface, embedding a playable video via
 * a :::video container with a JSON config body.
 *
 * The JSON body is parsed into structured `data` by the Rust parser
 * (`parse_video_data` in crates/supramark-markdown). The TS hook below keeps
 * the ContainerFeature contract complete and mirrors that mapping for the
 * registry/hook path.
 *
 * @example
 * ```markdown
 * :::video
 * {
 *   "src": "https://example.com/demo.mp4",
 *   "poster": "https://example.com/cover.jpg",
 *   "title": "Product demo"
 * }
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
// Container name definition (single source of truth)
// ============================================================================

/**
 * Container names supported by Video
 */
export const VIDEO_CONTAINER_NAMES = ['video'] as const;

export type VideoContainerName = (typeof VIDEO_CONTAINER_NAMES)[number];

/**
 * Video node data structure
 *
 * Populated by the Rust parser; this interface must stay in sync with
 * `parse_video_data` in crates/supramark-markdown/src/supramark.rs.
 */
export interface VideoData {
  /** Video source URL (required for rendering) */
  src?: string;
  /** Poster / thumbnail image URL */
  poster?: string;
  /** Accessible title / caption */
  title?: string;
  /** Autoplay on mount (browsers usually require muted as well) */
  autoplay?: boolean;
  /** Loop playback */
  loop?: boolean;
  /** Start muted */
  muted?: boolean;
  /** Show native playback controls (defaults to true in renderers) */
  controls?: boolean;
  /** Player width as a percentage of the container (1-100) */
  width?: number;
  /** Parse error message (kept when the JSON body is invalid) */
  parseError?: string;
  /** Raw config text (kept when parsing fails) */
  rawConfig?: string;
}

/** Fields copied from the JSON config body into VideoData. */
const VIDEO_CONFIG_FIELDS = [
  'src',
  'poster',
  'title',
  'autoplay',
  'loop',
  'muted',
  'controls',
  'width',
] as const;

/**
 * Parse a JSON config body into VideoData
 *
 * Mirrors the Rust `parse_video_data` mapping: unknown fields are dropped,
 * and invalid JSON yields parseError + rawConfig instead of throwing.
 */
function parseVideoConfig(content: string): Partial<VideoData> {
  try {
    const parsed: unknown = JSON.parse(content.trim());
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      return { parseError: 'video JSON config must be an object', rawConfig: content };
    }
    const source = parsed as Record<string, unknown>;
    const result: Record<string, unknown> = {};
    for (const field of VIDEO_CONFIG_FIELDS) {
      const value = source[field];
      if (value !== undefined && value !== null) {
        result[field] = value;
      }
    }
    return result as Partial<VideoData>;
  } catch (e) {
    return { parseError: `JSON parse error: ${(e as Error).message}`, rawConfig: content };
  }
}

// ============================================================================
// Parsing logic
// ============================================================================

function createVideoContainerHook(name: string): ContainerHook {
  return {
    name,
    opaque: true,
    onOpen(ctx: ContainerHookContext) {
      const { token, stack, sourceLines } = ctx;

      const innerText = extractContainerInnerText(token, sourceLines);
      const data: VideoData = { ...parseVideoConfig(innerText) };

      const node: SupramarkContainerNode = {
        type: 'container' as const,
        name: 'video',
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
      if (top && top.type === 'container' && top.name === 'video') {
        ctx.stack.pop();
      }
    },
  };
}

/**
 * Register the Video parser
 */
function registerVideoParser(): void {
  for (const name of VIDEO_CONTAINER_NAMES) {
    registerContainerHook(createVideoContainerHook(name));
  }
}

// ============================================================================
// Feature definition (implements the ContainerFeature interface)
// ============================================================================

/**
 * Video Feature
 *
 * A video embed container with a JSON config body
 */
export const videoFeature: ContainerFeature = {
  id: '@supramark/feature-video',
  name: 'Video',
  version: '0.1.0',
  description: 'A video embed container configured by a JSON body',

  containerNames: [...VIDEO_CONTAINER_NAMES],

  registerParser: registerVideoParser,

  webRendererExport: 'renderVideoContainerWeb',
  rnRendererExport: 'renderVideoContainerRN',
};
