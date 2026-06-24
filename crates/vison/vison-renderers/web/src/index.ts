// Re-export the upstream renderer + shared types so consumers `import
// { VisonWebRenderer } from '@actrium/vison-web'` instead of pointing
// at the .tsx file directly.
//
// The actual implementation lives one level up at
// `../VisonWebRenderer.tsx` to keep the upstream file path unchanged.
// When this package's patches land upstream, the file is expected to
// move to `src/` proper.
export { VisonWebRenderer } from '../VisonWebRenderer';
export type { VisonComponent, ComponentType } from '../../shared/types';
