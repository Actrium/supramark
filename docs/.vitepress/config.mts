import { defineConfig } from 'vitepress';

// Supramark documentation site.
// base MUST be /supramark/ (GitHub Pages project-site path).
export default defineConfig({
  lang: 'zh-CN',
  title: 'Supramark',
  description:
    'A Markdown extension and diagram rendering integration library for React Native / mini-program hosts',
  base: '/supramark/',
  srcExclude: ['guide/design-system.zh.md'],
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
        return '<code v-pre>' + md.utils.escapeHtml(tokens[idx].content) + '</code>';
      };
    },
  },
  themeConfig: {
    nav: [
      { text: 'Guide', link: '/guide/getting-started.zh' },
      { text: 'Architecture', link: '/architecture/DOCUMENTATION_ARCHITECTURE.zh' },
      { text: 'Features', link: '/features/' },
      { text: 'Examples', link: '/examples/' },
      { text: 'API', link: '/typedoc/' },
    ],
    sidebar: {
      '/guide/': [
        {
          text: 'Guide',
          items: [
            { text: 'Getting Started', link: '/guide/getting-started.zh' },
            { text: 'Core Concepts', link: '/guide/concepts.zh' },
            { text: 'Architecture', link: '/guide/architecture.zh' },
            { text: 'Custom Features', link: '/guide/custom-features.zh' },
            { text: 'Feature Creation Guide', link: '/guide/CREATE_FEATURE_GUIDE.zh' },
            { text: 'Feature Quality Assurance', link: '/guide/FEATURE_QUALITY_ASSURANCE.zh' },
            { text: 'CI Setup', link: '/guide/CI_SETUP.zh' },
            { text: 'Crate Release', link: '/guide/CRATE_RELEASE.zh' },
            { text: 'Documentation System', link: '/guide/doc-system.zh' },
          ],
        },
      ],
      '/architecture/': [
        {
          text: 'Architecture',
          items: [
            { text: 'Documentation Architecture', link: '/architecture/DOCUMENTATION_ARCHITECTURE.zh' },
            { text: 'Plugin System', link: '/architecture/PLUGIN_SYSTEM.zh' },
            { text: 'Engines and CLI Plan', link: '/architecture/ENGINES_AND_CLI_PLAN.zh' },
            { text: 'Diagram Engine Target', link: '/architecture/DIAGRAM_ENGINE_TARGET.zh' },
            { text: 'Diagram Semantic AST', link: '/architecture/diagram-semantic-ast.zh' },
            {
              text: 'Diagram Semantic AST Implementation',
              link: '/architecture/diagram-semantic-ast-impl-plan.zh',
            },
            { text: 'AST Spec', link: '/architecture/ast-spec.zh' },
            { text: 'Dependency Graph', link: '/architecture/dependency-graph.zh' },
            { text: 'License Compatibility', link: '/architecture/LICENSE_COMPATIBILITY.zh' },
            { text: 'Project Structure Report', link: '/architecture/PROJECT_STRUCTURE_REPORT.zh' },
            { text: 'Native FFI Blockers', link: '/architecture/native-ffi-blockers.zh' },
          ],
        },
      ],
      '/features/': [
        {
          text: 'Features',
          items: [
            { text: 'Overview', link: '/features/' },
            { text: 'Core Markdown', link: '/features/core-markdown' },
            { text: 'GFM', link: '/features/gfm' },
            { text: 'Math', link: '/features/math' },
            { text: 'Admonition', link: '/features/admonition' },
            { text: 'Definition List', link: '/features/definition-list' },
            { text: 'Emoji', link: '/features/emoji' },
            { text: 'Footnote', link: '/features/footnote' },
          ],
        },
      ],
      '/examples/': [
        {
          text: 'Examples',
          items: [
            { text: 'Overview', link: '/examples/' },
            { text: 'Feature Example Gallery', link: '/examples/gallery' },
            { text: 'React Web CSR', link: '/examples/react-web-csr' },
            { text: 'React Native', link: '/examples/react-native' },
            { text: 'Build Configuration Examples', link: '/examples/config-examples' },
          ],
        },
      ],
    },
    socialLinks: [{ icon: 'github', link: 'https://github.com/Actrium/supramark' }],
    outline: { label: 'On this page', level: [2, 3] },
    docFooter: { prev: 'Previous page', next: 'Next page' },
  },
});
