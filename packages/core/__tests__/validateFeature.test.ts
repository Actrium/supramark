/**
 * validateFeature 函数测试
 */

import { validateFeature } from '../src/feature';

describe('validateFeature', () => {
  describe('基本验证', () => {
    it('应该通过完整的 Feature 定义', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
          author: 'Test Author',
          description: 'Test description',
          license: 'Apache-2.0',
          tags: ['test'],
        },
        syntax: {
          ast: {
            type: 'test_node',
            interface: {
              required: ['type', 'value'],
              fields: {
                type: { type: 'string', description: 'Node type' },
                value: { type: 'string', description: 'Node value' },
              },
            },
            examples: [
              {
                type: 'test_node',
                value: 'test',
              },
            ],
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0]);
      expect(result.valid).toBe(true);
      expect(result.errors.filter(e => e.severity === 'error')).toHaveLength(0);
    });

    it('应该检测到缺少 id', () => {
      const feature = {
        metadata: {
          name: 'Test Feature',
          version: '1.0.0',
        },
        syntax: {
          ast: {
            type: 'test_node',
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0]);
      expect(result.valid).toBe(false);
      expect(result.errors.some(e => e.code === 'metadata-id-required')).toBe(true);
    });

    it('应该检测到 id 格式错误', () => {
      const feature = {
        metadata: {
          id: 'invalid-id',
          name: 'Test Feature',
          version: '1.0.0',
        },
        syntax: {
          ast: {
            type: 'test_node',
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0]);
      expect(result.valid).toBe(false);
      expect(result.errors.some(e => e.code === 'metadata-id-format')).toBe(true);
    });

    it('应该检测到版本号格式错误', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0',
        },
        syntax: {
          ast: {
            type: 'test_node',
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0]);
      expect(result.valid).toBe(false);
      expect(result.errors.some(e => e.code === 'metadata-version-semver')).toBe(true);
    });

    it('应该检测到缺少 AST type', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
        },
        syntax: {
          ast: {},
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0]);
      expect(result.valid).toBe(false);
      expect(result.errors.some(e => e.code === 'ast-type-required')).toBe(true);
    });
  });

  describe('警告检查', () => {
    it('应该警告缺少 description', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
        },
        syntax: {
          ast: {
            type: 'test_node',
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0]);
      expect(result.errors.some(e => e.code === 'metadata-description-required')).toBe(true);
      expect(result.errors.find(e => e.code === 'metadata-description-required')?.severity).toBe(
        'warning'
      );
    });

    it('应该警告 required 只包含 type', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
        },
        syntax: {
          ast: {
            type: 'test_node',
            interface: {
              required: ['type'],
              fields: {
                type: { type: 'string', description: 'Node type' },
              },
            },
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0]);
      expect(result.errors.some(e => e.code === 'ast-interface-required-nonempty')).toBe(true);
    });

    it('应该警告 fields 缺少定义', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
        },
        syntax: {
          ast: {
            type: 'test_node',
            interface: {
              required: ['type', 'value'],
              fields: {
                type: { type: 'string', description: 'Node type' },
                // 缺少 value 字段定义
              },
            },
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0]);
      expect(result.errors.some(e => e.code === 'ast-interface-fields-defined')).toBe(true);
    });
  });

  describe('建议检查', () => {
    it('应该建议添加 tags', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
        },
        syntax: {
          ast: {
            type: 'test_node',
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0]);
      expect(result.errors.some(e => e.code === 'metadata-tags-nonempty')).toBe(true);
      expect(result.errors.find(e => e.code === 'metadata-tags-nonempty')?.severity).toBe('info');
    });

    it('应该建议提供 examples', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
        },
        syntax: {
          ast: {
            type: 'test_node',
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0]);
      expect(result.errors.some(e => e.code === 'ast-examples-provided')).toBe(true);
      expect(result.errors.find(e => e.code === 'ast-examples-provided')?.severity).toBe('info');
    });
  });

  describe('严格模式', () => {
    it('严格模式下警告应该导致验证失败', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
          // 缺少 description (warning)
        },
        syntax: {
          ast: {
            type: 'test_node',
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0], { strict: true });
      expect(result.valid).toBe(false);
    });

    it('严格模式下建议不应该导致验证失败', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
          author: 'Test Author',
          description: 'Test description',
          license: 'Apache-2.0',
          // 缺少 tags (info)
        },
        syntax: {
          ast: {
            type: 'test_node',
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0], { strict: true });
      expect(result.valid).toBe(true);
    });
  });

  describe('生产模式', () => {
    it('生产模式下应该要求 interface', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
        },
        syntax: {
          ast: {
            type: 'test_node',
            // 缺少 interface
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0], { production: true });
      expect(result.valid).toBe(false);
      expect(result.errors.some(e => e.code === 'ast-interface-required-production')).toBe(true);
    });

    it('生产模式下应该要求至少一个渲染器', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
        },
        syntax: {
          ast: {
            type: 'test_node',
            interface: {
              required: ['type'],
              fields: {
                type: { type: 'string', description: 'Node type' },
              },
            },
          },
        },
        renderers: {},
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0], { production: true });
      expect(result.valid).toBe(false);
      expect(result.errors.some(e => e.code === 'renderers-required-production')).toBe(true);
    });

    it('生产模式下应该建议提供测试', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
        },
        syntax: {
          ast: {
            type: 'test_node',
            interface: {
              required: ['type'],
              fields: {
                type: { type: 'string', description: 'Node type' },
              },
            },
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0], { production: true });
      expect(result.errors.some(e => e.code === 'testing-recommended-production')).toBe(true);
    });
  });
});
