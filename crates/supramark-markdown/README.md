# supramark-markdown

Rust-first Markdown parser for Supramark.

Input: Markdown source.

Output: Supramark AST v2.

This crate owns the parser core, AST v2 schema, source map contract, and parse orchestration in one place. Some parser-core implementation code was adapted from `markdown-it-rust/markdown-it` as a code-level reference; Supramark does not preserve upstream API compatibility.

Public API is intentionally narrow: `parse(&str) -> SupramarkNode` plus the AST v2 data types. Internal `MarkdownIt`, `Node`, rule, and plugin APIs are implementation details.

Current guarantees:

- Outputs a serde-serializable Supramark AST v2.
- Every mapped node carries source `position` when the parser core provides `srcmap`.
- Positions include both UTF-8 byte offsets and UTF-16 offsets for JS/RN editor integration.
- Core CommonMark nodes, GFM tables, strikethrough, and diagram fences are mapped.

Near-term parser-core work:

- Move Supramark AST v2 construction deeper into parser rules instead of post-walking internal nodes.
- Add Supramark-native math, footnote, task list, definition-list, `:::` container, and `%%%` input rules.
- Collapse upstream-oriented internal names/APIs as they become irrelevant to Supramark.
