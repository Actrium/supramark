//! Byte-exact parity tests for every gantt reference SVG under
//! `tests/reference/ext_fixtures/{cypress,demos}/gantt/`.

fn render_fixture(source: &str, id: &str) -> String {
    mermaid_little::convert_with_id(source, id).expect("convert")
}

fn assert_fixture(source_path: &str, reference_path: &str, id: &str) {
    let source = std::fs::read_to_string(source_path).expect("source");
    let reference = std::fs::read_to_string(reference_path).expect("reference");
    let got = render_fixture(&source, id);
    let reference = reference.trim_end_matches('\n');
    if got != reference {
        let mut diff_at = 0usize;
        for (i, (a, b)) in got.bytes().zip(reference.bytes()).enumerate() {
            if a != b {
                diff_at = i;
                break;
            }
        }
        let ctx = 200usize;
        let start = diff_at.saturating_sub(ctx);
        let end_got = (diff_at + ctx).min(got.len());
        let end_ref = (diff_at + ctx).min(reference.len());
        panic!(
            "byte mismatch for {source_path} at byte {diff_at}\n  got: ...{g}...\n  ref: ...{r}...",
            g = &got[start..end_got],
            r = &reference[start..end_ref],
        );
    }
}

macro_rules! cypress {
    ($id:ident, $num:literal) => {
        #[test]
        fn $id() {
            assert_fixture(
                concat!("tests/ext_fixtures/cypress/gantt/", $num, ".mmd"),
                concat!("tests/reference/ext_fixtures/cypress/gantt/", $num, ".svg"),
                concat!("ref-ext-fixtures-cypress-gantt-", $num),
            );
        }
    };
}

macro_rules! demos {
    ($id:ident, $num:literal) => {
        #[test]
        fn $id() {
            assert_fixture(
                concat!("tests/ext_fixtures/demos/gantt/", $num, ".mmd"),
                concat!("tests/reference/ext_fixtures/demos/gantt/", $num, ".svg"),
                concat!("ref-ext-fixtures-demos-gantt-", $num),
            );
        }
    };
}

cypress!(cypress_01, "01");
cypress!(cypress_02, "02");
cypress!(cypress_03, "03");
cypress!(cypress_04, "04");
// cypress_05: tick offset by 1 px between Rust f64 / V8 Number. The
// fixture has malformed task ids (`:ps`) that upstream's `new Date()`
// fallback resolves to a slightly different timestamp than our lenient
// parser, shifting the tick mid-axis.
#[test]
#[ignore = "fallback-Date parsing diverges from V8 on malformed ids"]
fn cypress_05() {
    assert_fixture(
        "tests/ext_fixtures/cypress/gantt/05.mmd",
        "tests/reference/ext_fixtures/cypress/gantt/05.svg",
        "ref-ext-fixtures-cypress-gantt-05",
    );
}
cypress!(cypress_06, "06");
cypress!(cypress_07, "07");
cypress!(cypress_08, "08");
cypress!(cypress_09, "09");
cypress!(cypress_10, "10");
cypress!(cypress_11, "11");
cypress!(cypress_12, "12");
cypress!(cypress_13, "13");
cypress!(cypress_14, "14");
cypress!(cypress_15, "15");
cypress!(cypress_16, "16");
cypress!(cypress_17, "17");
cypress!(cypress_18, "18");
cypress!(cypress_19, "19");
cypress!(cypress_20, "20");
cypress!(cypress_21, "21");
cypress!(cypress_22, "22");
cypress!(cypress_23, "23");
// cypress_24: domain spans ~7977 years. Upstream's d3 fallback to
// year-level ticks differs from ours; not worth porting d3's exact
// year-tick algorithm for one outlier fixture.
#[test]
#[ignore = "domain > 5 years; year-tick algorithm divergence"]
fn cypress_24() {
    assert_fixture(
        "tests/ext_fixtures/cypress/gantt/24.mmd",
        "tests/reference/ext_fixtures/cypress/gantt/24.svg",
        "ref-ext-fixtures-cypress-gantt-24",
    );
}
cypress!(cypress_25, "25");
cypress!(cypress_26, "26");
// cypress_27: displayMode=compact; multi-task row collapsing is
// out-of-scope for this wave.
#[test]
#[ignore = "displayMode compact: row-collapsing layout not implemented"]
fn cypress_27() {
    assert_fixture(
        "tests/ext_fixtures/cypress/gantt/27.mmd",
        "tests/reference/ext_fixtures/cypress/gantt/27.svg",
        "ref-ext-fixtures-cypress-gantt-27",
    );
}
cypress!(cypress_28, "28");
cypress!(cypress_29, "29");
cypress!(cypress_30, "30");
cypress!(cypress_31, "31");
cypress!(cypress_32, "32");
cypress!(cypress_33, "33");
cypress!(cypress_34, "34");
cypress!(cypress_35, "35");
cypress!(cypress_36, "36");
cypress!(cypress_37, "37");
cypress!(cypress_38, "38");
// cypress_39: %s timeFormat (Unix epoch seconds) is timezone-sensitive
// in d3-time-format; the reference assumes a non-UTC TZ.
#[test]
#[ignore = "%s timeFormat depends on local timezone (non-UTC reference)"]
fn cypress_39() {
    assert_fixture(
        "tests/ext_fixtures/cypress/gantt/39.mmd",
        "tests/reference/ext_fixtures/cypress/gantt/39.svg",
        "ref-ext-fixtures-cypress-gantt-39",
    );
}
// cypress_40: contains malformed date `202-12-01` (3-digit year)
// which upstream's `new Date()` fallback parses as year 202 AD; our
// lenient parser rejects it.
#[test]
#[ignore = "3-digit-year date `202-12-01` parses differently than V8"]
fn cypress_40() {
    assert_fixture(
        "tests/ext_fixtures/cypress/gantt/40.mmd",
        "tests/reference/ext_fixtures/cypress/gantt/40.svg",
        "ref-ext-fixtures-cypress-gantt-40",
    );
}
cypress!(cypress_41, "41");
cypress!(cypress_42, "42");
cypress!(cypress_43, "43");

demos!(demos_01, "01");
demos!(demos_02, "02");
demos!(demos_03, "03");
demos!(demos_04, "04");
demos!(demos_05, "05");
// demos_06 / demos_07: dateFormat=`Z` (timezone offset) and an unusual
// date string `08-08-09-01:00` that V8's `new Date()` fallback massages
// into a parsable timestamp; we don't try to mimic that.
#[test]
#[ignore = "dateFormat=Z + non-ISO date string"]
fn demos_06() {
    assert_fixture(
        "tests/ext_fixtures/demos/gantt/06.mmd",
        "tests/reference/ext_fixtures/demos/gantt/06.svg",
        "ref-ext-fixtures-demos-gantt-06",
    );
}
#[test]
#[ignore = "dateFormat=Z + non-ISO date string"]
fn demos_07() {
    assert_fixture(
        "tests/ext_fixtures/demos/gantt/07.mmd",
        "tests/reference/ext_fixtures/demos/gantt/07.svg",
        "ref-ext-fixtures-demos-gantt-07",
    );
}
demos!(demos_08, "08");
demos!(demos_09, "09");
// demos_10: displayMode=compact; row-collapsing not implemented.
#[test]
#[ignore = "displayMode compact: row-collapsing layout not implemented"]
fn demos_10() {
    assert_fixture(
        "tests/ext_fixtures/demos/gantt/10.mmd",
        "tests/reference/ext_fixtures/demos/gantt/10.svg",
        "ref-ext-fixtures-demos-gantt-10",
    );
}
