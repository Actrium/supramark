import DefaultTheme from 'vitepress/theme';
import { h } from 'vue';
import { useData } from 'vitepress';
import HomePreview from './components/HomePreview.vue';
import './tokens.css';
import './custom.css';

export default {
  extends: DefaultTheme,
  Layout() {
    const { frontmatter } = useData();

    return h(DefaultTheme.Layout, null, {
      'home-hero-before': () =>
        frontmatter.value.pageClass === 'sm-preview-home' ? h(HomePreview) : null,
    });
  },
};
