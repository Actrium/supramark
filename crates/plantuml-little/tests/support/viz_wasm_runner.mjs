#!/usr/bin/env node
// Deterministic Graphviz backend for plantuml-little's reference tests.
//
// This script is spawned once by the Rust test harness (see
// `src/layout/wasm_backend.rs`) and services many `render_dot_to_svg`
// calls over stdin/stdout. The goal is byte-identical SVG output on
// every developer machine and every CI runner — @actrium/graphviz-
// anywhere-web ships a single viz.wasm binary that produces the same
// SVG bytes under Node's V8 wasm runtime regardless of host OS.
// (Version is pinned in `tests/support/package.json`.)
//
// Framing protocol (request, sent by Rust):
//   <length-in-bytes>\n
//   <dot-source, exactly <length> bytes>
//   \n
//
// Framing protocol (response, written by Node):
//   OK\n                                 (on success)
//   <length-in-bytes>\n
//   <svg-bytes, exactly <length> bytes>
//   \n
// …or on failure:
//   ERR\n
//   <length-in-bytes>\n
//   <error-message-bytes, exactly <length> bytes>
//   \n
//
// Exit codes: 0 on clean EOF, 1 on fatal init error (wasm fails to load).

import process from "node:process";

async function main() {
  let Graphviz;
  try {
    ({ Graphviz } = await import("@actrium/graphviz-anywhere-web"));
  } catch (err) {
    process.stderr.write(
      `viz_wasm_runner: failed to import @actrium/graphviz-anywhere-web: ${err.stack ?? err}\n` +
        `Make sure 'npm install' has been run in tests/support/.\n`,
    );
    process.exit(1);
  }

  let gv;
  try {
    gv = await Graphviz.load();
  } catch (err) {
    process.stderr.write(
      `viz_wasm_runner: Graphviz.load() failed: ${err.stack ?? err}\n`,
    );
    process.exit(1);
  }

  // Announce readiness (Rust waits for this line before sending any request).
  process.stdout.write(`READY ${gv.version()}\n`);

  // Read length-prefixed framed requests from stdin. We use a byte
  // buffer so that UTF-8 payloads don't get chunked mid-codepoint.
  const stdin = process.stdin;
  let pending = Buffer.alloc(0);

  function tryProcessOne() {
    // Find first '\n' — that's the length line terminator.
    const nlIdx = pending.indexOf(0x0a);
    if (nlIdx < 0) return false;

    const lenStr = pending.subarray(0, nlIdx).toString("utf8").trim();
    const len = Number.parseInt(lenStr, 10);
    if (!Number.isFinite(len) || len < 0) {
      process.stderr.write(`viz_wasm_runner: bad length line: ${JSON.stringify(lenStr)}\n`);
      process.exit(1);
    }

    // Need nlIdx + 1 (past '\n') + len (dot bytes) + 1 (trailing '\n').
    const needed = nlIdx + 1 + len + 1;
    if (pending.length < needed) return false;

    const dotStart = nlIdx + 1;
    const dotEnd = dotStart + len;
    const dot = pending.subarray(dotStart, dotEnd).toString("utf8");
    // consume (including the trailing '\n')
    pending = pending.subarray(needed);

    let svg;
    try {
      svg = gv.layout(dot, "svg", "dot");
    } catch (err) {
      const msg = (err && err.stack) || String(err);
      const buf = Buffer.from(msg, "utf8");
      process.stdout.write(`ERR\n${buf.length}\n`);
      process.stdout.write(buf);
      process.stdout.write("\n");
      return true;
    }

    const buf = Buffer.from(svg, "utf8");
    process.stdout.write(`OK\n${buf.length}\n`);
    process.stdout.write(buf);
    process.stdout.write("\n");
    return true;
  }

  stdin.on("data", (chunk) => {
    pending = pending.length === 0 ? chunk : Buffer.concat([pending, chunk]);
    // Drain as many framed requests as are fully buffered.
    while (tryProcessOne()) {
      /* loop */
    }
  });

  stdin.on("end", () => {
    // Graceful shutdown when Rust closes the pipe.
    process.exit(0);
  });
}

main().catch((err) => {
  process.stderr.write(
    `viz_wasm_runner: unexpected error: ${err && err.stack ? err.stack : err}\n`,
  );
  process.exit(1);
});
