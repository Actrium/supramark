import { mock } from 'bun:test';

/** Shared spy for assertions about the renderer's Linking fallback. */
export const openUrlMock = mock(async (_url: string) => undefined);

// react-native's JS entry contains Flow syntax (import typeof) that bun cannot load,
// so tests always run against a mock. bun's mock.module registry is process-wide:
// when multiple test files each register their own mock, a later narrow surface
// clobbers an earlier wide one, and already-loaded modules then fail to resolve the
// missing exports (ActivityIndicator disappeared when DiagramNode.test and
// styles.test shared one process). Register the react-native mock here once with
// the full surface; test files import this module instead of calling
// mock.module('react-native') themselves.
// String host components are rendered by react-test-renderer as host nodes,
// which keeps testID-based assertions simple.
mock.module('react-native', () => ({
  View: 'View',
  Text: 'Text',
  Image: 'Image',
  ScrollView: 'ScrollView',
  TouchableOpacity: 'TouchableOpacity',
  Pressable: 'Pressable',
  ActivityIndicator: 'ActivityIndicator',
  Platform: { OS: 'ios', select: (spec: Record<string, unknown>) => spec.ios ?? spec.default },
  // The selection layer attaches a root PanResponder; the mock only needs the
  // shape (`panHandlers`) since tests never drive touches.
  PanResponder: { create: () => ({ panHandlers: {} }) },
  Dimensions: { get: () => ({ width: 375, height: 812 }) },
  UIManager: {
    measureInWindow: (
      _tag: number,
      callback: (x: number, y: number, w: number, h: number) => void
    ) => callback(0, 0, 100, 80),
  },
  Linking: { openURL: openUrlMock },
  StyleSheet: {
    create: (s: unknown) => s,
    // Match React Native's production overlay primitive so tests verify layout.
    absoluteFillObject: { position: 'absolute', top: 0, right: 0, bottom: 0, left: 0 },
  },
}));

// Share the react-native-svg mock across the renderer tests. The nested AST
// matches a diagram label so the decoder visits nested text nodes.
mock.module('react-native-svg', () => ({
  SvgXml: 'SvgXml',
  SvgAst: 'SvgAst',
  parse: (xml: string, middleware?: (ast: unknown) => unknown) => {
    if (xml === '__parse_error__') throw new Error('bad xml');
    const tspan = { tag: 'tspan', props: {}, children: [xml], parent: null, Tag: 'TSpan' };
    const text = { tag: 'text', props: {}, children: [tspan], parent: null, Tag: 'Text' };
    const root = { tag: 'svg', props: {}, children: [text], parent: null, Tag: 'Svg' };
    return middleware ? middleware(root) : root;
  },
}));
