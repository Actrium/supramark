// Integration-test shim: pull in the pure mapping helpers from build_helpers.rs
// (which is also `include!`-d by build.rs) and run the unit-tests there.
//
// This way the tests are compiled by `cargo test --tests` without touching
// src/lib.rs and without duplicating any logic.

include!("../src/build_helpers.rs");
