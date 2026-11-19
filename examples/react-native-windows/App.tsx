import React, { useRef, useState } from 'react';
import {
  findNodeHandle, NativeModules, StyleSheet, Text, UIManager, View, type NativeSyntheticEvent,
} from 'react-native';
import { Supramark } from '@supramark/rn';
import type { SupramarkSelectionHandle } from '@supramark/rn';
import type { SupramarkRootNode } from '@supramark/core';

const first = 'Drag across these words in either direction. Compare narrow iii and wide WWW.';
const second =
  'Continue into the next paragraph: caf\u00e9, \u4e2d\u6587 and \u{1f600}. Resize the window to check wrapping.';
const markdown = first + '\n\n**' + second + '**';
// Use the renderer's parsed-AST entry point so this demo can run without a native parser DLL.
const ast: SupramarkRootNode = {
  type: 'root',
  ast_version: 2,
  diagnostics: [],
  children: [
    { type: 'paragraph', children: [{ type: 'text', value: first }] },
    {
      type: 'paragraph',
      children: [{ type: 'strong', children: [{ type: 'text', value: second }] }],
    },
  ],
};

export default function App() {
  const [selected, setSelected] = useState('');
  const surface = useRef<View>(null);
  const selection = useRef<SupramarkSelectionHandle | null>(null);
  const hasRange = useRef(false);
  const keyboard = {
    onKeyDown(event: NativeSyntheticEvent<{
      key: string; ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean;
    }>) {
      const key = event.nativeEvent;
      if (!key.ctrlKey || key.shiftKey || key.altKey || key.metaKey || key.key.toLowerCase() !== 'c') return;
      const text = selection.current?.getSelectedText();
      if (text) NativeModules.Clipboard.setString(text);
    },
  };
  return (
    <View
      ref={surface}
      focusable
      style={styles.screen}
      {...keyboard}
      onPointerDown={event => {
        // The document has already handled an inside press. Toolbar buttons stop bubbling here.
        if (hasRange.current && event.nativeEvent.pointerType === 'mouse' && event.nativeEvent.button === 0) {
          selection.current?.clearSelection();
        }
      }}
      onPointerUp={event => {
        if (!hasRange.current || event.nativeEvent.pointerType !== 'mouse' || event.nativeEvent.button !== 0) return;
        // RNW 0.84 exposes the Fabric focus command even when imperative View.focus is disabled.
        const tag = findNodeHandle(surface.current);
        if (tag !== null) UIManager.dispatchViewManagerCommand(tag, 'focus', []);
      }}
    >
      <Text style={styles.heading}>Text selection</Text>
      <Supramark
        ast={ast}
        markdown={markdown}
        selection={{
          documentId: 'windows-example',
          onHandleReady: handle => { selection.current = handle; },
          onSelectionChange: range => { hasRange.current = range !== null; },
          toolbarItems: [{ id: 'inspect', title: 'Show selection', format: 'plainText' }],
          onAction: request => setSelected(request.text),
        }}
      />
      <Text style={styles.label}>Last selected text</Text>
      <Text>{selected}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, padding: 24 },
  heading: { fontSize: 24, fontWeight: '600', marginBottom: 18 },
  label: { marginTop: 20, fontWeight: '600' },
});
