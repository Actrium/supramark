/**
 * Video React Native renderer
 *
 * Implements the ContainerRNRenderer interface
 *
 * React Native has no built-in video component, so the default renderer shows
 * a poster (or a neutral placeholder) with a play affordance that opens the
 * source in the system player via Linking. Hosts that want inline playback
 * (react-native-video / expo-av) can pass their own renderer through the
 * Supramark `containerRenderers` prop instead of this one.
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
} from 'react-native';
import type { ContainerRNRenderArgs } from '@supramark/core';
import type { VideoData } from './feature.js';

const localStyles = StyleSheet.create({
  container: {
    width: '100%',
    marginVertical: 12,
    borderRadius: 8,
    overflow: 'hidden',
    backgroundColor: '#000',
  },
  poster: {
    width: '100%',
    aspectRatio: 16 / 9,
  },
  placeholder: {
    width: '100%',
    aspectRatio: 16 / 9,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: '#1c1c1e',
  },
  playIcon: {
    fontSize: 40,
    color: '#ffffff',
  },
  caption: {
    paddingVertical: 6,
    paddingHorizontal: 12,
    fontSize: 14,
    color: '#555',
    textAlign: 'center',
    backgroundColor: '#ffffff',
  },
  error: {
    borderWidth: 1,
    borderColor: '#f5c6cb',
    backgroundColor: '#f8d7da',
    borderRadius: 8,
    padding: 12,
    marginVertical: 12,
  },
  errorTitle: {
    fontWeight: 'bold',
    color: '#721c24',
    marginBottom: 4,
  },
  errorText: {
    color: '#721c24',
  },
  errorCode: {
    marginTop: 6,
    fontFamily: 'monospace' as const,
    fontSize: 12,
    color: '#721c24',
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

/** Opens the video source in the system player; failures surface via console. */
function openVideo(src: string): void {
  Linking.openURL(src).catch((error: unknown) => {
    console.error('Failed to open video URL:', error);
  });
}

/**
 * RN renderer for :::video (poster + play fallback; see module docs)
 */
export function renderVideoContainerRN({ node, key }: ContainerRNRenderArgs): React.ReactNode {
  const data = (node?.data ?? {}) as unknown as VideoData;
  const { parseError, rawConfig, src, poster, title, width } = data;

  // Show an error message when parsing failed
  if (parseError) {
    return (
      <View key={key} style={localStyles.error}>
        <Text style={localStyles.errorTitle}>⚠️ Video config error</Text>
        <Text style={localStyles.errorText}>{parseError}</Text>
        {rawConfig && <Text style={localStyles.errorCode}>{rawConfig}</Text>}
      </View>
    );
  }

  // Missing required config
  if (!src) {
    return (
      <View key={key} style={localStyles.error}>
        <Text style={localStyles.errorTitle}>⚠️ Missing src config</Text>
        <Text style={localStyles.errorText}>Please specify the src field with the video URL</Text>
      </View>
    );
  }

  const widthStyle = playerWidth(width);

  return (
    <View
      key={key}
      style={widthStyle ? { ...localStyles.container, width: widthStyle } : localStyles.container}
    >
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={title ?? `Play video: ${src}`}
        onPress={() => openVideo(src)}
      >
        {poster ? (
          <Image source={{ uri: poster }} style={localStyles.poster} resizeMode="cover" />
        ) : (
          <View style={localStyles.placeholder}>
            <Text style={localStyles.playIcon}>▶</Text>
          </View>
        )}
      </Pressable>
      {title && <Text style={localStyles.caption}>{title}</Text>}
    </View>
  );
}
