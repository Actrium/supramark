import { defineConfig } from 'vitepress'

// Supramark documentation site.
// base MUST be /supramark/ (GitHub Pages project-site path).
export default defineConfig({
  lang: 'zh-CN',
  title: 'Supramark',
  description: '面向 React Native / 小程序宿主的 Markdown 扩展与图表渲染集成库',
  base: '/supramark/',
  // Existing docs use plain .md relative links; tolerate dead links for now.
  ignoreDeadLinks: true,
  // Inline code (single backtick) is NOT v-pre by default in VitePress,
  // so `{{ }}` inside it gets parsed as a Vue interpolation and breaks the
  // build. Force every inline code span to v-pre.
  markdown: {
    // Treat raw tags in prose as literal text (these docs aren't authored for Vue).
    html: false,
    config(md) {
      md.renderer.rules.code_inline = (tokens, idx) => {
        return '<code v-pre>' + md.utils.escapeHtml(tokens[idx].content) + '</code>'
      }
    },
  },
  themeConfig: {
    nav: [
      { text: '指南', link: '/guide/getting-started' },
      { text: '架构', link: '/architecture/DOCUMENTATION_ARCHITECTURE' },
      { text: 'Features', link: '/features/' },
      { text: '示例', link: '/examples/' },
      { text: 'API', link: '/typedoc/' },
    ],
    sidebar: {
      '/guide/': [
        {
          text: '指南',
          items: [
            { text: '快速开始', link: '/guide/getting-started' },
            { text: '核心概念', link: '/guide/concepts' },
            { text: '架构', link: '/guide/architecture' },
            { text: '自定义 Feature', link: '/guide/custom-features' },
            { text: '创建 Feature 指南', link: '/guide/CREATE_FEATURE_GUIDE' },
            { text: 'Feature 质量保障', link: '/guide/FEATURE_QUALITY_ASSURANCE' },
            { text: 'CI 配置', link: '/guide/CI_SETUP' },
            { text: '文档系统', link: '/guide/doc-system' },
          ],
        },
      ],
      '/architecture/': [
        {
          text: '架构',
          items: [
            { text: '文档架构', link: '/architecture/DOCUMENTATION_ARCHITECTURE' },
            { text: '插件系统', link: '/architecture/PLUGIN_SYSTEM' },
            { text: '引擎与 CLI 规划', link: '/architecture/ENGINES_AND_CLI_PLAN' },
            { text: 'Diagram 引擎目标', link: '/architecture/DIAGRAM_ENGINE_TARGET' },
            { text: 'Diagram 语义 AST', link: '/architecture/diagram-semantic-ast' },
            { text: 'Diagram 语义 AST 实施', link: '/architecture/diagram-semantic-ast-impl-plan' },
            { text: 'AST 规范', link: '/architecture/ast-spec' },
            { text: '依赖图', link: '/architecture/dependency-graph' },
            { text: '许可证兼容', link: '/architecture/LICENSE_COMPATIBILITY' },
            { text: '项目结构报告', link: '/architecture/PROJECT_STRUCTURE_REPORT' },
            { text: 'Native FFI 阻塞项', link: '/architecture/native-ffi-blockers' },
          ],
        },
      ],
      '/features/': [
        {
          text: 'Features',
          items: [
            { text: '概览', link: '/features/' },
            { text: 'Core Markdown', link: '/features/core-markdown' },
            { text: 'GFM', link: '/features/gfm' },
            { text: '数学公式', link: '/features/math' },
            { text: 'Admonition', link: '/features/admonition' },
            { text: '定义列表', link: '/features/definition-list' },
            { text: 'Emoji', link: '/features/emoji' },
            { text: '脚注', link: '/features/footnote' },
          ],
        },
      ],
      '/examples/': [
        {
          text: '示例',
          items: [
            { text: '概览', link: '/examples/' },
            { text: 'React Web', link: '/examples/react-web' },
            { text: 'React Web CSR', link: '/examples/react-web-csr' },
            { text: 'React Web Demo', link: '/examples/react-web-demo' },
            { text: 'React Native', link: '/examples/react-native' },
            { text: 'Native Demo', link: '/examples/native-demo' },
          ],
        },
      ],
    },
    socialLinks: [{ icon: 'github', link: 'https://github.com/kookyleo/supramark' }],
    outline: { label: '本页目录', level: [2, 3] },
    docFooter: { prev: '上一页', next: '下一页' },
  },
})
