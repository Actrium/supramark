import type MarkdownIt from 'markdown-it';
import type { PluginWithOptions } from 'markdown-it';
import texmath from 'markdown-it-texmath';
import footnote from 'markdown-it-footnote';
import deflist from 'markdown-it-deflist';
// @ts-expect-error - markdown-it-emoji 类型定义不完整
import { full as emoji } from 'markdown-it-emoji';
import type { SupramarkConfig } from '../feature.js';
import { isFeatureEnabled, getFeatureOptionsAs } from '../feature.js';
import { registerInputSyntax } from './input.js';

interface GfmOptions {
  tables?: boolean;
  taskListItems?: boolean;
  strikethrough?: boolean;
}

interface EmojiOptions {
  shortcodes?: boolean;
  nativeOnly?: boolean;
}

// ----------------------------------------------------------------------------
// Inline HTML: 仅支持 <br> / <br/> / <br />
// ----------------------------------------------------------------------------

// 只将 `<br>` 家族标签映射为换行，避免在 `html:false` 前提下放开整套 HTML 能力。
function inlineBrPlugin(md: MarkdownIt) {
  md.inline.ruler.before('text', 'inline-br', function inlineBr(state, silent) {
    const start = state.pos;
    const src = state.src.slice(start);
    const match = src.match(/^<br\s*\/?>/i);

    if (!match) {
      return false;
    }
    if (silent) {
      return true;
    }

    const token = state.push('hardbreak', 'br', 0);
    token.markup = match[0];
    state.pos += match[0].length;
    return true;
  });
}

// ----------------------------------------------------------------------------
// GFM: 任务列表 & 删除线插件（inline 层）
// ----------------------------------------------------------------------------

// 简单的任务列表插件（GFM task lists: - [ ] / - [x]）
function taskListPlugin(md: MarkdownIt) {
  md.core.ruler.after('inline', 'task-lists', function taskLists(state) {
    const tokens = state.tokens;

    for (let i = 0; i < tokens.length; i++) {
      const token = tokens[i];

      if (token.type === 'list_item_open') {
        // 查找下一个 inline token
        let j = i + 1;
        while (j < tokens.length && tokens[j].type !== 'inline') {
          j++;
        }

        if (j < tokens.length) {
          const children = tokens[j].children;
          if (children && children.length > 0) {
            const firstChild = children[0];
            if (firstChild && firstChild.type === 'text') {
              const text = firstChild.content;
              const uncheckedMatch = text.match(/^\s*\[ \]\s+/);
              const checkedMatch = text.match(/^\s*\[x\]\s+/i);

              if (uncheckedMatch) {
                token.attrSet('task-list-item', 'false');
                firstChild.content = text.slice(uncheckedMatch[0].length);
              } else if (checkedMatch) {
                token.attrSet('task-list-item', 'true');
                firstChild.content = text.slice(checkedMatch[0].length);
              }
            }
          }
        }
      }
    }

    return false;
  });
}

// 简单的删除线插件（GFM strikethrough: ~~text~~）
function strikethroughPlugin(md: MarkdownIt) {
  // 添加 s_open 和 s_close 规则
  md.inline.ruler.before('emphasis', 'strikethrough', function strikethrough(state, silent) {
    const start = state.pos;
    const marker = state.src.charCodeAt(start);

    if (silent) return false;
    if (marker !== 0x7e /* ~ */) return false;

    const scanned = state.scanDelims(start, true);
    let len = scanned.length;
    const ch = String.fromCharCode(marker);

    if (len < 2) return false;

    if (len % 2) {
      const token = state.push('text', '', 0);
      token.content = ch;
      len--;
    }

    for (let i = 0; i < len; i += 2) {
      const token = state.push('text', '', 0);
      token.content = ch + ch;

      if (!scanned.can_open && !scanned.can_close) {
        continue;
      }

      state.delimiters.push({
        marker,
        length: 0,
        token: state.tokens.length - 1,
        end: -1,
        open: scanned.can_open,
        close: scanned.can_close,
      });
    }

    state.pos += scanned.length;
    return true;
  });

  md.inline.ruler2.before('emphasis', 'strikethrough', function strikethrough(state) {
    const delimiters = state.delimiters;
    const max = delimiters.length;

    for (let i = 0; i < max; i++) {
      const startDelim = delimiters[i];

      if (startDelim.marker !== 0x7e /* ~ */) continue;
      if (startDelim.end === -1) continue;

      const endDelim = delimiters[startDelim.end];

      const token_o = state.tokens[startDelim.token];
      token_o.type = 's_open';
      token_o.tag = 's';
      token_o.nesting = 1;
      token_o.markup = '~~';
      token_o.content = '';

      const token_c = state.tokens[endDelim.token];
      token_c.type = 's_close';
      token_c.tag = 's';
      token_c.nesting = -1;
      token_c.markup = '~~';
      token_c.content = '';

      if (
        state.tokens[endDelim.token - 1].type === 'text' &&
        state.tokens[endDelim.token - 1].content === '~'
      ) {
        state.tokens[endDelim.token - 1].content = '';
      }
    }

    return false;
  });
}

// ----------------------------------------------------------------------------
// 注册 main 家族的 markdown-it 插件
// ----------------------------------------------------------------------------

/**
 * 为 main 语法家族（Core / GFM / Math / Footnote / Definition / Emoji 等）
 * 在 MarkdownIt 实例上注册插件。
 *
 * - 不处理 :::container 或 ```fence（分别由 syntax-container / syntax-fence 负责）；
 * - 当未提供 config 或 features 为空时，视为所有内置扩展均启用。
 */
export function registerMainSyntaxPlugins(md: MarkdownIt, config?: SupramarkConfig): void {
  const hasConfig = !!config && !!config.features && config.features.length > 0;

  const isFeatureOn = (id: string): boolean => {
    if (!hasConfig) return true;
    return isFeatureEnabled(config!, id);
  };

  // 仅补充 `<br>` 的兼容解析，不放开其他 HTML 标签。
  md.use(inlineBrPlugin);

  // GFM：表格 + 任务列表 + 删除线
  if (isFeatureOn('@supramark/feature-gfm')) {
    const gfmOptions = getFeatureOptionsAs<GfmOptions>(config, '@supramark/feature-gfm') ?? {};

    const enableTables = gfmOptions.tables !== false;
    const enableTaskList = gfmOptions.taskListItems !== false;
    const enableStrikethrough = gfmOptions.strikethrough !== false;

    if (enableTables) {
      md.enable('table');
    }
    if (enableTaskList) {
      md.use(taskListPlugin);
    }
    if (enableStrikethrough) {
      md.use(strikethroughPlugin);
    }
  } else {
    // 即使关闭 GFM，出于兼容性考虑仍保留 markdown-it 内置 table 解析能力的最小子集，
    // 但 supramark AST 中不会再额外做 GFM 语义增强（当前阶段先保持简单策略）。
    md.enable('table');
  }

  // Math
  if (isFeatureOn('@supramark/feature-math')) {
    md.use(texmath as PluginWithOptions<{ engine: object; delimiters: string }>, {
      engine: {},
      delimiters: 'dollars',
    });
  }

  // Footnote
  if (isFeatureOn('@supramark/feature-footnote')) {
    md.use(footnote as PluginWithOptions);
  }

  // Definition list
  if (isFeatureOn('@supramark/feature-definition-list')) {
    md.use(deflist as PluginWithOptions);
  }

  // Emoji / 短代码
  if (isFeatureOn('@supramark/feature-emoji')) {
    const emojiOptions =
      getFeatureOptionsAs<EmojiOptions>(config, '@supramark/feature-emoji') ?? {};

    const enableShortcodes =
      emojiOptions.nativeOnly === true ? false : emojiOptions.shortcodes !== false;

    if (enableShortcodes) {
      md.use(emoji as PluginWithOptions);
    }
  }

  // Input 语法 (%%%)
  registerInputSyntax(md, config);
}
