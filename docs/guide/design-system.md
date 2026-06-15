# 设计系统

Supramark 文档站使用 Ultraviolet 作为主品牌色，并把颜色、间距、圆角、阴影、动效和组件样式拆成可复用 token。新增页面或组件时优先使用 token，避免在页面里直接写一次性颜色值。

## 品牌基调

主色来自电光紫 `#7C3AED`，对应 Supramark 的语义增强、图表渲染和高阶 Markdown 扩展定位。辅助色使用 cyan，负责表示连接、运行时和跨平台能力；状态色独立保留，避免所有信息都被紫色表达。

| 用途        | Token            | 值        |
| ----------- | ---------------- | --------- |
| Primary     | `--sm-brand-600` | `#7C3AED` |
| Accent      | `--sm-brand-400` | `#A78BFA` |
| Dark        | `--sm-brand-950` | `#2E1065` |
| Light       | `--sm-brand-50`  | `#F5F3FF` |
| Link accent | `--sm-cyan-600`  | `#0891B2` |

## Token 分层

### Primitive tokens

Primitive token 是原始尺度，不直接表达组件含义。

```css
--sm-brand-600: #7c3aed;
--sm-space-6: 24px;
--sm-radius-lg: 8px;
--sm-duration-normal: 180ms;
```

### Semantic tokens

Semantic token 表达产品语义，新增页面应优先使用这一层。

```css
--sm-color-brand: var(--sm-brand-600);
--sm-color-bg-soft: #fbfaff;
--sm-color-text-soft: #4c456d;
--sm-color-border: rgba(109, 40, 217, 0.16);
```

### Component tokens

Component token 用于稳定常见组件的视觉语言。

```css
--sm-card-radius: var(--sm-radius-lg);
--sm-card-border: 1px solid var(--sm-color-border);
--sm-button-radius: var(--sm-radius-md);
--sm-code-radius: var(--sm-radius-md);
```

## VitePress 映射

文档站在 `docs/.vitepress/theme/tokens.css` 中把 Supramark token 映射到 VitePress 变量：

```css
--vp-c-brand-1: var(--sm-color-brand);
--vp-c-bg: var(--sm-color-bg);
--vp-c-text-1: var(--sm-color-text);
--vp-button-brand-bg: var(--sm-color-brand);
```

因此默认导航、按钮、链接、代码块和 custom block 会自动继承品牌体系。页面级样式只需要消费 `--sm-*` token，不要覆盖 `--vp-*` 变量，除非是在维护主题桥接层。

## 使用约定

- 新增页面区块时，背景使用 `--sm-color-bg`、`--sm-color-bg-soft` 或 `--sm-color-surface`。
- 交互主操作使用 `--sm-color-brand`，次要操作使用 border + `--sm-color-bg-mute`。
- 图表、运行时、跨平台相关提示可以使用 `--sm-color-accent`，但不要替代主操作色。
- 状态反馈使用 success / warning / danger semantic token，不要用紫色表达错误或警告。
- 卡片圆角默认使用 `--sm-card-radius`，紧凑控件使用 `--sm-radius-md` 或更小。
- 文档正文不要自定义字体大小阶梯，优先保持 VitePress 的排版节奏。

## 文件位置

```text
docs/.vitepress/theme/index.ts
docs/.vitepress/theme/tokens.css
docs/.vitepress/theme/custom.css
docs/public/brand/supramark-mark.svg
```

`tokens.css` 只负责 token 定义和 VitePress 映射；`custom.css` 只负责消费 token 的视觉规则。后续如果新增 Feature gallery、API overview 或交互式示例，应先在 token 层补齐语义变量，再写组件样式。
