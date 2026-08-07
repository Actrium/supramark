# micromark fixture attribution

The generated `cases.json` contains Markdown inputs and expected HTML captured from the
Markdown-to-HTML assertions under
[`test/io`](https://github.com/micromark/micromark/tree/774a70c6bae6dd94486d3385dbd9a0f14550b709/test/io)
in micromark 4.0.2. Index-only modules, stream tests, and constant-table tests are excluded because
they do not define standalone Markdown-to-HTML cases.

micromark is Copyright (c) Titus Wormer and is licensed under the
[MIT License](https://github.com/micromark/micromark/blob/774a70c6bae6dd94486d3385dbd9a0f14550b709/license).
Supramark executes only the pinned test modules in an isolated capture context: imports of
`node:test`, `node:assert/strict`, and `micromark` are replaced with capture stubs, while upstream
test control flow, Markdown inputs, renderer options, and expected HTML remain intact. Repository
file line endings are normalized only for source hashing; escaped Markdown line endings are
preserved in the generated cases.

The exact source repository, commit, selected fixture pattern, exclusions, per-file hashes, and
aggregate source hash are recorded in `version.json`.
