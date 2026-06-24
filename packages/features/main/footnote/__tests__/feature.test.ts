import { footnoteFeature } from '../src/feature';
import { validateFeature } from '@supramark/core';
import type { SupramarkNode } from '@supramark/core';

describe('Footnote Feature', () => {
  describe('Metadata', () => {
    it('should have valid metadata', () => {
      const result = validateFeature(footnoteFeature);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('should have correct id', () => {
      expect(footnoteFeature.metadata.id).toMatch(/^@[\w-]+\/feature-[\w-]+$/);
    });

    it('should have semantic version', () => {
      expect(footnoteFeature.metadata.version).toMatch(/^\d+\.\d+\.\d+$/);
    });
  });

  describe('Syntax', () => {
    it('should define AST node type', () => {
      expect(footnoteFeature.syntax.ast.type).toBeDefined();
      expect(typeof footnoteFeature.syntax.ast.type).toBe('string');
    });

    it('selector should match both reference and definition nodes', () => {
      const { selector } = footnoteFeature.syntax.ast;
      expect(selector).toBeDefined();

      const refMatch = selector!({
        type: 'footnote_reference',
        index: 1,
      } as unknown as SupramarkNode);

      const defMatch = selector!({
        type: 'footnote_definition',
        index: 1,
        children: [],
      } as unknown as SupramarkNode);

      expect(refMatch).toBe(true);
      expect(defMatch).toBe(true);
    });
  });

  // TODO: 添加渲染测试
  // TODO: 添加集成测试
});
