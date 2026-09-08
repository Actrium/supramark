/**
 * Video web renderer
 *
 * Implements the ContainerWebRenderer interface
 *
 * @packageDocumentation
 */

import React from 'react';
import type { ContainerWebRenderArgs } from '@supramark/core';
import type { VideoData } from './feature.js';

const styles: Record<string, React.CSSProperties> = {
  container: {
    width: '100%',
    margin: '12px 0',
  },
  video: {
    display: 'block',
    width: '100%',
    borderRadius: '8px',
    backgroundColor: '#000',
  },
  error: {
    border: '1px solid #f5c6cb',
    backgroundColor: '#f8d7da',
    color: '#721c24',
    borderRadius: '8px',
    padding: '12px 16px',
    margin: '12px 0',
  },
  errorTitle: {
    fontWeight: 'bold',
    marginBottom: '4px',
  },
  errorCode: {
    marginTop: '6px',
    fontFamily: 'monospace',
    fontSize: '12px',
    whiteSpace: 'pre-wrap' as const,
  },
};

/**
 * Clamp the configured width (percent) to a safe CSS value.
 */
function playerWidth(width: number | undefined): string {
  if (typeof width !== 'number' || !Number.isFinite(width) || width <= 0) {
    return '100%';
  }
  return `${Math.min(width, 100)}%`;
}

/** Accepts browser-safe poster sources while preserving relative media URLs. */
function safePosterUrl(poster: string | undefined): string | undefined {
  // Missing posters activate the metadata preload path.
  if (!poster) return undefined;
  const trimmed = poster.trim();
  // Whitespace-only values are equivalent to no poster.
  if (!trimmed) return undefined;
  // These explicit schemes are valid image sources in supported browsers.
  if (/^(?:https?:|blob:|data:image\/)/i.test(trimmed)) return trimmed;
  // Any other explicit URI scheme is rejected; scheme-less values are relative URLs.
  if (/^[a-z][a-z\d+.-]*:/i.test(trimmed)) return undefined;
  return trimmed;
}

/**
 * Web renderer for :::video
 */
export function renderVideoContainerWeb({ node, key }: ContainerWebRenderArgs): React.ReactNode {
  // Defense in depth: the parser filters types, but hosts may supply a hand-built AST.
  const data = (node?.data ?? {}) as unknown as VideoData;
  const src = typeof data.src === 'string' ? data.src : undefined;
  const poster = safePosterUrl(typeof data.poster === 'string' ? data.poster : undefined);
  const title = typeof data.title === 'string' ? data.title : undefined;
  const width = typeof data.width === 'number' ? data.width : undefined;
  const autoplay = typeof data.autoplay === 'boolean' ? data.autoplay : false;
  const loop = typeof data.loop === 'boolean' ? data.loop : false;
  const muted = typeof data.muted === 'boolean' ? data.muted : false;
  const controls = typeof data.controls === 'boolean' ? data.controls : true;
  // Hand-built AST diagnostics must also stay valid React text children.
  const parseError = typeof data.parseError === 'string' ? data.parseError : undefined;
  const rawConfig = typeof data.rawConfig === 'string' ? data.rawConfig : undefined;

  // Show an error message when parsing failed
  if (parseError) {
    return (
      <div key={key} style={styles.error}>
        <div style={styles.errorTitle}>⚠️ Video config error</div>
        <div>{parseError}</div>
        {rawConfig && <pre style={styles.errorCode}>{rawConfig}</pre>}
      </div>
    );
  }

  // Missing required config
  if (!src) {
    return (
      <div key={key} style={styles.error}>
        <div style={styles.errorTitle}>⚠️ Missing src config</div>
        <div>Please specify the src field with the video URL</div>
      </div>
    );
  }

  return (
    <div key={key} style={{ ...styles.container, width: playerWidth(width) }}>
      <video
        style={styles.video}
        src={src}
        poster={poster}
        // Without a poster, preload metadata so the browser renders the first
        // frame instead of an empty black rectangle.
        preload={poster ? undefined : 'metadata'}
        controls={controls}
        autoPlay={autoplay}
        loop={loop}
        muted={muted}
        aria-label={title}
      />
    </div>
  );
}
