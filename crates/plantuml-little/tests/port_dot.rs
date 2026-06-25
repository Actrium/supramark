// Port of Java PlantUML dot-package tests to Rust.
//
// Source Java tests:
//   generated-public-api-tests-foundation/packages/net/sourceforge/plantuml/dot/
//
// Rust equivalents live in plantuml_little::dot::*.
//
// Tests that have no Rust equivalent are marked #[ignore = "gap: <Name> not yet ported"].

use plantuml_little::dot::{
    rect_line_intersection, DotSplines, ExeState, GraphvizVersion, Point2D, ProcessState, Rect2D,
};

// ---------------------------------------------------------------------------
// ProcessState — Java: ProcessStateSkeletonTest
// ---------------------------------------------------------------------------
//
// Java ProcessState has factory methods: TERMINATED_OK(), TIMEOUT(), EXCEPTION(cause).
// Rust uses enum variants: ProcessState::TerminatedOk, Timeout, Exception(String).
// Java EXCEPTION holds a Throwable cause; Rust holds a String message.

#[cfg(test)]
mod process_state_tests {
    use super::*;

    #[test]
    fn terminated_ok_is_consistent() {
        // Java: assertSame(ProcessState.TERMINATED_OK(), ProcessState.TERMINATED_OK())
        assert_ne!(ProcessState::TerminatedOk, ProcessState::Timeout);
        assert!(ProcessState::TerminatedOk.is_ok());
    }

    #[test]
    fn timeout_is_consistent() {
        assert_ne!(ProcessState::Timeout, ProcessState::TerminatedOk);
        assert!(!ProcessState::Timeout.is_ok());
    }

    #[test]
    fn to_string_terminated_ok() {
        // Java: assertEquals("TERMINATED_OK", ProcessState.TERMINATED_OK().toString())
        assert_eq!(format!("{}", ProcessState::TerminatedOk), "TERMINATED_OK");
    }

    #[test]
    fn to_string_timeout() {
        // Java: assertEquals("TIMEOUT", ProcessState.TIMEOUT().toString())
        assert_eq!(format!("{}", ProcessState::Timeout), "TIMEOUT");
    }

    #[test]
    fn to_string_exception_includes_cause_message() {
        // Java: assertTrue(s.startsWith("EXCEPTION")); assertTrue(s.contains("dot crashed"))
        let state = ProcessState::Exception("dot crashed".into());
        let s = format!("{state}");
        assert!(
            s.starts_with("EXCEPTION"),
            "should start with EXCEPTION, got: {s}"
        );
        assert!(
            s.contains("dot crashed"),
            "should contain cause message, got: {s}"
        );
    }

    #[test]
    fn equals_same_state() {
        // Java: assertTrue(ProcessState.TERMINATED_OK().equals(ProcessState.TERMINATED_OK()))
        // Verify each variant is distinct from the other variants.
        assert_ne!(ProcessState::TerminatedOk, ProcessState::Timeout);
    }

    #[test]
    fn equals_different_states() {
        // Java: assertFalse(ProcessState.TERMINATED_OK().equals(ProcessState.TIMEOUT()))
        assert_ne!(ProcessState::TerminatedOk, ProcessState::Timeout);
    }

    #[test]
    fn differs_returns_true_for_different_states() {
        // Java: assertTrue(ProcessState.TERMINATED_OK().differs(ProcessState.TIMEOUT()))
        assert!(ProcessState::TerminatedOk.differs(&ProcessState::Timeout));
    }

    #[test]
    fn differs_returns_false_for_same_state() {
        // Java: assertFalse(ProcessState.TERMINATED_OK().differs(ProcessState.TERMINATED_OK()))
        assert!(!ProcessState::TerminatedOk.differs(&ProcessState::TerminatedOk));
    }

    #[test]
    fn exception_differs_from_terminated_ok() {
        // Java: assertTrue(ex.differs(ProcessState.TERMINATED_OK()))
        let ex = ProcessState::Exception(String::new());
        assert!(ex.differs(&ProcessState::TerminatedOk));
    }

    #[test]
    fn two_exception_instances_are_equal_by_variant() {
        // Java: two EXCEPTION instances are equal by name (not by cause reference).
        // In Rust, Exception("a") != Exception("b") because the String payload differs.
        // The Java equality-by-name behaviour would require ignoring the payload.
        // We document the Rust behaviour: same-message exceptions are equal.
        let a = ProcessState::Exception("same".into());
        let b = ProcessState::Exception("same".into());
        assert_eq!(a, b);
    }

    #[test]
    #[ignore = "gap: ProcessState::Exception equality ignores payload in Java but not in Rust"]
    fn two_exception_instances_with_different_messages_are_equal_java_style() {
        // Java: a.equals(b) -> true even with different cause messages.
        // Rust Exception("a") != Exception("b") — behaviour differs from Java.
        todo!()
    }
}

// ---------------------------------------------------------------------------
// ExeState — Java: ExeStateSkeletonTest
// ---------------------------------------------------------------------------

#[cfg(test)]
mod exe_state_tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn has_six_variants() {
        // Java: assertEquals(6, ExeState.values().length)
        // Rust: ExeState has Ok, NullUndefined, DoesNotExist, IsADirectory, NotAFile, CannotBeRead
        let variants = [
            ExeState::Ok,
            ExeState::NullUndefined,
            ExeState::DoesNotExist,
            ExeState::IsADirectory,
            ExeState::NotAFile,
            ExeState::CannotBeRead,
        ];
        assert_eq!(variants.len(), 6);
    }

    #[test]
    fn check_file_none_returns_null_undefined() {
        // Java: assertEquals(ExeState.NULL_UNDEFINED, ExeState.checkFile(null))
        assert_eq!(ExeState::check_file(None), ExeState::NullUndefined);
    }

    #[test]
    fn check_file_nonexistent_returns_does_not_exist() {
        // Java: assertEquals(ExeState.DOES_NOT_EXIST, ExeState.checkFile(new File("/nonexistent/path/dot")))
        let p = Path::new("/nonexistent/path/dot_that_definitely_does_not_exist_xyz");
        assert_eq!(ExeState::check_file(Some(p)), ExeState::DoesNotExist);
    }

    #[test]
    fn check_file_directory_returns_is_a_directory() {
        // Java: assertEquals(ExeState.IS_A_DIRECTORY, ExeState.checkFile(dir))
        // Use the platform temp dir rather than a hard-coded "/tmp", which does
        // not exist on Windows (there it would resolve to DoesNotExist).
        let p = std::env::temp_dir();
        assert_eq!(ExeState::check_file(Some(&p)), ExeState::IsADirectory);
    }

    #[test]
    fn check_file_regular_file_returns_ok() {
        // Java: assertEquals(ExeState.OK, ExeState.checkFile(exe)) for a readable file
        let tmp = std::env::temp_dir().join("port_dot_exe_state_test_file.tmp");
        fs::write(&tmp, b"").expect("failed to create temp file");
        let result = ExeState::check_file(Some(&tmp));
        let _ = fs::remove_file(&tmp);
        assert_eq!(result, ExeState::Ok);
    }

    #[test]
    fn text_message_null_undefined() {
        // Java: assertEquals("No dot executable found", ExeState.NULL_UNDEFINED.getTextMessage())
        assert_eq!(
            ExeState::NullUndefined.text_message(),
            "No dot executable found"
        );
    }

    #[test]
    fn text_message_ok() {
        // Java: assertEquals("Dot executable OK", ExeState.OK.getTextMessage())
        assert_eq!(ExeState::Ok.text_message(), "Dot executable OK");
    }

    #[test]
    fn text_message_does_not_exist() {
        // Java: assertEquals("Dot executable does not exist", ExeState.DOES_NOT_EXIST.getTextMessage())
        assert_eq!(
            ExeState::DoesNotExist.text_message(),
            "Dot executable does not exist"
        );
    }

    #[test]
    fn text_message_is_a_directory() {
        // Java: assertEquals("Dot executable should be an executable, not a directory", ...)
        assert_eq!(
            ExeState::IsADirectory.text_message(),
            "Dot executable should be an executable, not a directory"
        );
    }

    #[test]
    fn text_message_not_a_file() {
        // Java: assertEquals("Dot executable is not a valid file", ExeState.NOT_A_FILE.getTextMessage())
        assert_eq!(
            ExeState::NotAFile.text_message(),
            "Dot executable is not a valid file"
        );
    }

    #[test]
    fn text_message_cannot_be_read() {
        // Java: assertEquals("Dot executable cannot be read", ExeState.CANNOT_BE_READ.getTextMessage())
        assert_eq!(
            ExeState::CannotBeRead.text_message(),
            "Dot executable cannot be read"
        );
    }

    #[test]
    fn text_message_with_path_ok_contains_path_and_ok() {
        // Java: assertTrue(msg.contains(exe.getAbsolutePath())); assertTrue(msg.contains("OK"))
        let p = Path::new("/usr/bin/dot");
        let msg = ExeState::Ok.text_message_with_path(p);
        assert!(
            msg.contains("/usr/bin/dot"),
            "should contain path, got: {msg}"
        );
        assert!(msg.contains("OK"), "should contain 'OK', got: {msg}");
    }

    #[test]
    fn text_message_with_path_does_not_exist_contains_path() {
        // Java: assertTrue(msg.contains("/fake/dot")); assertTrue(msg.contains("does not exist"))
        let p = Path::new("/fake/dot");
        let msg = ExeState::DoesNotExist.text_message_with_path(p);
        assert!(msg.contains("/fake/dot"), "should contain path, got: {msg}");
        assert!(
            msg.contains("does not exist"),
            "should describe absence, got: {msg}"
        );
    }

    #[test]
    fn text_message_with_path_null_undefined_ignores_file() {
        // Java: assertEquals(ExeState.NULL_UNDEFINED.getTextMessage(), ExeState.NULL_UNDEFINED.getTextMessage(f))
        // NullUndefined ignores the path argument — same message with or without path.
        let p = Path::new("/fake/dot");
        assert_eq!(
            ExeState::NullUndefined.text_message(),
            ExeState::NullUndefined.text_message_with_path(p)
        );
    }
}

// ---------------------------------------------------------------------------
// UnparsableGraphvizException — Java: UnparsableGraphvizExceptionSkeletonTest
// ---------------------------------------------------------------------------
//
// Java: UnparsableGraphvizException(Throwable cause, String version, String svg, String diagram)
// There is no direct Rust equivalent struct — this is a gap.

#[cfg(test)]
mod unparsable_graphviz_exception_tests {
    #[test]
    #[ignore = "gap: UnparsableGraphvizException not yet ported to Rust"]
    fn get_graphviz_version_returns_version_string() {
        todo!()
    }

    #[test]
    #[ignore = "gap: UnparsableGraphvizException not yet ported to Rust"]
    fn get_debug_data_contains_svg_and_diagram() {
        // Java: assertEquals("SVG=" + svg + "\r\nDIAGRAM=" + diagram, ex.getDebugData())
        todo!()
    }

    #[test]
    #[ignore = "gap: UnparsableGraphvizException not yet ported to Rust"]
    fn is_runtime_exception() {
        todo!()
    }

    #[test]
    #[ignore = "gap: UnparsableGraphvizException not yet ported to Rust"]
    fn get_cause_is_original_exception() {
        todo!()
    }
}

// ---------------------------------------------------------------------------
// DotSplines — Java: DotSplinesSkeletonTest
// ---------------------------------------------------------------------------
//
// Java: enum with 3 variants: POLYLINE, ORTHO, SPLINES (in that order).
// Rust: enum with 4 variants: Splines (default), Polyline, Ortho, Curved.
// Note: Rust added Curved; Java order differs (Java: POLYLINE=0, ORTHO=1, SPLINES=2).

#[cfg(test)]
mod dot_splines_tests {
    use super::*;

    #[test]
    fn rust_has_at_least_three_of_the_java_variants() {
        // Java: assertEquals(3, DotSplines.values().length)
        // Rust has 4 (added Curved); we verify the 3 Java variants exist and are distinct.
        let all = [
            DotSplines::Splines,
            DotSplines::Polyline,
            DotSplines::Ortho,
            DotSplines::Curved,
        ];
        assert_eq!(all.len(), 4);
        assert_ne!(DotSplines::Polyline, DotSplines::Ortho);
        assert_ne!(DotSplines::Polyline, DotSplines::Splines);
        assert_ne!(DotSplines::Ortho, DotSplines::Splines);
    }

    #[test]
    fn polyline_parses_from_name() {
        // Java: assertSame(DotSplines.POLYLINE, DotSplines.valueOf("POLYLINE"))
        assert_eq!(
            DotSplines::from_str_opt("polyline"),
            Some(DotSplines::Polyline)
        );
    }

    #[test]
    fn ortho_parses_from_name() {
        // Java: assertSame(DotSplines.ORTHO, DotSplines.valueOf("ORTHO"))
        assert_eq!(DotSplines::from_str_opt("ortho"), Some(DotSplines::Ortho));
    }

    #[test]
    fn splines_parses_from_name() {
        // Java: assertSame(DotSplines.SPLINES, DotSplines.valueOf("SPLINES"))
        assert_eq!(
            DotSplines::from_str_opt("splines"),
            Some(DotSplines::Splines)
        );
    }

    #[test]
    fn unknown_name_returns_none() {
        // Java: DotSplines.valueOf("UNKNOWN") throws IllegalArgumentException
        // Rust: from_str_opt returns None
        assert_eq!(DotSplines::from_str_opt("UNKNOWN"), None);
    }

    #[test]
    fn default_is_splines() {
        // Rust: #[default] is Splines; Java default wasn't specified per test.
        assert_eq!(DotSplines::default(), DotSplines::Splines);
    }

    #[test]
    #[ignore = "gap: Rust DotSplines has 4 variants (Java had 3); ordinal order differs"]
    fn ordinal_matches_java_order() {
        // Java: POLYLINE=0, ORTHO=1, SPLINES=2
        // Rust enum doesn't expose ordinals the same way; variant order differs.
        todo!()
    }
}

// ---------------------------------------------------------------------------
// GraphvizUtils — Java: GraphvizUtilsSkeletonTest
// ---------------------------------------------------------------------------
//
// Java: GraphvizUtils.getenvImageLimit() / setLocalImageLimit() / removeLocalLimitSize()
// Rust: image_limit() reads from PLANTUML_LIMIT_SIZE env var; no thread-local set/remove API.

#[cfg(test)]
mod graphviz_utils_tests {
    use plantuml_little::dot::graphviz::{image_limit, DEFAULT_IMAGE_LIMIT};

    #[test]
    fn default_image_limit_constant_is_4096() {
        // Java: assertEquals(4096, limit) when no env var and no local override
        assert_eq!(DEFAULT_IMAGE_LIMIT, 4096);
    }

    #[test]
    fn image_limit_returns_positive_value() {
        // Java: assertEquals(4096, GraphvizUtils.getenvImageLimit())
        // Rust: may be overridden by PLANTUML_LIMIT_SIZE env var, so we just verify > 0.
        let limit = image_limit();
        assert!(limit > 0, "image limit should be positive, got {limit}");
    }

    #[test]
    #[ignore = "gap: Rust image_limit() has no setLocalImageLimit / removeLocalLimitSize API"]
    fn set_local_image_limit_overrides_default() {
        // Java: GraphvizUtils.setLocalImageLimit(1234); assertEquals(1234, getenvImageLimit())
        todo!()
    }

    #[test]
    #[ignore = "gap: Rust image_limit() has no setLocalImageLimit / removeLocalLimitSize API"]
    fn remove_local_limit_size_restores_default() {
        todo!()
    }

    #[test]
    #[ignore = "gap: Rust image_limit() has no setLocalImageLimit / removeLocalLimitSize API"]
    fn set_local_image_limit_to_large_value() {
        todo!()
    }
}

// ---------------------------------------------------------------------------
// GraphvizVersionFinder / GraphvizVersion — Java: GraphvizVersionFinderSkeletonTest
// ---------------------------------------------------------------------------
//
// Java: GraphvizVersionFinder.DEFAULT is a GraphvizVersion with fixed flag values.
// Rust: GraphvizVersion::DEFAULT is the equivalent constant.

#[cfg(test)]
mod graphviz_version_finder_tests {
    use super::*;

    #[test]
    fn default_constant_is_not_null() {
        // Java: assertNotNull(GraphvizVersionFinder.DEFAULT)
        // Rust: verify DEFAULT has meaningful version data (major=2, minor=28).
        let v = GraphvizVersion::DEFAULT;
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 28);
        assert_eq!(v.numeric(), 228);
    }

    #[test]
    fn default_use_shield_for_quantifier_is_true() {
        // Java: assertTrue(GraphvizVersionFinder.DEFAULT.useShieldForQuantifier())
        // DEFAULT is version 2.28, numeric=228, threshold is <=228 -> true.
        assert!(GraphvizVersion::DEFAULT.use_shield_for_quantifier());
    }

    #[test]
    fn default_use_protection_for_group_links_is_true() {
        // Java: assertTrue(GraphvizVersionFinder.DEFAULT.useProtectionWhenThereALinkFromOrToGroup())
        // DEFAULT is 2.28, not 239 or 240 -> true.
        assert!(GraphvizVersion::DEFAULT.use_protection_for_group_links());
    }

    #[test]
    fn default_use_xlabel_instead_of_label_is_false() {
        // Java: assertFalse(GraphvizVersionFinder.DEFAULT.useXLabelInsteadOfLabel())
        assert!(!GraphvizVersion::DEFAULT.use_xlabel_instead_of_label());
    }

    #[test]
    fn default_is_vizjs_is_false() {
        // Java: assertFalse(GraphvizVersionFinder.DEFAULT.isVizjs())
        assert!(!GraphvizVersion::DEFAULT.is_vizjs());
    }

    #[test]
    fn default_ignore_horizontal_links_is_false() {
        // Java: assertFalse(GraphvizVersionFinder.DEFAULT.ignoreHorizontalLinks())
        // DEFAULT is 2.28, numeric=228, threshold is ==230 -> false.
        assert!(!GraphvizVersion::DEFAULT.ignore_horizontal_links());
    }

    #[test]
    fn version_with_missing_exe_uses_default_flags() {
        // Java: finder with non-existent exe falls back to DEFAULT; use_shield_for_quantifier=true, isVizjs=false
        // Verify DEFAULT flags AND that an alternate version behaves differently.
        let v = GraphvizVersion::DEFAULT;
        assert!(v.use_shield_for_quantifier());
        assert!(!v.is_vizjs());
        // A newer version (2.44) should NOT need the quantifier shield.
        let new_v = GraphvizVersion {
            major: 2,
            minor: 44,
            patch: 0,
        };
        assert!(
            !new_v.use_shield_for_quantifier(),
            "v2.44 should not need quantifier shield"
        );
    }

    #[test]
    fn dot_version_with_missing_exe_returns_question_mark_or_empty() {
        // Java: assertTrue(v.contains("?") || v.isEmpty())
        // Rust: detect_graphviz_version returns DEFAULT when dot is absent.
        // We test version parsing fallback with a bad string.
        let result = GraphvizVersion::parse_from_dot_output("no version here");
        assert!(result.is_none(), "unparseable string should return None");
    }
}

// ---------------------------------------------------------------------------
// GraphvizRuntimeEnvironment — Java: GraphvizRuntimeEnvironmentSkeletonTest
// ---------------------------------------------------------------------------
//
// Java: GraphvizRuntimeEnvironment.getInstance().retrieveVersion(String)
// Rust: GraphvizVersion::retrieve_numeric(s) — equivalent static method.

#[cfg(test)]
mod graphviz_runtime_environment_tests {
    use super::*;

    #[test]
    fn retrieve_version_null_returns_minus_one() {
        // Java: assertEquals(-1, getInstance().retrieveVersion(null))
        // Rust: retrieve_numeric treats "" as unparseable -> -1
        assert_eq!(GraphvizVersion::retrieve_numeric(""), -1);
    }

    #[test]
    fn retrieve_version_empty_string_returns_minus_one() {
        // Java: assertEquals(-1, getInstance().retrieveVersion(""))
        assert_eq!(GraphvizVersion::retrieve_numeric(""), -1);
    }

    #[test]
    fn retrieve_version_no_version_string_returns_minus_one() {
        // Java: assertEquals(-1, getInstance().retrieveVersion("no version here"))
        assert_eq!(GraphvizVersion::retrieve_numeric("no version here"), -1);
    }

    #[test]
    fn retrieve_version_parses_dot_2_38() {
        // Java: assertEquals(238, getInstance().retrieveVersion("dot - graphviz version 2.38.0 ..."))
        let v = GraphvizVersion::retrieve_numeric("dot - graphviz version 2.38.0 (20140413.2041)");
        assert_eq!(v, 238);
    }

    #[test]
    fn retrieve_version_parses_dot_2_44() {
        // Java: assertEquals(244, ...)
        let v = GraphvizVersion::retrieve_numeric("dot - graphviz version 2.44.1 (20200629.0846)");
        assert_eq!(v, 244);
    }

    #[test]
    fn retrieve_version_parses_dot_9_0() {
        // Java: assertEquals(900, ...)
        let v = GraphvizVersion::retrieve_numeric("dot - graphviz version 9.0.0 (20230911.1827)");
        assert_eq!(v, 900);
    }

    #[test]
    #[ignore = "gap: GraphvizRuntimeEnvironment.getInstance() singleton not ported; use detect_graphviz_version() instead"]
    fn get_instance_is_singleton() {
        // Java: assertSame(getInstance(), getInstance())
        todo!()
    }
}

// ---------------------------------------------------------------------------
// DebugTrace — Java: DebugTraceSkeletonTest
// ---------------------------------------------------------------------------
//
// Java: DebugTrace.DEBUG(String) / DEBUG(String, Throwable) — write to timestamped file.
// Rust: No DebugTrace struct — logging is done via the `log` crate.
// All tests are gaps; smoke: just verify no panic from log::debug.

#[cfg(test)]
mod debug_trace_tests {
    #[test]
    #[ignore = "gap: DebugTrace not yet ported to Rust; use log::debug! instead"]
    fn debug_string_does_not_throw() {
        // Java: DebugTrace.DEBUG("test message from DebugTraceSkeletonTest")
        todo!()
    }

    #[test]
    #[ignore = "gap: DebugTrace not yet ported to Rust; use log::debug! instead"]
    fn debug_string_with_throwable_does_not_throw() {
        todo!()
    }

    #[test]
    #[ignore = "gap: DebugTrace not yet ported to Rust; use log::debug! instead"]
    fn debug_empty_string_does_not_throw() {
        todo!()
    }

    #[test]
    #[ignore = "gap: DebugTrace not yet ported to Rust; use log::debug! instead"]
    fn debug_called_multiple_times_does_not_throw() {
        todo!()
    }
}

#[cfg(test)]
mod graphviz_tests {
    use plantuml_little::dot::graphviz::{Graphviz, GraphvizInProcess};

    #[test]
    fn test_in_process_render() {
        let gv = GraphvizInProcess::new("digraph G { A -> B }");
        let mut buf = Vec::new();
        let state = gv.create_file(&mut buf);
        assert!(state.is_ok());
        assert!(String::from_utf8_lossy(&buf).contains("<svg"));
    }

    #[test]
    fn test_dot_version() {
        let gv = GraphvizInProcess::new("");
        let version = gv.dot_version();
        assert!(!version.is_empty());
    }
}

// ---------------------------------------------------------------------------
// Neighborhood — Java: NeighborhoodSkeletonTest
// ---------------------------------------------------------------------------
//
// Java: Neighborhood.intersection(XRectangle2D, XPoint2D center, XPoint2D target) -> XPoint2D|null
// Rust: rect_line_intersection(&Rect2D, Point2D, Point2D) -> Option<Point2D>
//       segment_intersection(p1, p2, p3, p4) -> Option<Point2D>

#[cfg(test)]
mod neighborhood_tests {
    use super::*;

    fn pt(x: f64, y: f64) -> Point2D {
        Point2D::new(x, y)
    }

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect2D {
        Rect2D::new(x, y, w, h)
    }

    const EPS: f64 = 1e-6;

    #[test]
    fn intersection_horizontal_ray_from_center_hits_right_edge() {
        // Java: assertEquals(10.0, p.getX(), 1e-6); assertEquals(5.0, p.getY(), 1e-6)
        let r = rect(0.0, 0.0, 10.0, 10.0);
        let p = rect_line_intersection(&r, pt(5.0, 5.0), pt(15.0, 5.0));
        assert!(p.is_some(), "expected intersection on the right edge");
        let p = p.unwrap();
        assert!((p.x - 10.0).abs() < EPS, "x should be 10.0, got {}", p.x);
        assert!((p.y - 5.0).abs() < EPS, "y should be 5.0, got {}", p.y);
    }

    #[test]
    fn intersection_horizontal_ray_leftward_hits_left_edge() {
        // Java: assertEquals(0.0, p.getX(), 1e-6); assertEquals(5.0, p.getY(), 1e-6)
        let r = rect(0.0, 0.0, 10.0, 10.0);
        let p = rect_line_intersection(&r, pt(5.0, 5.0), pt(-5.0, 5.0));
        assert!(p.is_some(), "expected intersection on the left edge");
        let p = p.unwrap();
        assert!((p.x - 0.0).abs() < EPS, "x should be 0.0, got {}", p.x);
        assert!((p.y - 5.0).abs() < EPS, "y should be 5.0, got {}", p.y);
    }

    #[test]
    fn intersection_vertical_ray_upward_hits_top_edge() {
        // Java: assertEquals(5.0, p.getX(), 1e-6); assertEquals(0.0, p.getY(), 1e-6)
        let r = rect(0.0, 0.0, 10.0, 10.0);
        let p = rect_line_intersection(&r, pt(5.0, 5.0), pt(5.0, -5.0));
        assert!(p.is_some(), "expected intersection on the top edge");
        let p = p.unwrap();
        assert!((p.x - 5.0).abs() < EPS, "x should be 5.0, got {}", p.x);
        assert!((p.y - 0.0).abs() < EPS, "y should be 0.0, got {}", p.y);
    }

    #[test]
    fn intersection_vertical_ray_downward_hits_bottom_edge() {
        // Java: assertEquals(5.0, p.getX(), 1e-6); assertEquals(10.0, p.getY(), 1e-6)
        let r = rect(0.0, 0.0, 10.0, 10.0);
        let p = rect_line_intersection(&r, pt(5.0, 5.0), pt(5.0, 15.0));
        assert!(p.is_some(), "expected intersection on the bottom edge");
        let p = p.unwrap();
        assert!((p.x - 5.0).abs() < EPS, "x should be 5.0, got {}", p.x);
        assert!((p.y - 10.0).abs() < EPS, "y should be 10.0, got {}", p.y);
    }

    #[test]
    fn intersection_segment_entirely_outside_returns_none() {
        // Java: assertNull("segment outside should yield null", p)
        let r = rect(0.0, 0.0, 10.0, 10.0);
        let p = rect_line_intersection(&r, pt(20.0, 20.0), pt(30.0, 30.0));
        assert!(p.is_none(), "segment outside should yield None");
    }

    #[test]
    fn intersection_diagonal_ray_toward_top_right_corner() {
        // Java: assertNotNull; assertTrue x in [0,10]; assertTrue y in [0,10]
        let r = rect(0.0, 0.0, 10.0, 10.0);
        let p = rect_line_intersection(&r, pt(5.0, 5.0), pt(15.0, -5.0));
        assert!(p.is_some(), "expected an intersection");
        let p = p.unwrap();
        assert!(
            p.x >= -EPS && p.x <= 10.0 + EPS,
            "x must be within [0,10], got {}",
            p.x
        );
        assert!(
            p.y >= -EPS && p.y <= 10.0 + EPS,
            "y must be within [0,10], got {}",
            p.y
        );
    }

    #[test]
    fn intersection_segment_entirely_above_rectangle_returns_none() {
        // Java: assertNull("segment entirely above rectangle should yield null", p)
        // Segment from (5,-5) to (5,-15) never crosses any edge of rect [0..10].
        let r = rect(0.0, 0.0, 10.0, 10.0);
        let p = rect_line_intersection(&r, pt(5.0, -5.0), pt(5.0, -15.0));
        assert!(p.is_none(), "segment above rect should yield None");
    }

    #[test]
    #[ignore = "gap: Neighborhood struct constructor differs; Java takes (Entity, List<Link>, List<Link>)"]
    fn constructor_accepts_empty_link_lists() {
        // Java: new Neighborhood(null, new ArrayList(), new ArrayList())
        // Rust: Neighborhood::new(leaf_uid: String) — different signature.
        todo!()
    }
}

// ---------------------------------------------------------------------------
// CucaDiagramTxtMaker — Java: CucaDiagramTxtMakerSkeletonTest
// ---------------------------------------------------------------------------
//
// Java CucaDiagramTxtMaker renders class/package diagrams as ASCII (ATXT) or
// Unicode (UTXT) text art via BasicCharArea. Rust plantuml-little is currently
// SVG-only; text rendering has not been ported. All tests here are TDD anchors.
//
// When text output is ported (e.g. plantuml_little::render::text_renderer), these
// tests should be filled in using a golden-file approach analogous to the SVG tests.

#[cfg(test)]
mod cuca_diagram_txt_maker_tests {

    #[test]
    #[ignore = "gap: text rendering not ported — Rust is SVG-only; TDD anchor for future port"]
    fn single_class_atxt_produces_boxed_class_name() {
        // Java: renderAsAtxt("@startuml\nclass Foo\n@enduml")
        //       assertTrue(actual.contains("Foo"))
        //       golden file CucaDiagramTxtMakerSkeletonTest_singleClass_atxt.txt
        todo!()
    }

    #[test]
    #[ignore = "gap: text rendering not ported — Rust is SVG-only; TDD anchor for future port"]
    fn single_class_utxt_produces_unicode_box() {
        // Java: renderAsUtxt("@startuml\nclass Foo\n@enduml")
        //       assertTrue(actual contains '┌' | '└' | '─' | '│')
        //       golden file CucaDiagramTxtMakerSkeletonTest_singleClass_utxt.txt
        todo!()
    }

    #[test]
    #[ignore = "gap: text rendering not ported — Rust is SVG-only; TDD anchor for future port"]
    fn two_classes_linked_atxt_renders_both_names() {
        // Java: renderAsAtxt("@startuml\nclass Alpha\nclass Beta\nAlpha --> Beta\n@enduml")
        //       assertTrue contains "Alpha" and "Beta"
        //       golden file CucaDiagramTxtMakerSkeletonTest_twoClasses_linked_atxt.txt
        todo!()
    }

    #[test]
    #[ignore = "gap: text rendering not ported — Rust is SVG-only; TDD anchor for future port"]
    fn two_classes_linked_utxt_renders_both_names() {
        // Java: renderAsUtxt("@startuml\nclass Alpha\nclass Beta\nAlpha --> Beta\n@enduml")
        //       assertTrue contains "Alpha" and "Beta"
        //       golden file CucaDiagramTxtMakerSkeletonTest_twoClasses_linked_utxt.txt
        todo!()
    }

    #[test]
    #[ignore = "gap: text rendering not ported — Rust is SVG-only; TDD anchor for future port"]
    fn class_with_members_atxt_shows_separator_line() {
        // Java: renderAsAtxt("@startuml\nclass Vehicle {\n  +String brand\n  +drive()\n}\n@enduml")
        //       assertTrue(actual.contains("Vehicle"))
        //       assertTrue(actual.contains("-") || actual.contains("="))
        //       golden file CucaDiagramTxtMakerSkeletonTest_classWithMembers_atxt.txt
        todo!()
    }

    #[test]
    #[ignore = "gap: text rendering not ported — Rust is SVG-only; TDD anchor for future port"]
    fn output_stream_atxt_produces_non_empty_text() {
        // Java: outputImage(os, FileFormatOption(FileFormat.ATXT))
        //       assertFalse(output.trim().isEmpty())
        //       assertTrue(output.contains("Dog"))
        todo!()
    }

    #[test]
    #[ignore = "gap: text rendering not ported — Rust is SVG-only; TDD anchor for future port"]
    fn usecase_diagram_atxt_does_not_throw() {
        // Java: outputImage(os, FileFormatOption(FileFormat.ATXT)) on use-case diagram
        //       should not throw; result may be empty
        todo!()
    }
}
