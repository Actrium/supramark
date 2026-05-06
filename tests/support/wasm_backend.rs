//! Deterministic Graphviz backend for plantuml-little's reference tests.
//!
//! Spawns a long-lived Node.js child process that loads
//! `@kookyleo/graphviz-anywhere-web` (a wasm-compiled Graphviz) and
//! streams DOT → SVG requests over stdin/stdout.

use plantuml_little::error::Error;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Mutex, OnceLock};

/// Env var that opts the test harness into the wasm backend.
pub const BACKEND_ENV_VAR: &str = "PLANTUML_LITTLE_TEST_BACKEND";

/// Returns true iff the env var selects the wasm backend.
pub fn wasm_backend_selected() -> bool {
    match std::env::var(BACKEND_ENV_VAR) {
        Ok(v) => {
            let v = v.trim();
            !v.is_empty() && !v.eq_ignore_ascii_case("native") && !v.eq_ignore_ascii_case("off")
        }
        Err(_) => false,
    }
}

/// Long-lived Node subprocess running `tests/support/viz_wasm_runner.mjs`.
struct WasmRunner {
    _child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl WasmRunner {
    fn spawn() -> Result<Self, Error> {
        let runner_script = locate_runner_script()?;
        let working_dir = runner_script
            .parent()
            .ok_or_else(|| Error::Layout("viz_wasm_runner has no parent dir".to_string()))?
            .to_path_buf();

        let mut cmd = Command::new("node");
        cmd.arg(&runner_script)
            .current_dir(&working_dir)
            .env("NODE_NO_WARNINGS", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd.spawn().map_err(|e| {
            Error::Layout(format!(
                "failed to spawn node for wasm Graphviz backend: {e}. \
                 Script path: {}",
                runner_script.display()
            ))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Layout("node child has no stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Layout("node child has no stdout".to_string()))?;
        let mut stdout = BufReader::new(stdout);

        let mut ready = String::new();
        stdout.read_line(&mut ready).map_err(|e| {
            Error::Layout(format!(
                "wasm Graphviz backend: failed to read READY line from node: {e}"
            ))
        })?;
        if !ready.starts_with("READY ") {
            return Err(Error::Layout(format!(
                "wasm Graphviz backend: unexpected handshake: {ready:?}"
            )));
        }

        Ok(WasmRunner {
            _child: child,
            stdin,
            stdout,
        })
    }

    fn render(&mut self, dot_src: &str) -> Result<String, Error> {
        let bytes = dot_src.as_bytes();
        writeln!(self.stdin, "{}", bytes.len())
            .and_then(|()| self.stdin.write_all(bytes))
            .and_then(|()| self.stdin.write_all(b"\n"))
            .and_then(|()| self.stdin.flush())
            .map_err(|e| Error::Layout(format!("wasm backend write failed: {e}")))?;

        let mut status = String::new();
        self.stdout
            .read_line(&mut status)
            .map_err(|e| Error::Layout(format!("wasm backend read status failed: {e}")))?;
        let status = status.trim_end_matches(['\r', '\n']);

        let mut len_line = String::new();
        self.stdout
            .read_line(&mut len_line)
            .map_err(|e| Error::Layout(format!("wasm backend read length failed: {e}")))?;
        let len: usize = len_line
            .trim_end_matches(['\r', '\n'])
            .parse()
            .map_err(|e| Error::Layout(format!("wasm backend bad length: {e}")))?;

        let mut payload = vec![0u8; len];
        self.stdout
            .read_exact(&mut payload)
            .map_err(|e| Error::Layout(format!("wasm backend read payload failed: {e}")))?;
        let mut trailing = [0u8; 1];
        let _ = self.stdout.read_exact(&mut trailing);

        let payload_str = String::from_utf8(payload)
            .map_err(|e| Error::Layout(format!("wasm backend non-UTF-8: {e}")))?;

        match status {
            "OK" => Ok(payload_str),
            "ERR" => Err(Error::Layout(format!("wasm backend ERR: {payload_str}"))),
            other => Err(Error::Layout(format!("wasm backend status {other}"))),
        }
    }
}

pub fn render_dot_to_svg(dot_src: &str) -> Result<String, Error> {
    static RUNNER: OnceLock<Mutex<WasmRunner>> = OnceLock::new();
    let mutex = match RUNNER.get() {
        Some(m) => m,
        None => {
            let runner = WasmRunner::spawn()?;
            let _ = RUNNER.set(Mutex::new(runner));
            RUNNER.get().unwrap()
        }
    };
    let mut guard = mutex.lock().unwrap();
    guard.render(dot_src)
}

fn locate_runner_script() -> Result<PathBuf, Error> {
    const REL: &str = "tests/support/viz_wasm_runner.mjs";
    if let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
        let p = Path::new(manifest_dir).join(REL);
        if p.exists() {
            return Ok(p);
        }
    }
    if let Ok(mut cur) = std::env::current_dir() {
        loop {
            let p = cur.join(REL);
            if p.exists() {
                return Ok(p);
            }
            if !cur.pop() {
                break;
            }
        }
    }
    Err(Error::Layout(
        "could not locate viz_wasm_runner.mjs".to_string(),
    ))
}
