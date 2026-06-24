export type ComponentType = 'container' | 'text' | 'image' | 'markdown' | 'divider';

export interface VisonComponent {
  version?: string;
  type: ComponentType;
  props?: Record<string, unknown>;
  style?: Record<string, unknown>;
  children?: VisonComponent[];
}
