/**
 * Supramark RN demo app.
 *
 * Migrated to the current feature API:
 *  - Imports per-feature objects directly (no more `createXxxFeatureConfig`
 *    factories on the caller side — we just write FeatureConfig literals).
 *  - Calls `admonitionFeature.registerParser()` once at module load, since
 *    admonition is a ContainerFeature whose hooks aren't auto-registered.
 *  - html-page / map register their parsers via their own runtime.js
 *    side-effect import; importing the feature object suffices.
 *  - Drops the old `DiagramRenderProvider` wrapper — the RN renderer now
 *    creates its own engine via `createReactNativeDiagramEngine()` and
 *    native adapters (d2 / mermaid / plantuml) register themselves via
 *    the side-effect imports below.
 *
 * Boot also runs the three native FFI engines once for smoke verification.
 * Look for `[<ENGINE>_SMOKE_*]` lines in logcat / xcode console.
 */

import React, { useEffect, useState } from 'react';
import {
  SafeAreaView,
  ScrollView,
  StyleSheet,
  Text,
  TouchableOpacity,
  View,
  Alert,
  NativeModules,
} from 'react-native';

import { Supramark } from '@supramark/rn';
import type { SupramarkConfig } from '@supramark/core';

// Feature metadata — id/version is read off these for the FeatureConfig list.
import { coreMarkdownFeature } from '@supramark/feature-core-markdown';
import { gfmFeature } from '@supramark/feature-gfm';
import { admonitionFeature } from '@supramark/feature-admonition';
import { definitionListFeature } from '@supramark/feature-definition-list';
import { htmlPageFeature } from '@supramark/feature-html-page';
import { mapFeature } from '@supramark/feature-map';
import { diagramVegaLiteFeature } from '@supramark/feature-diagram-vega-lite';
import { diagramEchartsFeature } from '@supramark/feature-diagram-echarts';
import { diagramDotFeature } from '@supramark/feature-diagram-dot';

// Side-effect: each registers a native adapter against @supramark/engines/rn
// so the diagram engine routes d2 / mermaid / plantuml blocks to the linked
// libsupramark_*_native.so / .a.
import '@actrium/supramark-d2-native-rn';
import '@actrium/supramark-mermaid-native-rn';
import '@actrium/supramark-plantuml-native-rn';
// Side-effect: registers the native Markdown parser adapter against
// @supramark/core's native registry so parse() routes source through the
// linked libsupramark_markdown_native static lib (no wasm on RN).
import '@supramark/markdown-native-rn';

import { DEMOS } from '../demos';

// admonition is a ContainerFeature — its container hooks must be registered
// explicitly. html-page / map register theirs via side-effect import in their
// own index.ts.
admonitionFeature.registerParser();

const BASE_CONFIG: SupramarkConfig = {
  features: [
    { id: coreMarkdownFeature.metadata.id, enabled: true },
    {
      id: gfmFeature.metadata.id,
      enabled: true,
      options: { tables: true, taskListItems: true, strikethrough: false },
    },
    {
      id: admonitionFeature.id,
      enabled: true,
      options: { kinds: ['note', 'warning'] },
    },
    {
      id: definitionListFeature.metadata.id,
      enabled: true,
      options: { compact: true },
    },
    { id: htmlPageFeature.metadata.id, enabled: true },
    { id: mapFeature.metadata.id, enabled: true, options: { provider: 'custom' } },
    { id: diagramVegaLiteFeature.metadata.id, enabled: true },
    { id: diagramEchartsFeature.metadata.id, enabled: true },
    { id: diagramDotFeature.metadata.id, enabled: true },
  ],

  diagram: {
    defaultTimeoutMs: 10000,
    defaultCache: {
      enabled: true,
      maxSize: 100,
      ttl: 300000,
    },
  },
};

type Theme = 'light' | 'dark';

interface NativeEngine {
  render: (source: string) => Promise<string>;
  getVersion: () => Promise<string>;
}

type SmokeStatus = 'pending' | 'ok' | 'error';
interface SmokeResult {
  name: string;
  status: SmokeStatus;
  detail: string;
}

const SMOKE_SPEC: Array<{ name: string; key: string; source: string }> = [
  { name: 'd2', key: 'SupramarkD2Native', source: 'a -> b -> c' },
  {
    name: 'mermaid',
    key: 'SupramarkMermaidNative',
    source: 'graph TD; A-->B; A-->C; B-->D; C-->D;',
  },
  {
    name: 'plantuml',
    key: 'SupramarkPlantumlNative',
    source: '@startuml\nAlice -> Bob: hi\nBob --> Alice: hello\n@enduml',
  },
];

export default function App() {
  const [activeId, setActiveId] = useState<string | null>(null);
  const [theme, setTheme] = useState<Theme>('light');
  const [smoke, setSmoke] = useState<SmokeResult[]>(
    SMOKE_SPEC.map((s) => ({ name: s.name, status: 'pending', detail: 'booting...' })),
  );
  const activeDemo = activeId ? DEMOS.find((d) => d.id === activeId) ?? null : null;

  const isDark = theme === 'dark';
  const toggleTheme = () => setTheme(isDark ? 'light' : 'dark');

  const runNativeSmokeTest = async () => {
    for (let i = 0; i < SMOKE_SPEC.length; i++) {
      const spec = SMOKE_SPEC[i];
      const mod = NativeModules[spec.key] as NativeEngine | undefined;
      let next: SmokeResult;
      try {
        if (!mod) {
          throw new Error(`NativeModules.${spec.key} is undefined — not linked`);
        }
        const version = await mod.getVersion();
        const svg = await mod.render(spec.source);
        const line = `[${spec.name.toUpperCase()}_SMOKE_OK] v=${version} len=${svg.length}`;
        console.log(line);
        next = {
          name: spec.name,
          status: 'ok',
          detail: `v=${version}  svg.length=${svg.length}`,
        };
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        console.log(`[${spec.name.toUpperCase()}_SMOKE_ERROR] ${msg.slice(0, 300)}`);
        next = { name: spec.name, status: 'error', detail: msg.slice(0, 600) };
      }
      setSmoke((prev) => {
        const arr = [...prev];
        arr[i] = next;
        return arr;
      });
    }
  };

  useEffect(() => {
    runNativeSmokeTest();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const containerStyle = [styles.container, isDark && { backgroundColor: '#0d1117' }];
  const headerStyle = [styles.header, isDark && { borderBottomColor: '#30363d' }];
  const titleStyle = [styles.title, isDark && { color: '#ffffff' }];
  const subtitleStyle = [styles.subtitle, isDark && { color: '#8b949e' }];
  const menuContentStyle = [styles.menuContent, isDark && { backgroundColor: '#0d1117' }];
  const menuItemStyle = [styles.menuItem, isDark && { borderBottomColor: '#21262d' }];
  const menuItemTitleStyle = [styles.menuItemTitle, isDark && { color: '#ffffff' }];
  const menuItemDescStyle = [styles.menuItemDesc, isDark && { color: '#8b949e' }];
  const themeButtonStyle = [styles.themeButton, isDark && { backgroundColor: '#21262d' }];
  const themeButtonTextStyle = [styles.themeButtonText, isDark && { color: '#58a6ff' }];

  if (!activeDemo) {
    return (
      <SafeAreaView style={containerStyle}>
        <View style={headerStyle}>
          <View style={styles.headerRow}>
            <View style={styles.headerLeft}>
              <Text style={titleStyle}>supramark Demo</Text>
              <Text style={subtitleStyle}>
                选择要演示的类型，进入详情查看 markdown 与渲染结果。
              </Text>
            </View>
            <TouchableOpacity style={themeButtonStyle} onPress={toggleTheme}>
              <Text style={themeButtonTextStyle}>{isDark ? '☀️ Light' : '🌙 Dark'}</Text>
            </TouchableOpacity>
          </View>
          <View style={styles.smokeRow}>
            {smoke.map((s) => (
              <View key={s.name} style={[styles.smokeBadge, smokeBadgeStyle(s.status)]}>
                <Text style={styles.smokeBadgeText}>
                  {s.name} · {s.status}
                </Text>
              </View>
            ))}
          </View>
          <TouchableOpacity
            style={[themeButtonStyle, { marginTop: 8, alignSelf: 'flex-start' }]}
            onPress={runNativeSmokeTest}
          >
            <Text style={themeButtonTextStyle}>🧪 Re-run native FFI smoke (d2 / mermaid / plantuml)</Text>
          </TouchableOpacity>
        </View>
        <ScrollView contentContainerStyle={menuContentStyle}>
          {DEMOS.map((demo) => (
            <TouchableOpacity
              key={demo.id}
              style={menuItemStyle}
              onPress={() => setActiveId(demo.id)}
            >
              <Text style={menuItemTitleStyle}>{demo.name}</Text>
              <Text style={menuItemDescStyle}>{demo.description}</Text>
            </TouchableOpacity>
          ))}
        </ScrollView>
      </SafeAreaView>
    );
  }

  const detailContentStyle = [styles.detailContent, isDark && { backgroundColor: '#0d1117' }];
  const demoTitleStyle = [styles.demoTitle, isDark && { color: '#ffffff' }];
  const demoDescriptionStyle = [styles.demoDescription, isDark && { color: '#8b949e' }];
  const demoSectionTitleStyle = [styles.demoSectionTitle, isDark && { color: '#ffffff' }];
  const sourceBlockStyle = [
    styles.sourceBlock,
    isDark && { backgroundColor: '#161b22', borderColor: '#30363d' },
  ];
  const sourceTextStyle = [styles.sourceText, isDark && { color: '#e0e0e0' }];

  return (
    <SafeAreaView style={containerStyle}>
      <ScrollView contentContainerStyle={detailContentStyle}>
        <View style={styles.detailHeader}>
          <TouchableOpacity onPress={() => setActiveId(null)} style={styles.backButton}>
            <Text style={[styles.backButtonText, isDark && { color: '#58a6ff' }]}>‹ 返回</Text>
          </TouchableOpacity>
          <View style={styles.detailTitleWrap}>
            <Text style={demoTitleStyle}>{activeDemo.name}</Text>
            <Text style={demoDescriptionStyle}>{activeDemo.description}</Text>
          </View>
          <TouchableOpacity style={themeButtonStyle} onPress={toggleTheme}>
            <Text style={themeButtonTextStyle}>{isDark ? '☀️' : '🌙'}</Text>
          </TouchableOpacity>
        </View>
        <Text style={demoSectionTitleStyle}>Markdown 源文本：</Text>
        <View style={sourceBlockStyle}>
          <Text style={sourceTextStyle}>{activeDemo.markdown}</Text>
        </View>
        <Text style={demoSectionTitleStyle}>渲染结果：</Text>
        <View style={styles.renderBlock}>
          <Supramark
            markdown={activeDemo.markdown}
            theme={theme}
            config={BASE_CONFIG}
            onOpenHtmlPage={(node) => {
              Alert.alert(
                node.params || 'HTML Page',
                '这里应该在宿主中打开独立页面或 Modal。当前只是示意回调已触发。',
              );
            }}
          />
        </View>
      </ScrollView>
    </SafeAreaView>
  );
}

function smokeBadgeStyle(s: SmokeStatus) {
  switch (s) {
    case 'ok':
      return { backgroundColor: '#1f883d' };
    case 'error':
      return { backgroundColor: '#cf222e' };
    default:
      return { backgroundColor: '#9aa0a6' };
  }
}

const styles = StyleSheet.create({
  container: { flex: 1 },
  title: { fontSize: 24, fontWeight: '600', marginBottom: 4 },
  subtitle: { fontSize: 14, color: '#666' },
  header: {
    paddingHorizontal: 16,
    paddingTop: 12,
    paddingBottom: 8,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: '#ddd',
  },
  headerRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  headerLeft: { flex: 1 },
  themeButton: {
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 6,
    backgroundColor: '#f5f5f5',
    marginLeft: 12,
  },
  themeButtonText: { fontSize: 12, fontWeight: '600', color: '#2f54eb' },
  smokeRow: { flexDirection: 'row', flexWrap: 'wrap', marginTop: 8, gap: 6 },
  smokeBadge: {
    paddingHorizontal: 8,
    paddingVertical: 3,
    borderRadius: 3,
    marginRight: 6,
    marginBottom: 4,
  },
  smokeBadgeText: { fontSize: 11, fontWeight: '700', color: '#ffffff' },
  menuContent: { paddingHorizontal: 16, paddingVertical: 12 },
  menuItem: {
    paddingVertical: 12,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: '#eee',
  },
  menuItemTitle: { fontSize: 16, fontWeight: '600', marginBottom: 4 },
  menuItemDesc: { fontSize: 13, color: '#666' },
  detailContent: { flexGrow: 1, padding: 16 },
  detailHeader: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    marginBottom: 12,
  },
  backButton: { paddingRight: 12, paddingVertical: 4 },
  backButtonText: { fontSize: 14, color: '#2f54eb' },
  detailTitleWrap: { flex: 1 },
  demoTitle: { fontSize: 20, fontWeight: '600', marginBottom: 4 },
  demoDescription: { fontSize: 13, color: '#666', marginBottom: 12 },
  demoSectionTitle: {
    fontSize: 14,
    fontWeight: '600',
    marginTop: 12,
    marginBottom: 6,
  },
  sourceBlock: {
    padding: 8,
    borderRadius: 4,
    backgroundColor: '#fafafa',
    borderWidth: StyleSheet.hairlineWidth,
    borderColor: '#eee',
  },
  sourceText: { fontFamily: 'Menlo', fontSize: 12 },
  renderBlock: { marginTop: 4 },
});
