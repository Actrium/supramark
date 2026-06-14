import { useState } from 'react';
import { Supramark } from '@supramark/web/client';

// Register weather container hook (must be imported before using)
import { renderWeatherContainerWeb } from '@supramark/feature-weather';

import './App.css';

const INITIAL_MARKDOWN = `# Supramark Live Editor

欢迎使用 **Supramark** 的实时 Markdown 编辑器！

## 功能特性

### GFM 扩展支持

- **粗体文本**
- *斜体文本*
- \`内联代码\`
- ~~删除线~~

### 任务列表

- [x] 支持 GFM 任务列表
- [x] 实时预览
- [x] 主题切换
- [ ] 更多功能开发中

### 表格示例

| 功能 | 状态 | 说明 |
| --- | :---: | ---: |
| 删除线 | ✅ | 使用 \`~~\` 语法 |
| 任务列表 | ✅ | \`[ ]\` 和 \`[x]\` |
| 表格 | ✅ | 标准 GFM 表格 |
| 主题系统 | ✅ | 支持自定义 className |

### 代码块

\`\`\`javascript
function hello(name) {
  console.log('Hello, ' + name);
}

hello('Supramark');
\`\`\`

### 链接和图片

这是一个 [链接示例](https://github.com)

### Weather 卡片

:::weather
location: Shanghai
condition: Cloudy
tempC: 22
:::

### Mermaid 图表

\`\`\`mermaid
graph TD
    A[开始] --> B{是否喜欢 Supramark?}
    B -->|是| C[继续使用]
    B -->|否| D[再试一次]
    C --> E[享受编写 Markdown]
    D --> B
\`\`\`

---

在左侧编辑 Markdown，右上角切换主题，右侧实时预览结果！
`.trim();

type ThemeOption = 'none' | 'tailwind' | 'minimal';

function App() {
  const [markdown, setMarkdown] = useState(INITIAL_MARKDOWN);
  const [theme, setTheme] = useState<ThemeOption>('tailwind');

  return (
    <div className="app-container">
      <header className="app-header">
        <h1>Supramark Live Editor</h1>
        <p>实时 Markdown 编辑器 - CSR 示例</p>
      </header>
      <div className="editor-container">
        <div className="editor-panel">
          <h2>Markdown 编辑器</h2>
          <textarea
            value={markdown}
            onChange={e => setMarkdown(e.target.value)}
            className="markdown-editor"
            placeholder="在此输入 Markdown..."
          />
        </div>
        <div className="preview-panel">
          <div className="preview-header">
            <h2>实时预览</h2>
            <div className="theme-selector">
              <label>主题：</label>
              <select value={theme} onChange={e => setTheme(e.target.value as ThemeOption)}>
                <option value="none">无主题</option>
                <option value="tailwind">Tailwind CSS</option>
                <option value="minimal">极简主题</option>
              </select>
            </div>
          </div>
          <div className="markdown-preview">
            <Supramark
              markdown={markdown}
              theme={theme === 'none' ? undefined : theme}
              containerRenderers={{ weather: renderWeatherContainerWeb }}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;
