/**
 * Translate a conformance case's upstream parse options into the JSON payload
 * accepted by the `supramark-markdown --options` CLI flag.
 *
 * The defaults are source-dependent, because micromark and CommonMark disagree
 * about raw HTML:
 *   - micromark escapes raw HTML by default (`allowDangerousHtml` defaults off);
 *   - CommonMark / cmark-gfm pass raw HTML through verbatim.
 * For micromark cases we therefore default `allowDangerousHtml` to false and let
 * an explicit `upstreamOptions.allowDangerousHtml === true` opt back in. For every
 * other source we default to true (the parser's own default profile), so wiring
 * this in changes nothing for the commonmark/cmark-gfm harnesses.
 *
 * `disable` is micromark's construct-disable map. micromark accepts it in two
 * places and the test fixtures use both: top-level (`{disable: {null: [...]}}`)
 * and nested inside a syntax extension (`{extensions: [{disable: {null: [...]}}]}`).
 * Each value is a phase→names map where the `null` phase serialises to the string
 * `"null"`; we collect from both places and flatten every phase into one list.
 * Unknown names are harmless: the parser ignores them.
 */

export function parserOptionsForCase(testCase) {
  const upstream = testCase.input?.upstreamOptions;
  const isMicromark = testCase.source?.name === 'micromark';
  const allowDangerousHtml = isMicromark
    ? upstream?.allowDangerousHtml === true
    : true;
  const options = {
    allowDangerousHtml,
    disable: collectDisable(upstream),
  };
  // micromark's CommonMark profile has no GFM bare-URL/email autolink-literal
  // extension, so bare `http://`, `www.`, and `foo@bar` must stay literal.
  // Supramark ships that extension ON by default (#144 measures exactly that for
  // the commonmark/cmark-gfm gates). For an apples-to-apples micromark comparison
  // we turn it off here — scoped to micromark, so #144's default-config stance is
  // untouched elsewhere. This cannot regress a micromark case: micromark never
  // linkifies a bare URL, and angle-bracket autolinks are the core AutolinkScanner
  // (a separate rule), not this extension.
  if (isMicromark) {
    options.gfmAutolink = false;
  }
  return options;
}

/** `--options <json>` argv for the parser binary, ready to spread into spawnSync args. */
export function parserOptionsArgv(testCase) {
  return ['--options', JSON.stringify(parserOptionsForCase(testCase))];
}

function collectDisable(upstream) {
  if (!upstream) return [];
  const sources = [upstream.disable];
  if (Array.isArray(upstream.extensions)) {
    for (const extension of upstream.extensions) {
      sources.push(extension?.disable);
    }
  }
  return sources.flatMap(flattenDisable).filter(isString);
}

function flattenDisable(disable) {
  if (!disable) return [];
  if (Array.isArray(disable)) return disable.filter(isString);
  if (typeof disable === 'object') {
    return Object.values(disable).flat().filter(isString);
  }
  return [];
}

function isString(value) {
  return typeof value === 'string';
}
