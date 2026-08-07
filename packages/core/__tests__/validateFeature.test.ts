/**
 * Tests for the validateFeature function
 */

import { validateFeature } from '../src/feature';

describe('validateFeature', () => {
  describe('basic validation', () => {
    it('passes for a complete Feature definition', () => {
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

    it('detects a missing id', () => {
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

    it('detects a malformed id', () => {
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

    it('detects a malformed version number', () => {
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

    it('detects a missing AST type', () => {
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

  describe('warning checks', () => {
    it('warns about a missing description', () => {
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

    it('warns when required contains only type', () => {
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

    it('warns when fields is missing a definition', () => {
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
                // missing the value field definition
              },
            },
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0]);
      expect(result.errors.some(e => e.code === 'ast-interface-fields-defined')).toBe(true);
    });

    it('warns when renderers is missing or has no platform renderer', () => {
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
        // renderers key omitted entirely
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0]);
      expect(result.errors.some(e => e.code === 'renderers-required')).toBe(true);
      // basic mode treats it as a warning, not a failure
      expect(result.valid).toBe(true);
    });
  });

  describe('info-level checks', () => {
    it('suggests adding tags', () => {
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

    it('suggests providing examples', () => {
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

  describe('strict mode', () => {
    it('fails validation on a warning in strict mode', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
          // missing description (warning)
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

    it('does not fail validation on an info-level item in strict mode', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
          author: 'Test Author',
          description: 'Test description',
          license: 'Apache-2.0',
          // missing tags (info)
        },
        syntax: {
          ast: {
            type: 'test_node',
          },
        },
        renderers: { rn: { Component: () => null } },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0], { strict: true });
      expect(result.valid).toBe(true);
    });
  });

  describe('production mode', () => {
    it('requires interface in production mode', () => {
      const feature = {
        metadata: {
          id: '@supramark/feature-test',
          name: 'Test Feature',
          version: '1.0.0',
        },
        syntax: {
          ast: {
            type: 'test_node',
            // missing interface
          },
        },
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0], { production: true });
      expect(result.valid).toBe(false);
      expect(result.errors.some(e => e.code === 'ast-interface-required-production')).toBe(true);
    });

    it('requires at least one renderer in production mode', () => {
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

    it('fails production mode when renderers is omitted entirely', () => {
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
        // renderers key omitted entirely
      };

      const result = validateFeature(feature as unknown as Parameters<typeof validateFeature>[0], { production: true });
      expect(result.valid).toBe(false);
      expect(result.errors.some(e => e.code === 'renderers-required-production')).toBe(true);
    });

    it('suggests providing tests in production mode', () => {
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
