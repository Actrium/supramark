/**
 * Video React Native renderer
 *
 * Implements the ContainerRNRenderer interface
 *
 * React Native has no built-in video component, so the default renderer shows
 * a poster (or a neutral placeholder) with a play affordance. A host callback
 * owns playback when present; otherwise only http(s) sources are opened through
 * Linking. Hosts that want inline playback can inject their own renderer.
 *
 * @packageDocumentation
 */

import React from 'react';
import {
  View,
  Text,
  Image,
  Pressable,
  Linking,
  StyleSheet,
  type DimensionValue,
  type TextStyle,
  type ViewStyle,
} from 'react-native';
import type { ContainerRNRenderArgs, SupramarkVideoPressEvent } from '@supramark/core';
import { useVideoPressHandler } from '@supramark/rn/video-press';
import type { VideoData } from './feature.js';

const localStyles = StyleSheet.create({
  container: {
    width: '100%',
    marginVertical: 12,
    borderRadius: 8,
    overflow: 'hidden',
  },
  poster: {
    width: '100%',
    aspectRatio: 16 / 9,
  },
  playBadge: {
    ...StyleSheet.absoluteFillObject,
    alignItems: 'center',
    justifyContent: 'center',
    pointerEvents: 'none',
  },
  playBadgeIcon: {
    fontSize: 40,
    color: '#ffffff',
  },
  placeholder: {
    width: '100%',
    aspectRatio: 16 / 9,
    alignItems: 'center',
    justifyContent: 'center',
  },
  placeholderMeta: {
    marginTop: 6,
    fontSize: 13,
  },
  error: {
    borderWidth: 1,
    borderRadius: 8,
    padding: 12,
    marginVertical: 12,
  },
  errorTitle: {
    fontWeight: 'bold',
    marginBottom: 4,
  },
  errorCode: {
    marginTop: 6,
    fontFamily: 'monospace' as const,
    fontSize: 12,
  },
});

/**
 * Clamp the configured width (percent) to a safe RN style value.
 */
function playerWidth(width: number | undefined): DimensionValue | undefined {
  if (typeof width !== 'number' || !Number.isFinite(width) || width <= 0) {
    return undefined;
  }
  return `${Math.min(width, 100)}%`;
}

/** Returns whether the default Linking fallback may open this source. */
function isOpenableVideoUrl(url: string): boolean {
  return /^https?:\/\//i.test(url.trim());
}

/** Accepts image-capable poster schemes and rejects other explicit schemes. */
function safePosterUrl(poster: string | undefined): string | undefined {
  // Missing posters use the neutral placeholder instead.
  if (!poster) return undefined;
  const trimmed = poster.trim();
  // Whitespace-only values are equivalent to no poster.
  if (!trimmed) return undefined;
  // Retain the image-capable schemes supported by React Native hosts.
  if (/^(?:https?:|file:|content:|asset:|ph:|data:image\/)/i.test(trimmed)) return trimmed;
  // Scheme-less values are retained for host-specific asset resolution.
  if (/^[a-z][a-z\d+.-]*:/i.test(trimmed)) return undefined;
  return trimmed;
}

/** Opens an http(s) video in the system player; failures surface via console. */
function openVideo(src: string): void {
  // Prevent opaque cards from dispatching phone, message, or app deep links.
  if (!isOpenableVideoUrl(src)) {
    console.error('Refusing to open non-http(s) video URL:', src);
    return;
  }
  Linking.openURL(src).catch((error: unknown) => {
    console.error('Failed to open video URL:', error);
  });
}

/** Last path segment of the source URL, shown on the no-poster placeholder. */
function videoFileName(src: string): string {
  const segment = src.split('?')[0].split('/').filter(Boolean).pop() ?? src;
  return segment.length > 40 ? `${segment.slice(0, 37)}...` : segment;
}

/** Relative-luminance check used to select a readable error palette. */
function isDarkColor(color: string): boolean {
  const match = /^#(?:[0-9a-f]{3}|[0-9a-f]{6})$/i.exec(color.trim());
  // Unknown color syntaxes fall back to the light palette.
  if (!match) return false;
  let hex = color.trim().slice(1);
  // Expand shorthand colors before computing luminance.
  if (hex.length === 3) {
    hex = hex
      .split('')
      .map(character => character + character)
      .join('');
  }
  const value = Number.parseInt(hex, 16);
  const red = (value >> 16) & 0xff;
  const green = (value >> 8) & 0xff;
  const blue = value & 0xff;
  return 0.299 * red + 0.587 * green + 0.114 * blue < 128;
}

interface VideoPalette {
  background: string;
  icon: string;
  meta: string;
}

/** Derives placeholder colors from styles already resolved from the theme prop. */
function placeholderPalette(styles: Record<string, unknown>): VideoPalette {
  const placeholder = styles.imagePlaceholder as ViewStyle | undefined;
  const placeholderText = styles.imagePlaceholderText as TextStyle | undefined;
  const background =
    typeof placeholder?.backgroundColor === 'string' ? placeholder.backgroundColor : '#f2f2f7';
  const meta = typeof placeholderText?.color === 'string' ? placeholderText.color : '#8e8e93';
  return { background, icon: meta, meta };
}

interface ErrorPalette {
  borderColor: string;
  backgroundColor: string;
  textColor: string;
}

/** Selects an error-card palette that remains readable in the document theme. */
function errorPalette(darkDocument: boolean): ErrorPalette {
  return darkDocument
    ? { borderColor: '#5a2d32', backgroundColor: '#3a2225', textColor: '#e8a1a8' }
    : { borderColor: '#f5c6cb', backgroundColor: '#f8d7da', textColor: '#721c24' };
}

/**
 * RN renderer for :::video (poster + play fallback; see module docs)
 */
export function renderVideoContainerRN({
  node,
  key,
  styles,
}: ContainerRNRenderArgs): React.ReactNode {
  // Defense in depth: the parser filters types, but hosts may supply a hand-built AST.
  const data = (node?.data ?? {}) as unknown as VideoData;
  const src = typeof data.src === 'string' ? data.src : undefined;
  const poster = safePosterUrl(typeof data.poster === 'string' ? data.poster : undefined);
  const title = typeof data.title === 'string' ? data.title : undefined;
  const width = typeof data.width === 'number' ? data.width : undefined;
  // Hand-built AST diagnostics must also stay valid React text children.
  const parseError = typeof data.parseError === 'string' ? data.parseError : undefined;
  const rawConfig = typeof data.rawConfig === 'string' ? data.rawConfig : undefined;

  const palette = placeholderPalette(styles);
  const errorColors = errorPalette(isDarkColor(palette.background));
  const errorStyle = [
    localStyles.error,
    { borderColor: errorColors.borderColor, backgroundColor: errorColors.backgroundColor },
  ];
  const errorTextStyle = { color: errorColors.textColor };

  // Show an error message when parsing failed
  if (parseError) {
    return (
      <View key={key} style={errorStyle}>
        <Text style={[localStyles.errorTitle, errorTextStyle]}>⚠️ Video config error</Text>
        <Text style={errorTextStyle}>{parseError}</Text>
        {rawConfig && <Text style={[localStyles.errorCode, errorTextStyle]}>{rawConfig}</Text>}
      </View>
    );
  }

  // Missing required config
  if (!src) {
    return (
      <View key={key} style={errorStyle}>
        <Text style={[localStyles.errorTitle, errorTextStyle]}>⚠️ Missing src config</Text>
        <Text style={errorTextStyle}>Please specify the src field with the video URL</Text>
      </View>
    );
  }

  return (
    <VideoCard key={key} src={src} poster={poster} title={title} width={width} palette={palette} />
  );
}

/** A real component that can consume the host callback through React context. */
function VideoCard({
  src,
  poster,
  title,
  width,
  palette,
}: {
  src: string;
  poster?: string;
  title?: string;
  width?: number;
  palette: VideoPalette;
}): React.ReactElement {
  const onVideoPress = useVideoPressHandler();
  const widthStyle = playerWidth(width);

  /** Delegates playback to the host, or uses the restricted Linking fallback. */
  const handlePress = (): void => {
    // A host callback always wins over the default external-player behavior.
    if (onVideoPress) {
      const event: SupramarkVideoPressEvent = { src };
      // Omit absent optionals rather than materializing undefined fields.
      if (poster !== undefined) event.poster = poster;
      // Preserve the same exact-optional contract for the accessible title.
      if (title !== undefined) event.title = title;
      onVideoPress(event);
      return;
    }
    openVideo(src);
  };

  return (
    <View style={[localStyles.container, widthStyle ? { width: widthStyle } : undefined]}>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={title ? `Play video: ${title}` : 'Play video'}
        onPress={handlePress}
      >
        {poster ? (
          <View>
            <Image
              source={{ uri: poster }}
              // Neutral underlay so a loading/failed poster shows the placeholder
              // tone instead of a black band.
              style={[localStyles.poster, { backgroundColor: palette.background }]}
              resizeMode="cover"
            />
            {/* Centered play affordance on the poster, matching the placeholder card. */}
            <View style={localStyles.playBadge}>
              <Text style={localStyles.playBadgeIcon}>▶</Text>
            </View>
          </View>
        ) : (
          <View style={[localStyles.placeholder, { backgroundColor: palette.background }]}>
            <Text style={[localStyles.playBadgeIcon, { color: palette.icon }]}>▶</Text>
            <Text style={[localStyles.placeholderMeta, { color: palette.meta }]}>
              {videoFileName(src)}
            </Text>
          </View>
        )}
      </Pressable>
    </View>
  );
}
