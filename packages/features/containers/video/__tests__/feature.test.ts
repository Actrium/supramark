import { videoFeature, VIDEO_CONTAINER_NAMES } from '../src/feature';
import { videoExamples } from '../src/examples';
import { validateContainerFeature, parse } from '@supramark/core';
import type { SupramarkNode, SupramarkContainerNode, SupramarkRootNode } from '@supramark/core';

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

describe('Video Feature', () => {
  describe('ContainerFeature shape', () => {
    it('should have valid container feature definition', () => {
      const result = validateContainerFeature(videoFeature);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('should have correct id', () => {
      expect(videoFeature.id).toBe('@supramark/feature-video');
    });

    it('should have semantic version', () => {
      expect(videoFeature.version).toMatch(/^\d+\.\d+\.\d+$/);
    });

    it('should expose the video container name', () => {
      expect(videoFeature.containerNames).toEqual([...VIDEO_CONTAINER_NAMES]);
    });

    it('should provide parser registration and renderer exports', () => {
      expect(typeof videoFeature.registerParser).toBe('function');
      expect(videoFeature.webRendererExport).toBe('renderVideoContainerWeb');
      expect(videoFeature.rnRendererExport).toBe('renderVideoContainerRN');
    });
  });

  describe('parse() integration', () => {
    it('parses a JSON body into structured video data', async () => {
      const root: SupramarkRootNode = await parse(
        ':::video\n{\n  "src": "https://example.com/demo.mp4",\n  "poster": "https://example.com/cover.jpg",\n  "title": "Product demo",\n  "autoplay": true,\n  "muted": true,\n  "loop": false,\n  "controls": true,\n  "width": 80\n}\n:::\n'
      );
      const [video] = findContainers(root);

      expect(video).toBeDefined();
      expect(video.name).toBe('video');
      expect(video.data?.src).toBe('https://example.com/demo.mp4');
      expect(video.data?.poster).toBe('https://example.com/cover.jpg');
      expect(video.data?.title).toBe('Product demo');
      expect(video.data?.autoplay).toBe(true);
      expect(video.data?.muted).toBe(true);
      expect(video.data?.loop).toBe(false);
      expect(video.data?.controls).toBe(true);
      expect(video.data?.width).toBe(80);
    });

    it('keeps the raw body on value and does not parse markdown children', async () => {
      const root: SupramarkRootNode = await parse(
        ':::video\n{"src": "https://example.com/demo.mp4"}\n:::\n'
      );
      const [video] = findContainers(root);

      expect(video.children).toHaveLength(0);
      expect(typeof video.value).toBe('string');
    });

    it('drops unknown config fields', async () => {
      const root: SupramarkRootNode = await parse(
        ':::video\n{"src": "https://example.com/a.mp4", "unknown": "dropped"}\n:::\n'
      );
      const [video] = findContainers(root);

      expect(video.data?.src).toBe('https://example.com/a.mp4');
      expect(video.data).not.toHaveProperty('unknown');
    });

    it('does not require src at parse time', async () => {
      const root: SupramarkRootNode = await parse(':::video\n{"title": "No src yet"}\n:::\n');
      const [video] = findContainers(root);

      expect(video.data?.src).toBeUndefined();
      expect(video.data?.title).toBe('No src yet');
    });

    it('reports parseError with rawConfig for invalid JSON', async () => {
      const root: SupramarkRootNode = await parse(':::video\n{invalid json}\n:::\n');
      const [video] = findContainers(root);

      expect(typeof video.data?.parseError).toBe('string');
      expect(video.data?.rawConfig).toBe('{invalid json}');
    });

    it('reports parseError for a non-object body', async () => {
      const root: SupramarkRootNode = await parse(':::video\n["not", "an", "object"]\n:::\n');
      const [video] = findContainers(root);

      expect(video.data?.parseError).toBe('video JSON config must be an object');
    });
  });

  describe('examples', () => {
    it('should have at least one example', () => {
      expect(videoExamples.length).toBeGreaterThan(0);
    });

    it('every example markdown contains a :::video block', () => {
      for (const example of videoExamples) {
        expect(example.markdown).toContain(':::video');
        expect(example.markdown).toContain(':::');
      }
    });
  });
});
