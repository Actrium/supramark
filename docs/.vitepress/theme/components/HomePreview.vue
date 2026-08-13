<script setup lang="ts">
import { withBase } from 'vitepress';

const playgroundPath = withBase('/playground/mermaid/');
const guidePath = withBase('/guide/getting-started');
</script>

<template>
  <section class="sm-home-preview" aria-labelledby="sm-home-title">
    <div class="sm-home-preview__intro">
      <div class="sm-home-preview__copy">
        <p class="sm-home-preview__eyebrow">Live Playground</p>
        <h1 id="sm-home-title">Cross-platform Markdown and diagram playground</h1>
        <p>
          Pick Mermaid, D2, PlantUML, ECharts, Vega-Lite, math, or container features, edit the
          Markdown directly, and see the rendered result.
        </p>
      </div>
      <div class="sm-home-preview__actions">
        <a
          class="sm-home-preview__button sm-home-preview__button--primary"
          :href="playgroundPath"
          target="_self"
        >
          Open playground
        </a>
        <a class="sm-home-preview__button" :href="guidePath">Get started</a>
      </div>
    </div>

    <div class="sm-home-preview__frame" aria-label="Supramark playground">
      <iframe
        title="Supramark playground"
        :src="playgroundPath"
        loading="eager"
        referrerpolicy="no-referrer"
      />
    </div>
  </section>
</template>
