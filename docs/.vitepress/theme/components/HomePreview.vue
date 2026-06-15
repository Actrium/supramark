<script setup lang="ts">
import { withBase } from 'vitepress';

const previewPath = withBase('/preview/?feature=mermaid');
const guidePath = withBase('/guide/getting-started');
</script>

<template>
  <section class="sm-home-preview" aria-labelledby="sm-home-title">
    <div class="sm-home-preview__intro">
      <div class="sm-home-preview__copy">
        <p class="sm-home-preview__eyebrow">Live Feature Preview</p>
        <h1 id="sm-home-title">多端 Markdown 与图表渲染预览</h1>
        <p>
          选择 Mermaid、D2、PlantUML、ECharts、Vega-Lite、数学公式或容器 Feature，直接编辑 Markdown
          并查看渲染结果。
        </p>
      </div>
      <div class="sm-home-preview__actions">
        <a class="sm-home-preview__button sm-home-preview__button--primary" :href="previewPath">
          打开完整预览
        </a>
        <a class="sm-home-preview__button" :href="guidePath">快速开始</a>
      </div>
    </div>

    <div class="sm-home-preview__frame" aria-label="Supramark feature preview">
      <iframe
        title="Supramark feature preview"
        :src="previewPath"
        loading="eager"
        referrerpolicy="no-referrer"
      />
    </div>
  </section>
</template>
