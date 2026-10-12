// cmark-gfm's test file (test/extensions.txt) marks a few crash-safety edge
// cases with an `<IGNORE>` sentinel as the expected HTML, and test/spec_tests.py
// auto-passes them without comparing. The sentinel is not a real rendering
// target — but the cmark-gfm 0.29.0.gfm.13 *binary* (commit 587a12bb) does
// produce real HTML for these inputs, and that real output is what we want the
// conformance gate to compare Supramark against. CI has no cmark binary, so the
// real binary output is captured here as a per-case override.
//
// Each entry replaces both `expected.html` and `expected.semanticTypes` for the
// named case (the fixture's own fields are derived from `<IGNORE>` and are not
// meaningful). Values were produced by piping the case's exact input through
// `/tmp/cmark-gfm-src/build/src/cmark-gfm` (587a12bb) and running the result
// through lib/semantic/html-semantics.mjs.
const IGNORE_BINARY_OVERRIDES = {
  'cmark-gfm-0.29.0.gfm.13-extensions-0020': {
    html: "<p>This shouldn't crash everything: (<em>A</em>@_.A</p>",
    semanticTypes: ['paragraph', 'text', 'emphasis'],
  },
};

// Returns the effective expected HTML for a case, substituting the cmark
// binary's real output for `<IGNORE>` sentinel cases when an override exists.
// Callers that also need semanticTypes should use effectiveExpected().
export function effectiveExpectedHtml(testCase) {
  if (testCase.expected.html.trim() === '<IGNORE>') {
    const override = IGNORE_BINARY_OVERRIDES[testCase.id];
    if (override) return override.html;
  }
  return testCase.expected.html;
}

// Cases where Supramark intentionally diverges from the reference HTML for a
// documented reason (e.g. a security sanitization the reference omits). The
// conformance gate records these as `divergence` (not `pass` and not `fail`)
// so they remain visible without counting as a regression.
const INTENTIONAL_DIVERGENCES = {
  'micromark-4.0.2-text-image-0033': {
    reason:
      'javascript: URL in <img src> is sanitized to empty (XSS protection). Supramark neutralizes dangerous protocols on img src regardless of allowDangerousHtml; micromark passes the literal src through.',
  },
};

export function intentionalDivergence(caseId) {
  return INTENTIONAL_DIVERGENCES[caseId] ?? null;
}

// Returns { html, semanticTypes, isIgnoreOverride, isIgnoreWithoutOverride }.
// `html` is null only for an `<IGNORE>` case with no recorded binary override —
// callers should auto-pass that (mirroring spec_tests.py) rather than compare.
export function effectiveExpected(testCase) {
  if (testCase.expected.html.trim() === '<IGNORE>') {
    const override = IGNORE_BINARY_OVERRIDES[testCase.id];
    if (override) {
      return {
        html: override.html,
        semanticTypes: override.semanticTypes,
        isIgnoreOverride: true,
        isIgnoreWithoutOverride: false,
      };
    }
    return {
      html: null,
      semanticTypes: null,
      isIgnoreOverride: false,
      isIgnoreWithoutOverride: true,
    };
  }
  return {
    html: testCase.expected.html,
    semanticTypes: testCase.expected.semanticTypes,
    isIgnoreOverride: false,
    isIgnoreWithoutOverride: false,
  };
}
