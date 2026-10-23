import { describe, expect, mock, test } from 'bun:test';
import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

import './support/mock-react-native';

// react-test-renderer only flushes synchronously when the act environment is on.
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT =
  true;

// Dynamic import: the react-native-svg stand-in above must be registered before
// `svgAst` loads its parser from the package's internal XML module.
const { SvgXmlDecoded, decodeSvgTextEntities } = await import('../src/svgAst');

describe('decodeSvgTextEntities', () => {
  test('decodes the references the SVG emitters produce', () => {
    expect(decodeSvgTextEntities('i&lt;n')).toBe('i<n');
    expect(decodeSvgTextEntities('a &amp; b &gt; c')).toBe('a & b > c');
    expect(decodeSvgTextEntities('&quot;q&quot; &apos;a&apos; &#39;b&#39;')).toBe('"q" \'a\' \'b\'');
    expect(decodeSvgTextEntities('&nbsp;')).toBe(' ');
  });

  test('decodes every reference in one run', () => {
    expect(decodeSvgTextEntities('&lt;a&gt; &amp; &lt;b&gt;')).toBe('<a> & <b>');
  });

  test('leaves references it does not know untouched', () => {
    expect(decodeSvgTextEntities('&unknown; &copy; &#x3C;')).toBe('&unknown; &copy; &#x3C;');
  });
});

describe('SvgXmlDecoded', () => {
  test('decodes text inside nested label elements', async () => {
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<SvgXmlDecoded xml={'i&lt;n&lt;br/&gt;loop'} width="100%" height="100%" />);
    });
    const tree = renderer.toJSON() as { type: string; props: Record<string, unknown> } | null;

    expect(tree?.type).toBe('SvgAst');
    // <svg><text><tspan>… — the decoder has to walk the element chain, not
    // only the root's direct children (the real parser nests label text).
    const ast = tree?.props.ast as { children: { children: { children: unknown[] }[] }[] };
    expect(ast.children[0].children[0].children).toEqual(['i<n<br/>loop']);
    // Host width/height reach the root <svg> through `SvgAst`'s override.
    expect(tree?.props.override).toMatchObject({ width: '100%', height: '100%' });
  });

  test('drops only this diagram and reports a parse failure', async () => {
    const onError = mock(() => undefined);
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(
        <SvgXmlDecoded xml="__parse_error__" onError={onError} fallback="fallback" />
      );
    });

    expect(renderer.toJSON()).toBe('fallback');
    expect(onError).toHaveBeenCalledTimes(1);
  });

  test('renders nothing for a null document', async () => {
    let renderer!: ReactTestRenderer;
    await act(async () => {
      renderer = create(<SvgXmlDecoded xml={null} width="100%" height="100%" />);
    });
    expect(renderer.toJSON()).toBeNull();
  });
});