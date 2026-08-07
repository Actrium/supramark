# Imported Markdown fixtures

[English](README.md) | [Chinese](README.zh.md)

This directory contains only normalized test cases imported from external specifications or implementation repositories. Import scripts, test runners, dependencies, and generated reports live elsewhere.

Directory layout:

```text
tests/cases/
  _fixtures/
    <source-name>/
      cases.json
      cases.json.license
      version.json
      NOTICE.md
```

Current sources:

- `commonmark`: CommonMark 0.31.2, with 652 normative cases.
- `cmark-gfm`: GitHub cmark-gfm 0.29.0.gfm.13, with 702 normative and extension cases.
- `micromark`: micromark 4.0.2, with 1,151 core Markdown-to-HTML implementation regression cases.

Import, validation, production Web Renderer comparison, and report tooling are located in `tests/markdown-conformance/`.