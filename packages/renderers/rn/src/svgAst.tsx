import React, { useEffect, useMemo } from 'react';
import type { SvgProps } from 'react-native-svg';
import { SvgAst, parse, type XmlAST } from 'react-native-svg/lib/commonjs/xml';

/**
 * Text references the SVG emitters produce. `react-native-svg`'s XML parser
 * copies text runs verbatim — it decodes no entities — so a `&lt;` paints as
 * the literal reference instead of `<`. Its CDATA branch would preserve raw
 * text, but switching the engines to CDATA would change the SVG they emit for
 * every consumer; decoding the AST text children instead leaves the markup
 * untouched and makes `<` an ordinary React text character.
 */
const TEXT_ENTITIES: Record<string, string> = {
  '&lt;': '<',
  '&gt;': '>',
  '&amp;': '&',
  '&quot;': '"',
  '&apos;': "'",
  '&#39;': "'",
  '&nbsp;': ' ',
};

const ENTITY_PATTERN = /&(?:lt|gt|amp|quot|apos|#39|nbsp);/g;

/** Decode the references an SVG text run may carry into real characters. */
export function decodeSvgTextEntities(text: string): string {
  return text.replace(ENTITY_PATTERN, entity => TEXT_ENTITIES[entity] ?? entity);
}

/** Decode every text run in the subtree, leaving the element nodes intact. */
function decodeNode(node: XmlAST): XmlAST {
  return {
    ...node,
    children: node.children.map(child =>
      typeof child === 'string' ? decodeSvgTextEntities(child) : decodeNode(child)
    ),
  };
}

export interface SvgXmlDecodedProps extends SvgProps {
  /** SVG markup; `null` renders nothing, mirroring `SvgXml`. */
  xml: string | null;
  /** Called once when the document cannot be parsed; the diagram is dropped. */
  onError?: (error: Error) => void;
  /** Rendered instead of the diagram when parsing fails. */
  fallback?: React.ReactNode;
}

/**
 * `SvgXml` with text references decoded, for the label text the engines
 * escape (`i&lt;n` must paint as `i<n`, not as the raw reference).
 *
 * `parse` and `SvgAst` come from `react-native-svg`'s own XML module rather
 * than its public entry, which re-exports `SvgXml` but not the parser it is
 * built on. A document the parser rejects drops this one diagram instead of
 * reaching the renderer's ErrorBoundary, matching `SvgXml`'s fallback scope.
 */
export function SvgXmlDecoded({ xml, onError, fallback, ...override }: SvgXmlDecodedProps) {
  const { ast, error } = useMemo(() => {
    if (xml === null) return { ast: null, error: null };
    try {
      return { ast: parse(xml, decodeNode), error: null };
    } catch (cause) {
      return {
        ast: null,
        error: cause instanceof Error ? cause : new Error(String(cause)),
      };
    }
  }, [xml]);

  useEffect(() => {
    if (error) onError?.(error);
  }, [error, onError]);

  if (!ast) return <>{fallback ?? null}</>;
  return <SvgAst ast={ast} override={override} />;
}