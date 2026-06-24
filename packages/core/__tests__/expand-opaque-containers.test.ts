import { parse, expandOpaqueContainers } from '../src/plugin';
import type {
  SupramarkNode,
  SupramarkRootNode,
  SupramarkParentNode,
  SupramarkContainerNode,
} from '../src/ast';

// Test-local view of a node that may carry children, used to walk an arbitrary tree.
type NodeWithChildren = SupramarkNode & { children?: SupramarkNode[] };

function findContainers(
  node: SupramarkNode,
  out: SupramarkContainerNode[] = []
): SupramarkContainerNode[] {
  const n = node as NodeWithChildren;
  if (n && typeof n === 'object') {
    if (n.type === 'container') out.push(n as SupramarkContainerNode);
    for (const child of n.children ?? []) findContainers(child, out);
  }
  return out;
}

function hasNodeOfType(node: SupramarkNode, type: string): boolean {
  if (!node || typeof node !== 'object') return false;
  if (node.type === type) return true;
  const children = (node as NodeWithChildren).children ?? [];
  return children.some((c: SupramarkNode) => hasNodeOfType(c, type));
}

describe('expandOpaqueContainers', () => {
  describe('via parse() (integration)', () => {
    it('expands a transparent container (no data) by re-parsing its markdown body', async () => {
      const root = await parse(':::note Title\nhello **bold** world\n:::\n');
      const [note] = findContainers(root);

      expect(note).toBeDefined();
      expect(note.name).toBe('note');
      // Body was raw markdown on `value`; it is now parsed into children and
      // `value` is cleared.
      expect(note.value).toBeUndefined();
      expect(note.children.length).toBeGreaterThan(0);
      expect(note.children[0].type).toBe('paragraph');
      // The inline emphasis survived the round-trip.
      expect(hasNodeOfType(note, 'strong')).toBe(true);
    });

    it('leaves a genuinely-opaque container (carries data) untouched', async () => {
      const root = await parse(':::map\ncenter: [1, 2]\nzoom: 5\n:::\n');
      const [map] = findContainers(root);

      expect(map).toBeDefined();
      expect(map.name).toBe('map');
      // The Rust parser populated structured data; the raw value must be kept
      // verbatim and the body must NOT be parsed as markdown.
      expect(map.data).toBeDefined();
      expect(typeof map.value).toBe('string');
      expect(map.value).toContain('center');
      expect(map.children.length).toBe(0);
    });

  });

  describe('discriminator and idempotency (unit)', () => {
    function makeRoot(children: SupramarkNode[]): SupramarkRootNode {
      return { type: 'root', children } as unknown as SupramarkRootNode;
    }

    it('expands an opaque container with no data and a markdown value', async () => {
      const root = makeRoot([
        { type: 'container', name: 'note', mode: 'opaque', value: '# Heading', children: [] },
      ]);
      await expandOpaqueContainers(root);
      const note = root.children[0] as SupramarkContainerNode;

      expect(note.value).toBeUndefined();
      expect(hasNodeOfType(note, 'heading')).toBe(true);
    });

    it('never touches an opaque container that carries data', async () => {
      const root = makeRoot([
        {
          type: 'container',
          name: 'html',
          mode: 'opaque',
          value: '<b>raw</b>',
          data: { html: '<b>raw</b>' },
          children: [],
        },
      ]);
      await expandOpaqueContainers(root);
      const html = root.children[0] as SupramarkContainerNode;

      // data-bearing container is left byte-for-byte intact.
      expect(html.value).toBe('<b>raw</b>');
      expect(html.children.length).toBe(0);
    });

    it('recurses into nested parents to expand containers at any depth', async () => {
      // The opaque container is buried inside a blockquote, not at the top
      // level — the walk must descend into arbitrary parents to reach it.
      const root = makeRoot([
        {
          type: 'blockquote',
          children: [
            { type: 'container', name: 'note', mode: 'opaque', value: 'deep **x**', children: [] },
          ],
        },
      ]);
      await expandOpaqueContainers(root);
      const note = (root.children[0] as SupramarkParentNode).children[0] as SupramarkContainerNode;

      expect(note.value).toBeUndefined();
      expect(hasNodeOfType(note, 'strong')).toBe(true);
    });

    it('is idempotent: a second pass over an expanded tree changes nothing', async () => {
      const root = makeRoot([
        { type: 'container', name: 'note', mode: 'opaque', value: 'plain *text*', children: [] },
      ]);
      await expandOpaqueContainers(root);
      const afterFirst = JSON.stringify(root);
      await expandOpaqueContainers(root);
      const afterSecond = JSON.stringify(root);

      expect(afterSecond).toBe(afterFirst);
    });
  });
});
