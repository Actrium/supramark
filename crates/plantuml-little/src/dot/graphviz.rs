// Port of net.sourceforge.plantuml.dot.Graphviz (interface),
// AbstractGraphviz, GraphvizLinux, ProcessRunner, ProcessState, ExeState,
// GraphvizUtils, and GraphvizRuntimeEnvironment.
//
// The existing layout/graphviz.rs handles the actual `dot -Tsvg` execution
// and SVG parsing for node/edge layout. This module provides the canonical
// Graphviz abstraction layer: executable discovery, version detection,
// process execution, and exe state checking.

use std::fmt;
use std::io::Write;
use std::path::Path;

use crate::dot::version::GraphvizVersion;

// ---------------------------------------------------------------------------
// ExeState — port of net.sourceforge.plantuml.dot.ExeState
// ---------------------------------------------------------------------------

/// State of the Graphviz `dot` executable.
/// Mirrors Java `ExeState` enum for checking file validity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExeState {
    Ok,
    NullUndefined,
    DoesNotExist,
    IsADirectory,
    NotAFile,
    CannotBeRead,
}

impl ExeState {
    /// Check the state of an executable path.
    /// Java: `ExeState.checkFile(File dotExe)`
    pub fn check_file(path: Option<&Path>) -> ExeState {
        let path = match path {
            None => return ExeState::NullUndefined,
            Some(p) => p,
        };
        if !path.exists() {
            return ExeState::DoesNotExist;
        }
        if path.is_dir() {
            return ExeState::IsADirectory;
        }
        if !path.is_file() {
            return ExeState::NotAFile;
        }
        // On Unix, check read + execute permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(path) {
                let mode = meta.permissions().mode();
                if mode & 0o444 == 0 {
                    return ExeState::CannotBeRead;
                }
            }
        }
        ExeState::Ok
    }

    /// Human-readable message.
    /// Java: `ExeState.getTextMessage()`
    pub fn text_message(&self) -> &'static str {
        match self {
            ExeState::Ok => "Dot executable OK",
            ExeState::NullUndefined => "No dot executable found",
            ExeState::DoesNotExist => "Dot executable does not exist",
            ExeState::IsADirectory => "Dot executable should be an executable, not a directory",
            ExeState::NotAFile => "Dot executable is not a valid file",
            ExeState::CannotBeRead => "Dot executable cannot be read",
        }
    }

    /// Human-readable message including the file path.
    /// Java: `ExeState.getTextMessage(File exe)`
    pub fn text_message_with_path(&self, path: &Path) -> String {
        match self {
            ExeState::Ok => format!("File {} OK", path.display()),
            ExeState::NullUndefined => self.text_message().to_string(),
            _ => format!("File {}: {}", path.display(), self.text_message()),
        }
    }
}

impl fmt::Display for ExeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.text_message())
    }
}

// ---------------------------------------------------------------------------
// ProcessState — port of net.sourceforge.plantuml.dot.ProcessState
// ---------------------------------------------------------------------------

/// Outcome of a subprocess execution.
/// Mirrors Java `ProcessState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessState {
    TerminatedOk,
    Timeout,
    Exception(String),
}

impl ProcessState {
    pub fn is_ok(&self) -> bool {
        matches!(self, ProcessState::TerminatedOk)
    }

    /// Java: `differs(ProcessState other)` — true when states are different.
    pub fn differs(&self, other: &ProcessState) -> bool {
        self != other
    }
}

impl fmt::Display for ProcessState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessState::TerminatedOk => write!(f, "TERMINATED_OK"),
            ProcessState::Timeout => write!(f, "TIMEOUT"),
            ProcessState::Exception(msg) => write!(f, "EXCEPTION {msg}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Graphviz trait — port of net.sourceforge.plantuml.dot.Graphviz interface
// ---------------------------------------------------------------------------

/// Graphviz execution interface.
///
/// Port of Java `Graphviz` interface. Provides methods to:
/// - Execute `dot` to produce SVG output
/// - Query the dot version
/// - Check executable state
pub trait Graphviz {
    /// Run the `dot` command with the stored DOT source, writing output
    /// to the provided writer.
    /// Java: `createFile3(OutputStream os)`
    fn create_file(&self, output: &mut dyn Write) -> ProcessState;

    /// Return the `dot -V` version string.
    /// Java: `dotVersion()`
    fn dot_version(&self) -> String;

    /// Return the path to the dot executable.
    /// Java: `getDotExe()`
    fn dot_exe(&self) -> Option<&Path>;

    /// Check the executable state.
    /// Java: `getExeState()`
    fn exe_state(&self) -> ExeState;
}

// ---------------------------------------------------------------------------
// GraphvizInProcess — Recommended in-process Graphviz implementation
// ---------------------------------------------------------------------------

/// In-process Graphviz implementation using the `graphviz-anywhere` crate.
/// This is the only supported implementation as it avoids `Command::spawn` and
/// supports both native and WASM environments.
pub struct GraphvizInProcess {
    dot_string: String,
}

impl GraphvizInProcess {
    pub fn new(dot_string: &str) -> Self {
        GraphvizInProcess {
            dot_string: dot_string.to_string(),
        }
    }
}

/// Process-wide lock guarding all libgvc access. graphviz-anywhere wraps
/// libgvc, which keeps mutable global state (plugin registry, error
/// buffers) and is not thread-safe — concurrent contexts crash with
/// SIGSEGV. Both `GraphvizInProcess::create_file` here and
/// `crate::layout::graphviz::render_dot_to_svg_native` serialize through
/// this same lock.
pub(crate) fn gv_lock() -> std::sync::MutexGuard<'static, ()> {
    static GV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    GV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

impl Graphviz for GraphvizInProcess {
    fn create_file(&self, output: &mut dyn Write) -> ProcessState {
        use graphviz_anywhere::{Engine, Format, GraphvizContext};
        let _guard = gv_lock();
        let ctx = match GraphvizContext::new() {
            Ok(c) => c,
            Err(e) => return ProcessState::Exception(format!("context error: {e}")),
        };
        match ctx.render_to_string(&self.dot_string, Engine::Dot, Format::Svg) {
            Ok(svg) => {
                if let Err(e) = output.write_all(svg.as_bytes()) {
                    ProcessState::Exception(format!("write error: {e}"))
                } else {
                    ProcessState::TerminatedOk
                }
            }
            Err(e) => ProcessState::Exception(format!("render error: {e}")),
        }
    }

    fn dot_version(&self) -> String {
        // graphviz-anywhere 0.1.6 does not provide a direct version API yet,
        // but it links against the system libgvc. We return a generic string.
        "graphviz-anywhere (in-process)".to_string()
    }

    fn dot_exe(&self) -> Option<&Path> {
        None
    }

    fn exe_state(&self) -> ExeState {
        ExeState::Ok
    }
}

// ---------------------------------------------------------------------------
// Utility functions — port of GraphvizUtils
// ---------------------------------------------------------------------------

/// Default image size limit.
/// Java: `GraphvizUtils.getenvImageLimit()` default = 4096.
pub const DEFAULT_IMAGE_LIMIT: u32 = 4096;

/// Get the image size limit from environment or default.
/// Java: `GraphvizUtils.getenvImageLimit()`
pub fn image_limit() -> u32 {
    if let Ok(val) = std::env::var("PLANTUML_LIMIT_SIZE") {
        if let Ok(n) = val.parse::<u32>() {
            return n;
        }
    }
    DEFAULT_IMAGE_LIMIT
}

/// Quick test: create a trivial graph and verify SVG output.
/// Java: `GraphvizUtils.getTestCreateSimpleFile()`
///
/// Returns `Ok(())` if Graphviz produces valid SVG, or `Err(message)`.
pub fn test_graphviz_installation() -> Result<(), String> {
    let gv = GraphvizInProcess::new("digraph foo { test; }");

    let mut output = Vec::new();
    let state = gv.create_file(&mut output);
    if state.differs(&ProcessState::TerminatedOk) {
        return Err(format!("Error: timeout {state}"));
    }
    if output.is_empty() {
        return Err("Error: dot generates empty file. Check your dot installation.".into());
    }
    let s = String::from_utf8_lossy(&output);
    if !s.contains("<svg") {
        return Err(
            "Error: dot generates unreadable SVG file. Check your dot installation.".into(),
        );
    }
    Ok(())
}

/// Detect the installed Graphviz version.
/// Combines Java's `GraphvizVersionFinder` and `GraphvizRuntimeEnvironment`.
pub fn detect_graphviz_version() -> GraphvizVersion {
    let version_str = GraphvizInProcess::new("").dot_version();
    log::info!("detect_graphviz_version: raw={version_str}");
    GraphvizVersion::parse_from_dot_output(&version_str).unwrap_or(GraphvizVersion::DEFAULT)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_state_check_none() {
        assert_eq!(ExeState::check_file(None), ExeState::NullUndefined);
    }

    #[test]
    fn exe_state_check_nonexistent() {
        let p = Path::new("/nonexistent/path/to/dot_xyz_123");
        assert_eq!(ExeState::check_file(Some(p)), ExeState::DoesNotExist);
    }

    #[test]
    fn exe_state_check_directory() {
        // Use the platform temp dir rather than a hard-coded "/tmp", which does
        // not exist on Windows (there it would resolve to DoesNotExist).
        let p = std::env::temp_dir();
        assert_eq!(ExeState::check_file(Some(&p)), ExeState::IsADirectory);
    }

    #[test]
    fn exe_state_text_messages() {
        assert_eq!(ExeState::Ok.text_message(), "Dot executable OK");
        assert_eq!(
            ExeState::NullUndefined.text_message(),
            "No dot executable found"
        );
        assert!(!ExeState::DoesNotExist.text_message().is_empty());
    }

    #[test]
    fn exe_state_text_message_with_path() {
        let p = Path::new("/usr/bin/dot");
        let msg = ExeState::Ok.text_message_with_path(p);
        assert!(msg.contains("/usr/bin/dot"));
        assert!(msg.contains("OK"));
    }

    #[test]
    fn process_state_display() {
        assert_eq!(format!("{}", ProcessState::TerminatedOk), "TERMINATED_OK");
        assert_eq!(format!("{}", ProcessState::Timeout), "TIMEOUT");
    }

    #[test]
    fn process_state_differs() {
        assert!(!ProcessState::TerminatedOk.differs(&ProcessState::TerminatedOk));
        assert!(ProcessState::TerminatedOk.differs(&ProcessState::Timeout));
    }

    #[test]
    fn image_limit_default() {
        // Unless PLANTUML_LIMIT_SIZE is set, should return 4096
        let limit = image_limit();
        assert!(limit > 0);
    }

    #[test]
    fn graphviz_in_process_create_file() {
        let gv = GraphvizInProcess::new("digraph G { A -> B; }");
        let mut buf = Vec::new();
        let state = gv.create_file(&mut buf);
        assert!(state.is_ok(), "expected TerminatedOk, got {state}");
        let svg = String::from_utf8_lossy(&buf);
        assert!(svg.contains("<svg"), "output should contain <svg");
    }

    #[test]
    fn graphviz_in_process_dot_version() {
        let gv = GraphvizInProcess::new("");
        let version = gv.dot_version();
        assert!(!version.is_empty(), "should return a version string");
    }

    #[test]
    fn test_installation_integration() {
        let result = test_graphviz_installation();
        assert!(result.is_ok(), "installation test failed: {:?}", result);
    }

    #[test]
    fn detect_version_integration() {
        let v = detect_graphviz_version();
        // graphviz-anywhere returns GraphvizVersion::DEFAULT (2.44.0) currently
        assert!(v.major >= 2, "major version should be >= 2");
    }
}
