import assert from 'node:assert/strict';
import test from 'node:test';
import { renderConformanceIssue } from './issue-report.mjs';

test('single-case reproduction includes Bash and PowerShell commands', () => {
  const body = renderConformanceIssue({
    summary: {
      source: 'commonmark',
      sourceDisplayName: 'CommonMark',
      total: 1,
      passed: 0,
      notPassed: 1,
      overallNotPassedCases: 1,
      sourceCommit: 'test-source-commit',
      comparisonTarget: 'ast-projection',
      generatedAt: '2026-08-10T00:00:00Z',
      runtime: {},
      baseline: null,
      failureGroups: [],
      visual: { enabled: false, passed: 0, total: 0, notPassed: 0 },
    },
    semanticFailures: [{ id: 'commonmark-test-case' }],
    visualFailures: [],
    caseById: new Map(),
    astById: new Map(),
    actualHtmlById: new Map(),
    sourceVersion: '0.31.2',
  });

  assert.match(body, /\*\*Bash \(Linux\/macOS\):\*\*/);
  assert.match(body, /export CASE_IDS="commonmark-test-case"/);
  assert.match(body, /\*\*PowerShell \(Windows\):\*\*/);
  assert.match(body, /\$env:CASE_IDS = "commonmark-test-case"/);
});
