# cmark regression fixture attribution

The generated `cases.json` contains 27 Markdown inputs and expected HTML results derived from
[`test/regression.txt`](https://github.com/commonmark/cmark/blob/7042d9978b20fea86ca9cc98bda55f10be392e69/test/regression.txt)
in the CommonMark cmark repository.

The upstream fixture content is licensed under the BSD 2-Clause License. cmark is maintained by
John MacFarlane and cmark contributors.

Supramark converts the upstream fenced examples to a unified JSON representation and replaces the
visible tab marker with tab characters. Source line endings are preserved so byte-sensitive cases,
including the CR+CR+LF regression, retain their original test semantics. Markdown inputs and
expected HTML are otherwise unchanged.

The exact source repository, commit, fixture path, and raw source SHA-256 are recorded in
`version.json`.
