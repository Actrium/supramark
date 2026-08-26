import { wikilinkFeature } from '../src/feature';
import { validateFeature } from '@supramark/core';
import type { SupramarkNode } from '@supramark/core';

describe('WikiLink Feature', () => {
  describe('Metadata', () => {
    it('should have valid metadata', () => {
      const result = validateFeature(wikilinkFeature);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('should have correct id', () => {
      expect(wikilinkFeature.metadata.id).toMatch(/^@[\w-]+\/feature-[\w-]+$/);
    });

    it('should have semantic version', () => {
      expect(wikilinkFeature.metadata.version).toMatch(/^\d+\.\d+\.\d+$/);
    });
  });

  describe('Syntax', () => {
    it('should define AST node type', () => {
      expect(wikilinkFeature.syntax.ast.type).toBe('wiki_link');
    });

    it('selector should match wiki_link nodes only', () => {
      const { selector } = wikilinkFeature.syntax.ast;
      expect(selector).toBeDefined();

      const match = selector!({ type: 'wiki_link', target: 'a' } as SupramarkNode);
      const textMatch = selector!({ type: 'text', value: '[[a]]' } as SupramarkNode);

      expect(match).toBe(true);
      expect(textMatch).toBe(false);
    });
  });
});
