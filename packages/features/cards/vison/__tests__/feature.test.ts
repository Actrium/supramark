import { describe, expect, it } from 'bun:test';
import { parse, validateFeature } from '@supramark/core';
import {
  createVisonFeatureConfig,
  visonExamples,
  visonFeature,
  type SupramarkVisonContainerNode,
} from '../src/index';

describe('Vison Feature', () => {
  it('has valid feature metadata', () => {
    const result = validateFeature(visonFeature);

    expect(result.valid).toBe(true);
    expect(result.errors).toHaveLength(0);
    expect(visonFeature.metadata.id).toBe('@supramark/feature-card-vison');
    expect(visonFeature.metadata.syntaxFamily).toBe('container');
  });

  it('provides examples', () => {
    expect(visonExamples.length).toBeGreaterThan(0);
    expect(visonExamples[0]?.markdown).toContain(':::vison');
  });

  it('parses a valid Vison container into an opaque container node', async () => {
    const ast = await parse(
      [':::vison', '{ "version": "1", "type": "text", "props": { "text": "hi" } }', ':::'].join(
        '\n'
      ),
      {
        config: {
          features: [createVisonFeatureConfig()],
        },
      }
    );

    const node = ast.children[0] as SupramarkVisonContainerNode;

    expect(node.type).toBe('container');
    expect(node.name).toBe('vison');
    expect(node.children).toEqual([]);
    expect(node.data.source).toContain('"type": "text"');
    expect(node.data.parseError).toBeUndefined();
    expect(node.data.spec?.type).toBe('text');
  });

  it('keeps parse errors on invalid JSON bodies', async () => {
    const ast = await parse([':::vison', '{ invalid json', ':::'].join('\n'), {
      config: {
        features: [createVisonFeatureConfig()],
      },
    });

    const node = ast.children[0] as SupramarkVisonContainerNode;

    expect(node.type).toBe('container');
    expect(node.name).toBe('vison');
    expect(node.data.spec).toBeUndefined();
    expect(node.data.source).toContain('invalid json');
    expect(node.data.parseError).toBeDefined();
  });
});
