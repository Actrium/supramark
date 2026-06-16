/**
 * Feature registry for preview app.
 *
 * Provides unified access to feature metadata + examples,
 * regardless of whether the feature is SupramarkFeature or ContainerFeature.
 */

import React from 'react';
import type { ExampleDefinition } from '@supramark/core';

import { admonitionFeature } from '@supramark/feature-admonition';
import { admonitionExamples } from '@supramark/feature-admonition';
import { coreMarkdownFeature } from '@supramark/feature-core-markdown';
import { coreMarkdownExamples } from '@supramark/feature-core-markdown';
import { d2Feature } from '@supramark/feature-d2';
import { d2Examples } from '@supramark/feature-d2';
import { definitionListFeature } from '@supramark/feature-definition-list';
import { definitionListExamples } from '@supramark/feature-definition-list';
import { diagramDotFeature } from '@supramark/feature-diagram-dot';
import { diagramDotExamples } from '@supramark/feature-diagram-dot';
import { diagramEchartsFeature } from '@supramark/feature-diagram-echarts';
import { diagramEchartsExamples } from '@supramark/feature-diagram-echarts';
import { diagramVegaLiteFeature } from '@supramark/feature-diagram-vega-lite';
import { diagramVegaLiteExamples } from '@supramark/feature-diagram-vega-lite';
import { mermaidFeature } from '@supramark/feature-mermaid';
import { mermaidExamples } from '@supramark/feature-mermaid';
import { plantumlFeature } from '@supramark/feature-plantuml';
import { plantumlExamples } from '@supramark/feature-plantuml';
import { emojiFeature } from '@supramark/feature-emoji';
import { emojiExamples } from '@supramark/feature-emoji';
import { footnoteFeature } from '@supramark/feature-footnote';
import { footnoteExamples } from '@supramark/feature-footnote';
import { gfmFeature } from '@supramark/feature-gfm';
import { gfmExamples } from '@supramark/feature-gfm';
import { mathFeature } from '@supramark/feature-math';
import { mathExamples } from '@supramark/feature-math';
import { weatherFeature } from '@supramark/feature-weather';
import { weatherExamples, renderWeatherContainerWeb } from '@supramark/feature-weather';
import { htmlPageFeature, htmlPageExamples } from '@supramark/feature-html-page';
import { mapFeature, mapExamples } from '@supramark/feature-map';

// Register container feature parsers (must run before parsing).
// html-page and map register their container hooks via module side effect
// (`import './runtime.js'` in their index.ts), so simply importing above
// already wired the parser. Weather/admonition still use the explicit API.
admonitionFeature.registerParser();
weatherFeature.registerParser();

// Container renderers for Supramark component
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const containerRenderers: Record<string, any> = {
  weather: renderWeatherContainerWeb,
  html: renderHtmlContainerWeb,
  map: renderMapContainerWeb,
};

// Minimal inline renderers — html-page/map packages don't export a
// ready-made web component, so we render the container data here.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function renderHtmlContainerWeb({ node, key }: any) {
  const html = (node.data?.html as string | undefined) ?? '';
  const title = extractHtmlTitle(html) ?? 'HTML Page';
  return (
    <article key={key} className="supramark-html-page">
      <div className="supramark-html-page__header">
        <div>
          <span className="supramark-html-page__eyebrow">HTML Page</span>
          <strong className="supramark-html-page__title">{title}</strong>
        </div>
      </div>
      <iframe className="supramark-html-page__frame" title={title} srcDoc={html} sandbox="" />
    </article>
  );
}

function extractHtmlTitle(html: string): string | undefined {
  const match = /<title[^>]*>([\s\S]*?)<\/title>/i.exec(html);
  const title = match?.[1]?.replace(/\s+/g, ' ').trim();
  return title || undefined;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function renderMapContainerWeb({ node, key }: any) {
  const data = node.data ?? {};
  const center = Array.isArray(data.center) ? data.center : undefined;
  const lat = center?.[0] ?? data.marker?.lat;
  const lng = center?.[1] ?? data.marker?.lng;
  const zoom = data.zoom;
  return (
    <div
      key={key}
      className="supramark-map"
      style={{
        margin: '1em 0',
        padding: 12,
        border: '1px dashed #9ec5ff',
        borderRadius: 6,
        background: '#f0f6ff',
        fontSize: 13,
        color: '#1f3a6b',
      }}
    >
      <strong style={{ marginRight: 8 }}>🗺️ Map</strong>
      <span>
        {lat != null && lng != null
          ? `${Number(lat).toFixed(4)}, ${Number(lng).toFixed(4)}`
          : '(no coordinates)'}
        {zoom != null ? ` · zoom ${zoom}` : ''}
      </span>
    </div>
  );
}

export interface FeatureEntry {
  /** Short name, e.g. "weather", "math" */
  shortName: string;
  /** Display name, e.g. "Weather", "Math Formula" */
  displayName: string;
  /** Semver version */
  version: string;
  /** Examples */
  examples: ExampleDefinition[];
}

function shortName(id: string): string {
  // "@supramark/feature-weather" -> "weather"
  return id.replace(/^@supramark\/feature-/, '');
}

export const featureRegistry: FeatureEntry[] = [
  { shortName: shortName(admonitionFeature.id), displayName: admonitionFeature.name, version: admonitionFeature.version, examples: admonitionExamples },
  { shortName: shortName(coreMarkdownFeature.metadata.id), displayName: coreMarkdownFeature.metadata.name, version: coreMarkdownFeature.metadata.version, examples: coreMarkdownExamples },
  { shortName: shortName(d2Feature.metadata.id), displayName: d2Feature.metadata.name, version: d2Feature.metadata.version, examples: d2Examples },
  { shortName: shortName(definitionListFeature.metadata.id), displayName: definitionListFeature.metadata.name, version: definitionListFeature.metadata.version, examples: definitionListExamples },
  { shortName: shortName(diagramDotFeature.metadata.id), displayName: diagramDotFeature.metadata.name, version: diagramDotFeature.metadata.version, examples: diagramDotExamples },
  { shortName: shortName(diagramEchartsFeature.metadata.id), displayName: diagramEchartsFeature.metadata.name, version: diagramEchartsFeature.metadata.version, examples: diagramEchartsExamples },
  { shortName: shortName(diagramVegaLiteFeature.metadata.id), displayName: diagramVegaLiteFeature.metadata.name, version: diagramVegaLiteFeature.metadata.version, examples: diagramVegaLiteExamples },
  { shortName: shortName(mermaidFeature.metadata.id), displayName: mermaidFeature.metadata.name, version: mermaidFeature.metadata.version, examples: mermaidExamples },
  { shortName: shortName(plantumlFeature.metadata.id), displayName: plantumlFeature.metadata.name, version: plantumlFeature.metadata.version, examples: plantumlExamples },
  { shortName: shortName(emojiFeature.metadata.id), displayName: emojiFeature.metadata.name, version: emojiFeature.metadata.version, examples: emojiExamples },
  { shortName: shortName(footnoteFeature.metadata.id), displayName: footnoteFeature.metadata.name, version: footnoteFeature.metadata.version, examples: footnoteExamples },
  { shortName: shortName(gfmFeature.metadata.id), displayName: gfmFeature.metadata.name, version: gfmFeature.metadata.version, examples: gfmExamples },
  { shortName: shortName(mathFeature.metadata.id), displayName: mathFeature.metadata.name, version: mathFeature.metadata.version, examples: mathExamples },
  { shortName: shortName(weatherFeature.id), displayName: weatherFeature.name, version: weatherFeature.version, examples: weatherExamples },
  { shortName: shortName(htmlPageFeature.metadata.id), displayName: htmlPageFeature.metadata.name, version: htmlPageFeature.metadata.version, examples: htmlPageExamples },
  { shortName: shortName(mapFeature.metadata.id), displayName: mapFeature.metadata.name, version: mapFeature.metadata.version, examples: mapExamples },
].sort((a, b) => a.displayName.localeCompare(b.displayName));

export function findFeature(name: string): FeatureEntry | undefined {
  return featureRegistry.find(f => f.shortName === name || f.displayName.toLowerCase() === name.toLowerCase());
}
