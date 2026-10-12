# Conformance sources

Each JSON file declares one pinned upstream source. The generic import command reads the
configuration, verifies the repository remote and full commit, loads the configured adapter,
and writes normalized fixtures under `tests/cases/_fixtures/<source-name>/`.

Core configuration fields:

- `name`: stable lowercase source identifier.
- `displayName`: human-readable source name.
- `repository`: canonical upstream Git repository URL.
- `version`: source release represented by the fixture.
- `revision`: full 40-character source commit.
- `license`: SPDX license identifier for the imported content.
- `profile`: unified case profile (`commonmark`, `gfm`, or `supramark`).
- `importer`: adapter module basename under `tests/markdown-conformance/importers/`.
- `expectedKind`: optional unified expectation kind (`normative` by default or `implementation`).
- `expectedCaseCount`: optional pinned count checked during import and validation.
- `lineEndings`: optional fixture text mode (`normalize` by default or `preserve`).
- `sectionCoverage`: section-to-coverage mapping used by the generic `spec-fixture` adapter.
- `integrityChecks`: optional case selectors with `inputContains` or `inputEquals` assertions,
  useful for byte-sensitive fixtures such as unusual line endings.

Fixture selection uses one or more of the following fields:

- `input`: one fixture path within the pinned commit.
- `inputs`: ordered explicit fixture descriptors. Each descriptor supplies `path` and may carry
  adapter metadata such as `role`, `pairId`, `fixtureVersion`, or `caseIdNamespace`.
- `inputGlobs`: ordered repository glob descriptors. Each supplies `pattern` and can supply an
  `exclude` array. Matches are sorted by repository path and appended after explicit inputs.

`versionProbe` is optional. Adapters that call `verifyVersionProbe` use its `path`, regular
expression `pattern`, optional `flags`, and optional capture `group` to verify that the configured
release version is present in the pinned source.

An adapter default-exports a synchronous or asynchronous function accepting
`(sourceDocuments, sourceConfig)` and returning `{ cases, sourceSha256 }`. Multi-file adapters also
return `sourceFiles` in selected-input order, including the path, source hash, and case count for
every fixture. Metadata-only and expected-output companions use `caseCount: 0`. Spec-style sources
can reuse `importers/spec-examples.mjs`; Markdown/HTML file pairs can reuse
`importers/paired-files.mjs`.

From the repository root:

```console
node tests/markdown-conformance/scripts/import.mjs <source-name>
node tests/markdown-conformance/scripts/validate.mjs <source-name>
```

Use `--source-dir <repository>` to import from an existing checkout. Its `origin` and resolved
commit must match the pinned configuration. The reusable `spec-fixture` adapter imports
CommonMark-style fenced examples and can preserve source line endings when configured. Keep
generated cases in `tests/cases`; keep all
configuration, adapters, runners, dependencies, and reports in `tests/markdown-conformance`.
