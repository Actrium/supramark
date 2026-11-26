import { mock } from 'bun:test';
import type { DiagramRenderResult } from '@supramark/engines';

// Bun's mock registry is process-wide. Tests that load DiagramNode, including
// through Supramark, share this engine mock and its pending render state.
export const engineState = {
  renderCalls: 0,
  pendingResolve: null as null | ((result: DiagramRenderResult) => void),
};

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
