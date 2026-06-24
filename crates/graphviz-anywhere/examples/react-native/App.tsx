/*
 * Example React Native app demonstrating the Graphviz native module.
 *
 * Renders a DOT graph to SVG and displays it inline using react-native-svg.
 * Allows switching between layout engines and editing the DOT source.
 *
 * Licensed under the Apache License, Version 2.0
 */

import React, { useState, useCallback, useEffect } from 'react';
import {
  SafeAreaView,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  TouchableOpacity,
  View,
  ActivityIndicator,
} from 'react-native';
import { SvgXml } from 'react-native-svg';
import {
  renderDot,
  getVersion,
  GraphvizErrorCode,
} from '@actrium/graphviz-anywhere-rn';
import type { GraphvizEngine } from '@actrium/graphviz-anywhere-rn';

const DEFAULT_DOT = `digraph G {
  rankdir=LR;
  node [shape=box, style=filled, fillcolor="#e8f0fe", fontname="Helvetica"];
  edge [color="#5f6368"];

  Start -> Parse -> Layout -> Render -> Output;
  Parse -> Error [color=red, style=dashed];
  Layout -> Error [color=red, style=dashed];

  Start [fillcolor="#34a853", fontcolor=white];
  Output [fillcolor="#4285f4", fontcolor=white];
  Error [fillcolor="#ea4335", fontcolor=white, shape=octagon];
}`;

const ENGINES: GraphvizEngine[] = [
  'dot', 'neato', 'fdp', 'sfdp', 'circo', 'twopi', 'osage', 'patchwork',
];

export default function App() {
  const [dotSource, setDotSource] = useState(DEFAULT_DOT);
  const [svgOutput, setSvgOutput] = useState<string | null>(null);
  const [engine, setEngine] = useState<GraphvizEngine>('dot');
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [version, setVersion] = useState<string>('');

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion('unknown'));
  }, []);

  const handleRender = useCallback(async () => {
    setLoading(true);
    setError(null);
    setSvgOutput(null);

    try {
      const svg = await renderDot(dotSource, engine, 'svg');
      setSvgOutput(svg);
    } catch (e: unknown) {
      const err = e as { code?: string; message?: string } | null | undefined;
      const code = err?.code || GraphvizErrorCode.UNKNOWN;
      const message = err?.message || 'Unknown rendering error';
      setError(`[${code}] ${message}`);
    } finally {
      setLoading(false);
    }
  }, [dotSource, engine]);

  // Render on mount and when engine changes
  useEffect(() => {
    handleRender();
  }, [handleRender]);

  return (
    <SafeAreaView style={styles.container}>
      <ScrollView contentContainerStyle={styles.scroll}>
        <Text style={styles.title}>Graphviz Anywhere Example</Text>
        {version ? (
          <Text style={styles.version}>Graphviz v{version}</Text>
        ) : null}

        {/* Engine selector */}
        <Text style={styles.label}>Layout Engine</Text>
        <ScrollView horizontal showsHorizontalScrollIndicator={false} style={styles.engineRow}>
          {ENGINES.map((eng) => (
            <TouchableOpacity
              key={eng}
              style={[styles.engineBtn, engine === eng && styles.engineBtnActive]}
              onPress={() => setEngine(eng)}
            >
              <Text style={[styles.engineText, engine === eng && styles.engineTextActive]}>
                {eng}
              </Text>
            </TouchableOpacity>
          ))}
        </ScrollView>

        {/* DOT source editor */}
        <Text style={styles.label}>DOT Source</Text>
        <TextInput
          style={styles.editor}
          multiline
          value={dotSource}
          onChangeText={setDotSource}
          autoCapitalize="none"
          autoCorrect={false}
          spellCheck={false}
          textAlignVertical="top"
          placeholder="Enter DOT language..."
          placeholderTextColor="#999"
        />

        {/* Render button */}
        <TouchableOpacity style={styles.renderBtn} onPress={handleRender} disabled={loading}>
          <Text style={styles.renderBtnText}>
            {loading ? 'Rendering...' : 'Render Graph'}
          </Text>
        </TouchableOpacity>

        {/* Output area */}
        {loading && <ActivityIndicator size="large" color="#4285f4" style={styles.spinner} />}

        {error && (
          <View style={styles.errorBox}>
            <Text style={styles.errorText}>{error}</Text>
          </View>
        )}

        {svgOutput && (
          <View style={styles.svgContainer}>
            <SvgXml xml={svgOutput} width="100%" />
          </View>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f8f9fa',
  },
  scroll: {
    padding: 16,
  },
  title: {
    fontSize: 24,
    fontWeight: '700',
    color: '#202124',
    marginBottom: 4,
  },
  version: {
    fontSize: 12,
    color: '#5f6368',
    marginBottom: 16,
  },
  label: {
    fontSize: 14,
    fontWeight: '600',
    color: '#5f6368',
    marginBottom: 8,
    marginTop: 12,
  },
  engineRow: {
    flexDirection: 'row',
    marginBottom: 8,
  },
  engineBtn: {
    paddingHorizontal: 14,
    paddingVertical: 8,
    borderRadius: 20,
    backgroundColor: '#e8eaed',
    marginRight: 8,
  },
  engineBtnActive: {
    backgroundColor: '#4285f4',
  },
  engineText: {
    fontSize: 13,
    color: '#5f6368',
    fontWeight: '500',
  },
  engineTextActive: {
    color: '#fff',
  },
  editor: {
    backgroundColor: '#fff',
    borderWidth: 1,
    borderColor: '#dadce0',
    borderRadius: 8,
    padding: 12,
    fontFamily: 'monospace',
    fontSize: 12,
    minHeight: 160,
    color: '#202124',
  },
  renderBtn: {
    backgroundColor: '#4285f4',
    borderRadius: 8,
    paddingVertical: 14,
    alignItems: 'center',
    marginTop: 16,
  },
  renderBtnText: {
    color: '#fff',
    fontSize: 16,
    fontWeight: '600',
  },
  spinner: {
    marginTop: 24,
  },
  errorBox: {
    backgroundColor: '#fce8e6',
    borderRadius: 8,
    padding: 12,
    marginTop: 16,
  },
  errorText: {
    color: '#c5221f',
    fontSize: 13,
  },
  svgContainer: {
    backgroundColor: '#fff',
    borderWidth: 1,
    borderColor: '#dadce0',
    borderRadius: 8,
    padding: 12,
    marginTop: 16,
    alignItems: 'center',
    overflow: 'hidden',
  },
});
