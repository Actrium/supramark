/**
 * Selection over real Markdown: `<Supramark selection>` mounts the self-drawn
 * selection layer over the rendered document. Long press selects a word, drag
 * extends across blocks, and the bar carries whatever actions the host lists.
 *
 * The ScrollView pauses while a selection drag owns the touch
 * (`onGestureActiveChange`), so dragging a handle never scrolls the page.
 */

import React, { useState } from 'react';
import { SafeAreaView, ScrollView, StyleSheet, Text, TouchableOpacity, View } from 'react-native';

import { Supramark, type SupramarkSelectionOptions } from '@supramark/rn';
import type { SupramarkConfig } from '@supramark/core';
import { coreMarkdownFeature } from '@supramark/feature-core-markdown';
import { gfmFeature } from '@supramark/feature-gfm';

const CONFIG: SupramarkConfig = {
  features: [
    { id: coreMarkdownFeature.metadata.id, enabled: true },
    {
      id: gfmFeature.metadata.id,
      enabled: true,
      options: { tables: true, taskListItems: true, strikethrough: false },
    },
  ],
};

// CJK sample for selection verification. Built from code points because
// check-cjk disallows CJK literals in source.
const CJK_SAMPLE = String.fromCodePoint(
  0x4e2d,
  0x6587,
  0x4e5f,
  0x53ef,
  0x4ee5,
  0x9009,
  0x4e2d,
  0x3002
);

const MARKDOWN = `# Selection over Markdown

Long press any word here to start a selection. Drag from either handle to
extend it across **bold runs**, \`inline code\` and [links](https://example.com).

## Lists

- First bullet item
- Second bullet item with **bold**
- Third bullet item

1. Ordered item one
2. Ordered item two

- [ ] Unchecked task
- [x] Checked task

- A loose item whose first paragraph
  continues on a second line.

  And a second paragraph inside the same item.

## Code and tables

\`\`\`ts
const answer = 42;
function greet(name: string) {
  return \`hello \${name}\`;
}
\`\`\`

| Format   | Copy result     |
| -------- | --------------- |
| Plain    | text only       |
| Markdown | syntax restored |

> A blockquote paragraph that is also selectable.

---

Paragraph after a thematic break, so cross-block selection has something to
span. The quick brown fox jumps over the lazy dog. ${CJK_SAMPLE}
`;

export default function SelectionMarkdownDemo({ onBack }: { onBack: () => void }) {
  const [status, setStatus] = useState('idle');
  const [scrollLocked, setScrollLocked] = useState(false);

  const selection: SupramarkSelectionOptions = {
    toolbarItems: [
      { id: 'copy', title: 'Copy', format: 'plainText' },
      { id: 'copy-md', title: 'Copy MD', format: 'markdown' },
      { id: 'quote', title: 'Quote' },
    ],
    onAction: request => {
      setStatus(`${request.id} (${request.format}): ${request.text.slice(0, 60)}`);
    },
    onGestureActiveChange: setScrollLocked,
  };

  return (
    <SafeAreaView style={s.root}>
      <TouchableOpacity onPress={onBack}>
        <Text style={s.back}>back</Text>
      </TouchableOpacity>
      <ScrollView scrollEnabled={!scrollLocked} contentContainerStyle={s.body}>
        <Supramark markdown={MARKDOWN} config={CONFIG} selection={selection} />
      </ScrollView>
      <View style={s.statusPanel}>
        <Text testID="selection-md-status">{status}</Text>
      </View>
    </SafeAreaView>
  );
}

const s = StyleSheet.create({
  root: { flex: 1, backgroundColor: '#fff' },
  body: { padding: 16 },
  back: { color: '#2f54eb', padding: 8 },
  statusPanel: { borderTopWidth: 1, borderTopColor: '#ddd', padding: 8 },
});