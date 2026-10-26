//! issue #249: inputs that used to kill or wedge the process must now render or
//! return a normal error.
//!
//! Every case runs the CLI in a child process. A stack overflow or SIGSEGV
//! aborts the whole process, so an in-process test would take the test runner
//! down with it and could not report which input regressed; a hang would block
//! the runner until its own timeout. The child-process timeout here is test
//! protection only — the fixes themselves must terminate on their own.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Each case gets its own directory: the tests in this file run in parallel
/// threads of one process and would otherwise overwrite each other's input.
static NEXT_CASE_ID: AtomicUsize = AtomicUsize::new(0);

/// Run the CLI on `source`; returns the exit code (`None` if a signal killed
/// it) and the captured stderr.
fn run(source: &str, timeout: Duration) -> (Option<i32>, String) {
    let case_id = NEXT_CASE_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "supramark-issue249-{}-{case_id}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let input = dir.join("input.puml");
    let output = dir.join("out.svg");
    std::fs::write(&input, source).expect("write input");

    let mut child = Command::new(env!("CARGO_BIN_EXE_plantuml-little"))
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn CLI");

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait().expect("wait") {
            Some(status) => {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    use std::io::Read;
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return (status.code(), stderr);
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("timed out after {timeout:?}: the render never returned");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// A supported outcome: a rendered SVG (0) or a reported error (1). Anything
/// else — a signal, a panic-abort, a timeout — fails the assertion.
fn assert_no_process_death(case: &str, source: &str) {
    let (code, stderr) = run(source, Duration::from_secs(30));
    let Some(code) = code else {
        panic!("{case}: process died from a signal\n{stderr}");
    };
    assert!(
        code == 0 || code == 1,
        "{case}: unexpected exit code {code}\n{stderr}"
    );
}

fn assert_reports_error(case: &str, source: &str) {
    let (code, stderr) = run(source, Duration::from_secs(30));
    let Some(code) = code else {
        panic!("{case}: process died from a signal\n{stderr}");
    };
    assert_eq!(code, 1, "{case}: expected a reported error\n{stderr}");
    assert!(
        stderr.contains("Parse error"),
        "{case}: expected a parse error, got: {stderr}"
    );
}

#[test]
fn paren_link_in_same_named_boundary_renders() {
    // The issue's first input: `(a)` is a link reference to the enclosing
    // boundary, not a same-name declaration (Java renders it too).
    assert_no_process_death(
        "paren link in same-named boundary",
        "@startuml\nrectangle a {\n  (a) .> (aa) : <<a>>\n}\n@enduml\n",
    );
}

#[test]
fn unclosed_same_named_boundary_renders() {
    assert_no_process_death(
        "unclosed same-named boundary",
        "@startuml\nrectangle a {\n  (a) .> (aa) : <<a>>\n@enduml\n",
    );
}

#[test]
fn same_name_leaf_declaration_reports_an_error() {
    // Keyword declarations clashing with an enclosing group match the jar's
    // error page (outer groups included). `rectangle a { (a) }` is a
    // deliberate divergence: the jar renders it when it assumes an Activity
    // diagram and errors only with a component/usecase cue; main
    // stack-overflows on it, so it is rejected until `(a)` can resolve to the
    // enclosing boundary (follow-up).
    for case in [
        "@startuml\nrectangle a {\n  (a)\n}\n@enduml\n",
        "@startuml\nrectangle a {\n  usecase a\n}\n@enduml\n",
        "@startuml\nrectangle web {\n  component web\n}\n@enduml\n",
        "@startuml\npackage p {\n  component p\n}\n@enduml\n",
        "@startuml\nrectangle a {\n  rectangle b {\n    component a\n  }\n}\n@enduml\n",
    ] {
        assert_reports_error("same-name declaration", case);
    }
}

#[test]
fn paren_declaration_clashing_with_an_outer_group_renders() {
    // The jar renders this shape, so the outer name must not be treated as a
    // clash for the paren form.
    assert_no_process_death(
        "paren declaration inside a nested container with an outer same name",
        "@startuml\nrectangle a {\n  rectangle b {\n    (a)\n  }\n}\n@enduml\n",
    );
}

#[test]
fn sprite_path_with_invalid_char_terminates() {
    // Used to spin forever: no branch consumed the stray `$`.
    assert_no_process_death(
        "sprite path with `$`",
        "@startuml\nsprite s <svg><path d=\"M 1$ 2\"/></svg>\nAlice -> Bob : x <$s>\n@enduml\n",
    );
    assert_no_process_death(
        "sprite path with emoji",
        "@startuml\nsprite s <svg><path d=\"M 1\u{1F600}2 3\"/></svg>\nAlice -> Bob : x <$s>\n@enduml\n",
    );
}

#[test]
fn deeply_nested_states_report_an_error() {
    let depth = 500;
    let mut src = String::from("@startuml\n");
    for i in 0..depth {
        src.push_str(&format!("state s{i} {{\n"));
    }
    src.push_str("[*] -> x\n");
    for _ in 0..depth {
        src.push_str("}\n");
    }
    src.push_str("@enduml\n");
    assert_reports_error("deeply nested states", &src);
}

#[test]
fn nested_preprocessor_loop_is_bounded() {
    let src = "@startuml\n\
               !$n = 4000\n\
               !$row = 0\n\
               !while $row < $n\n\
               !$column = 0\n\
               !while $column < $n\n\
               !$column = $column + 1\n\
               !endwhile\n\
               !$row = $row + 1\n\
               !endwhile\n\
               A -> B\n\
               @enduml\n";
    assert_reports_error("nested preprocessor loop", src);
}

/// The state-diagram SIGSEGV is a vendored Graphviz 14.1.5 defect
/// (`lib/dotgen/dotsplines.c`, `make_flat_edge` reading `GD_rank(g)[r - 2]`
/// past the rank array). `patches/0001-dotgen-dotsplines-rank-guards.patch`
/// guards those sites, and `prepare_graphviz_source` applies it to every local
/// and CI build, so the case runs unconditionally here.
#[test]
fn state_pin_with_labelled_transition_renders() {
    assert_no_process_death(
        "state inputPin + labelled transition",
        "@startuml\nstate counter{\n  state count_start <<inputPin>>\n  [*] -> count_idle\n  count_finish -> count_idle\nsig_ff -> entry2 : \"!\"\n}\n@enduml\n",
    );
}
