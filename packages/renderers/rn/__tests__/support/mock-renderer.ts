import { mock } from 'bun:test';
import type { DiagramRenderResult } from '@supramark/engines';

// Shared renderer-level mocks: a react-native-svg host stub and a controllable
// @supramark/engines/rn adapter. bun's mock.module registry is process-wide, so
// test files must not each register their own engine mock — a second registration
// clobbers the first and the file that registered first loses control of
// pendingResolve (which is exactly how list.test broke DiagramNode.test). Import
// this module from every test that pulls in DiagramNode (directly or via Supramark)
// so the shared engineState stays the single source of truth.
export const engineState = {
  renderCalls: 0,
  pendingResolve: null as null | ((result: DiagramRenderResult) => void),
};

// Keep this surface identical to the one in mock-react-native.ts: both register
// process-wide, so whichever loads last wins and a mismatch would drop exports
// the other file's tests depend on (Svg / Path back the code-block icons).
mock.module('react-native-svg', () => ({
  SvgXml: 'SvgXml',
  Svg: 'Svg',
  Path: 'Path',
}));

mock.module('@supramark/engines/rn', () => ({
  createReactNativeDiagramEngine: () => ({
    render: () => {
      engineState.renderCalls += 1;
      return new Promise<DiagramRenderResult>(resolve => {
        engineState.pendingResolve = resolve;
      });
    },
  }),
}));
