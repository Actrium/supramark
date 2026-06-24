#!/usr/bin/env node
// One-shot dot-compatible wrapper backed by @actrium/graphviz-anywhere-web.
//
// Contrasts with `viz_wasm_runner.mjs`, which is a long-lived daemon spoken
// by the Rust test harness over a length-prefixed framing protocol. This
// file, instead, is invoked once per Graphviz call by Java PlantUML via
// `scripts/wasm-dot-wrapper.sh`. It:
//
//   - reads DOT source from stdin
//   - renders to SVG using Graphviz.load() (viz.wasm, Graphviz 14.1.5)
//   - writes the SVG bytes to stdout
//
// Using the same wasm blob on both sides (Java reference generator AND
// Rust consumer) guarantees byte-identical Graphviz output, which is the
// whole point of the "shared wasm Graphviz" reference-test pipeline.
//
// Flags accepted (for compatibility with what PlantUML passes):
//   -V           Print "dot - graphviz version <ver>" to stderr, exit 0.
//   -Tsvg        Output SVG (default; present because PlantUML always passes it).
//   -K<engine>   Layout engine override (default: dot).
//   -o <file>    Write output to <file> instead of stdout.
//   <path>       Read DOT from <path> instead of stdin.
//
// Exit codes: 0 on success, non-zero with an error on stderr on failure.

import process from "node:process";
import { readFileSync, writeFileSync } from "node:fs";

function parseArgs(argv) {
  const args = { format: "svg", engine: "dot", inputPath: null, outputPath: null, versionOnly: false };
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (a === "-V" || a === "--version") {
      args.versionOnly = true;
    } else if (a.startsWith("-T")) {
      args.format = a.slice(2) || "svg";
    } else if (a.startsWith("-K")) {
      args.engine = a.slice(2) || "dot";
    } else if (a === "-o") {
      args.outputPath = argv[++i];
    } else if (a.startsWith("-o")) {
      args.outputPath = a.slice(2);
    } else if (a.startsWith("-G") || a.startsWith("-N") || a.startsWith("-E") || a === "-q" || a.startsWith("-q") || a === "-v") {
      // Silently ignore global/node/edge attribute overrides and verbosity
      // flags. PlantUML doesn't currently pass these, but be lenient.
    } else if (a.startsWith("-")) {
      // Unknown flag: warn to stderr but keep going; Graphviz itself is
      // lenient too.
      process.stderr.write(`wasm-dot: warning: ignoring unknown flag ${JSON.stringify(a)}\n`);
    } else {
      args.inputPath = a;
    }
  }
  return args;
}

async function main() {
  const args = parseArgs(process.argv);

  let Graphviz;
  try {
    ({ Graphviz } = await import("@actrium/graphviz-anywhere-web"));
  } catch (err) {
    process.stderr.write(
      `wasm-dot: failed to import @actrium/graphviz-anywhere-web: ${err.stack ?? err}\n` +
        `Make sure 'npm install' has been run in tests/support/.\n`,
    );
    process.exit(1);
  }

  let gv;
  try {
    gv = await Graphviz.load();
  } catch (err) {
    process.stderr.write(`wasm-dot: Graphviz.load() failed: ${err.stack ?? err}\n`);
    process.exit(1);
  }

  if (args.versionOnly) {
    // Real dot writes its version to stderr in this exact shape.
    // PlantUML scrapes it to decide whether dot is available.
    const ver = gv.version();
    process.stderr.write(`dot - graphviz version ${ver} (wasm, @actrium/graphviz-anywhere-web)\n`);
    process.exit(0);
  }

  // Collect DOT source.
  let dot;
  if (args.inputPath) {
    dot = readFileSync(args.inputPath, "utf8");
  } else {
    const chunks = [];
    for await (const chunk of process.stdin) chunks.push(chunk);
    dot = Buffer.concat(chunks).toString("utf8");
  }

  let rendered;
  try {
    rendered = gv.layout(dot, args.format, args.engine);
  } catch (err) {
    process.stderr.write(`wasm-dot: layout failed: ${err && err.stack ? err.stack : err}\n`);
    process.exit(1);
  }

  const outBuf = Buffer.from(rendered, "utf8");
  if (args.outputPath) {
    writeFileSync(args.outputPath, outBuf);
  } else {
    process.stdout.write(outBuf);
  }
}

main().catch((err) => {
  process.stderr.write(`wasm-dot: unexpected error: ${err && err.stack ? err.stack : err}\n`);
  process.exit(1);
});
