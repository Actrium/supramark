import { useState } from 'react';
import { Supramark } from '@supramark/web/client';

// Register container hooks (must be imported before using)
import { renderWeatherContainerWeb } from '@supramark/feature-weather';
import { renderVideoContainerWeb } from '@supramark/feature-video';

import './App.css';

const INITIAL_MARKDOWN = `# Supramark Live Editor

Welcome to the **Supramark** live Markdown editor!

## Features

### GFM extensions

- **Bold text**
- *Italic text*
- \`Inline code\`
- ~~Strikethrough~~

### Task list

- [x] GFM task list support
- [x] Live preview
- [x] Theme switching
- [ ] More features in progress

### Table example

| Feature | Status | Notes |
| --- | :---: | ---: |
| Strikethrough | ✅ | Uses \`~~\` syntax |
| Task list | ✅ | \`[ ]\` and \`[x]\` |
| Table | ✅ | Standard GFM table |
| Theme system | ✅ | Supports custom className |

### Code block

\`\`\`javascript
function hello(name) {
  console.log('Hello, ' + name);
}

hello('Supramark');
\`\`\`

### Links and images

This is a [link example](https://github.com)

### Weather card

:::weather
location: Shanghai
condition: Cloudy
tempC: 22
:::

### Video embed

:::video
{
  "src": "https://interactive-examples.mdn.mozilla.net/cc0-videos/flower.mp4",
  "title": "CC0 flower video"
}
:::

### Mermaid diagram

\`\`\`mermaid
graph TD
    A[Start] --> B{Do you like Supramark?}
    B -->|Yes| C[Keep using it]
    B -->|No| D[Give it another try]
    C --> E[Enjoy writing Markdown]
    D --> B
\`\`\`

---

Edit Markdown on the left, switch themes in the top right, and watch the live preview update on the right!
`.trim();

type ThemeOption = 'none' | 'tailwind' | 'minimal';

function App() {
  const [markdown, setMarkdown] = useState(INITIAL_MARKDOWN);
  const [theme, setTheme] = useState<ThemeOption>('tailwind');

  return (
    <div className="app-container">
      <header className="app-header">
        <h1>Supramark Live Editor</h1>
        <p>Live Markdown Editor - CSR Example</p>
      </header>
      <div className="editor-container">
        <div className="editor-panel">
          <h2>Markdown Editor</h2>
          <textarea
            value={markdown}
            onChange={e => setMarkdown(e.target.value)}
            className="markdown-editor"
            placeholder="Type Markdown here..."
          />
        </div>
        <div className="preview-panel">
          <div className="preview-header">
            <h2>Live Preview</h2>
            <div className="theme-selector">
              <label>Theme:</label>
              <select value={theme} onChange={e => setTheme(e.target.value as ThemeOption)}>
                <option value="none">No theme</option>
                <option value="tailwind">Tailwind CSS</option>
                <option value="minimal">Minimal</option>
              </select>
            </div>
          </div>
          <div className="markdown-preview">
            <Supramark
              markdown={markdown}
              theme={theme === 'none' ? undefined : theme}
              containerRenderers={{ weather: renderWeatherContainerWeb, video: renderVideoContainerWeb }}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;
