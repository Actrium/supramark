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

/**
 * Web renderer for :::video
 */
export function renderVideoContainerWeb({ node, key }: ContainerWebRenderArgs): React.ReactNode {
  // The JSON body is user input: Rust copies fields verbatim without type
  // validation, so guard every field before use — a non-string src must
  // degrade to an error card instead of crashing the whole document.
  const data = (node?.data ?? {}) as unknown as VideoData;
  const src = typeof data.src === 'string' ? data.src : undefined;
  const poster = typeof data.poster === 'string' ? data.poster : undefined;
  const title = typeof data.title === 'string' ? data.title : undefined;
  const width = typeof data.width === 'number' ? data.width : undefined;
  const { parseError, rawConfig, autoplay, loop, muted, controls } = data;

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
        controls={controls ?? true}
        autoPlay={autoplay ?? false}
        loop={loop ?? false}
        muted={muted ?? false}
        aria-label={title}
      />
    </div>
  );
}
