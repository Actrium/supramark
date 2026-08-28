# Markdown Conformance

[English](README.md) | [Chinese](README.zh.md)

This directory contains the Markdown source-import and rendering-conformance test suite. Normalized fixture data is stored separately under `tests/cases/_fixtures/`; this directory contains only execution tools, configuration, the browser host, isolated dependencies, and generated run output.

## Directory responsibilities

```text
tests/markdown-conformance/
  config/       Source configuration and normalized case schema
  importers/    Fixture adapters for each source
  scripts/      Executable import, validation, and test commands
  lib/          Semantic, visual, and report implementations
  browser/      Browser host for the production Web Renderer
  baselines/    Approved failing-case ID baselines
  artifacts/    Local and GitHub Actions run output (gitignored)
```

## Importing CommonMark data

The CommonMark adapter parses normative example blocks from `spec.txt` in the `commonmark/commonmark-spec` repository. The import pins the upstream commit and writes normalized JSON to:

```text
tests/cases/_fixtures/commonmark/
  cases.json
  cases.json.license
  version.json
  NOTICE.md
```

Run from the repository root:

```console
node tests/markdown-conformance/scripts/import.mjs commonmark
node tests/markdown-conformance/scripts/validate.mjs commonmark
```

Use `--source-dir <path>` when the upstream repository is already available locally. The adapter still validates the `origin` URL and pinned commit.

## Importing cmark regression data

The generic `spec-fixture` adapter imports the 27 fenced examples from cmark's
`test/regression.txt` at pinned commit `7042d9978b20fea86ca9cc98bda55f10be392e69`.
These are implementation regression cases, not normative specification examples. This source
preserves original line endings so the CR+CR+LF case reaches the parser unchanged.

```powershell
node tests/markdown-conformance/scripts/import.mjs cmark-regression
node tests/markdown-conformance/scripts/validate.mjs cmark-regression
node tests/markdown-conformance/scripts/run.mjs cmark-regression
```

## Importing cmark-gfm data

The cmark-gfm adapter combines 672 GFM specification cases from `test/spec.txt` in `github/cmark-gfm` with 30 extension cases from `test/extensions.txt`, for 702 cases in total. The source is pinned to the full commit for `0.29.0.gfm.13`. Each case preserves its real fixture path and an independent upstream numbering space, and normalized data is written to:

```text
tests/cases/_fixtures/cmark-gfm/
  cases.json
  cases.json.license
  version.json
  NOTICE.md
```

```console
node tests/markdown-conformance/scripts/import.mjs cmark-gfm
node tests/markdown-conformance/scripts/validate.mjs cmark-gfm
```

## Importing micromark data

The micromark adapter discovers core Markdown-to-HTML assertions from `test/io/**/*.js` at the pinned commit. It excludes aggregation-only modules, stream tests, and constant-table tests. In an isolated capture environment it preserves Markdown input, expected HTML, upstream options, test names, and source line numbers, producing 1,151 normalized cases. Newly matching files require no changes to the import entry point or explicit path list.

```console
node tests/markdown-conformance/scripts/import.mjs micromark
node tests/markdown-conformance/scripts/validate.mjs micromark
```

To add another source, provide a source configuration and adapter. The generic entry point supports one file, explicit file lists, globbed file sets, synchronous or asynchronous adapters, and paired Markdown/HTML fixtures. See `config/sources/README.md` for the conventions.

## Test commands

Install the isolated dependencies and Chromium:

```console
pnpm --dir tests/markdown-conformance install --frozen-lockfile
node tests/markdown-conformance/node_modules/playwright/cli.js install chromium
```

Build the parser, then run semantic or visual comparison:

```console
cargo build -p supramark-markdown --bin supramark-markdown
node tests/markdown-conformance/scripts/run.mjs <source-name>
node tests/markdown-conformance/scripts/run-visual.mjs <source-name>
```

The directly supported source names are `commonmark`, `cmark-regression`, `cmark-gfm`, and `micromark`. For example:

```console
node tests/markdown-conformance/scripts/run.mjs micromark
node tests/markdown-conformance/scripts/run-visual.mjs micromark
```

The generic import, validation, run, visual-run, and baseline entry points are source-agnostic; adding a source does not require source-specific runner wrappers.

Supported environment variables:

- `SUPRAMARK_MARKDOWN_BIN`: parser CLI path.
- `CASE_IDS`: comma-separated case IDs for focused debugging.
- `FAIL_ON_FAILURES=0`: generate failure reports while keeping process exit code zero.
- `ARTIFACT_DIR`: output directory for the run, useful for isolated case debugging without overwriting the full report.
- `VISUAL_PIXEL_THRESHOLD`, `VISUAL_MAX_DIFF_PIXELS`, and `VISUAL_MAX_DIFF_RATIO`: visual comparison thresholds.

## Reports

Reports are written to `tests/markdown-conformance/artifacts/<source-name>/`:

- `summary.md`: summary and complete failure list.
- `report.html`: filterable visual report with expected, actual, and diff images side by side.
- `issue.md`: issue body containing the problem description, reproduction steps, expected result, and actual result.
- `issue-metadata.json`: source-aware issue title, stable marker, and labels.
- `summary.json`, `failures.json`, and `visual-failures.json`: machine-readable results.
- `visual/`: PNG artifacts for failing visual cases.
- `evidence/<case ID>/`: actual AST, actual HTML, and expected and actual semantic trees.
- `evidence-index.json`: failure-evidence index for the current run.

Visual tests use the repository's default Rust Supramark parser AST and the production React renderer at `packages/renderers/web/src/Supramark.tsx`. The browser host isolates diagram engines and the browser WASM parser to avoid parsing twice; the final DOM comes from the production renderer.

The GitHub Actions workflow is `.github/workflows/markdown-conformance.yml`. It validates the selected sources, creates a dynamic matrix, imports and validates each source independently, runs comparison, uploads reports, and maintains an aggregate issue. Failed runs upload the complete report and generate `issue.md` and `issue-metadata.json`. When issue creation is enabled, the workflow creates or updates the aggregate issue. Pull requests only validate and upload artifacts.

Issue titles use the format `[<source display name>] Conformance failure: cases did not pass`. The workflow applies the `bug` label and uses a stable marker to update the existing aggregate issue for the same source.

Workflow-dispatch inputs:

- `sources`: one source name, a comma-separated list, or `all`, such as `cmark-regression` or `cmark-gfm,micromark`.
- `create_issue`: create or update the aggregate issue after failures; enabled by default.
- `run_visual`: run browser visual comparison; enabled by default. Disable it for semantic-only comparison.
- `fail_workflow`: mark the workflow as failed when cases do not pass; enabled by default.
- `gate_mode`: use `regression` to fail only on new failures versus the baseline, or `absolute` to fail on any non-passing case.
- `issue_repository`: target issue repository in `owner/repo` form; leave empty to use the current repository.

Push runs use GitHub Actions variables for the same behavior:

- `MARKDOWN_CONFORMANCE_SOURCES`: a source name, comma-separated list, or `all`; all configured sources run when unset.
- `MARKDOWN_CONFORMANCE_AUTO_ISSUE=false`: disable issue creation or updates after failures; enabled when unset.
- `MARKDOWN_CONFORMANCE_RUN_VISUAL=false`: disable visual comparison; enabled when unset.
- `MARKDOWN_CONFORMANCE_FAIL_WORKFLOW=false`: preserve reports without marking the workflow failed; failures mark the workflow when unset.
- `MARKDOWN_CONFORMANCE_ISSUE_REPOSITORY=owner/repo`: target issue repository; the current repository is used when unset.

The legacy variables `COMMONMARK_AUTO_ISSUE`, `COMMONMARK_RUN_VISUAL`, `COMMONMARK_FAIL_WORKFLOW`, and `COMMONMARK_ISSUE_REPOSITORY` remain supported.

Pull requests never create issues automatically, even when `COMMONMARK_AUTO_ISSUE=true`.

The default `github.token` can create issues when the target repository is the repository running the workflow. For cross-repository issue creation, configure the `MARKDOWN_CONFORMANCE_ISSUE_TOKEN` secret in the workflow repository. `COMMONMARK_ISSUE_TOKEN` remains supported. The token needs Issues write permission in the target repository, and Issues must be enabled there.

## Approved baselines

Approved baselines are stored per source at `tests/markdown-conformance/baselines/<source-name>.json`. Reports use them to distinguish new, resolved, and persistent failures. A baseline can only be updated after a complete run of every case for that source, with visual testing enabled and zero semantic or visual execution errors:

```console
node tests/markdown-conformance/scripts/update-baseline.mjs <source-name>
```

`update-commonmark-baseline.mjs` remains as a compatibility entry point. Updating a baseline is a manual approval action and must not happen during a normal GitHub Actions run.
