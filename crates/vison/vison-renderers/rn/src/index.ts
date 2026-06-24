// Re-export the upstream renderer + shared types so consumers `import
// { VisonRNRenderer } from '@actrium/vison-rn'` instead of pointing
// at the .tsx file directly.
//
// The actual implementation lives one level up at
// `../VisonRNRenderer.tsx` to keep the upstream file path unchanged.
// When this package's patches land upstream, the file is expected to
// move to `src/` proper.
export { VisonRNRenderer } from '../VisonRNRenderer';
export type { VisonComponent, ComponentType } from '../../shared/types';
