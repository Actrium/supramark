//! Phase 4 spike: measure drift between TtfParserMetrics (production candidate)
//! and StaticDejaVuMetrics (byte-equal Java reference) across a broad stimulus
//! set. Prints data; doesn't assert. Run via:
//!   cargo test -p font-metrics --features static-fixtures --test dynamic_vs_static_drift_spike -- --nocapture
//!
//! The team reads the printed table to decide whether ttf-parser drift is
//! acceptable for the unified production backend.

#![cfg(feature = "static-fixtures")]

use font_metrics::static_dejavu::StaticDejaVuMetrics;
use font_metrics::ttf_parser::TtfParserMetrics;
use font_metrics::Metrics;

struct Stimulus {
    text: String,
    family: &'static str,
    size: f64,
    bold: bool,
    italic: bool,
    label: &'static str,
}

fn stimuli() -> Vec<Stimulus> {
    let mut v = Vec::new();
    // Common ASCII characters at typical UML sizes.
    for size in [10.0, 12.0, 14.0, 16.0, 18.0, 24.0] {
        for family in ["Sans", "Monospaced"] {
            for ch in ['A', 'a', 'g', 'M', 'i', 'W', '0', ' ', 'p', 'q'] {
                v.push(Stimulus {
                    text: ch.to_string(),
                    family,
                    size,
                    bold: false,
                    italic: false,
                    label: "single char",
                });
            }
        }
    }
    // Typical UML diagram labels.
    let labels = [
        "Class",
        "Component",
        "Server",
        "Database",
        "ServiceA",
        "Customer",
        "Order",
        "Payment",
        "Invoice",
        "User",
        "Admin",
        "Guest",
        "GET /api/users",
        "HTTPRequest",
        "RFC 1918",
        "interface MyService",
        "abstract class BaseEntity",
    ];
    for size in [12.0, 14.0, 16.0, 20.0] {
        for family in ["Sans", "Monospaced"] {
            for bold in [false, true] {
                for &label in &labels {
                    v.push(Stimulus {
                        text: label.to_string(),
                        family,
                        size,
                        bold,
                        italic: false,
                        label: "uml label",
                    });
                }
            }
        }
    }
    // Multi-line text.
    let multi = ["line1\nline2", "header\n=======\nbody"];
    for size in [12.0, 16.0] {
        for &t in &multi {
            v.push(Stimulus {
                text: t.to_string(),
                family: "Sans",
                size,
                bold: false,
                italic: false,
                label: "multi-line",
            });
        }
    }
    v
}

#[test]
fn print_drift_report() {
    let s = StaticDejaVuMetrics;
    let d = TtfParserMetrics::default_latin().expect("ttf parser init");
    let stimuli = stimuli();

    println!();
    println!("=== Phase 4 prelim: TtfParser vs StaticDejaVu drift ===");
    println!("Stimulus count: {}", stimuli.len());
    println!();
    println!("Width drift stats:");
    let mut width_diffs = Vec::new();
    let mut asc_diffs = Vec::new();
    let mut desc_diffs = Vec::new();
    let mut worst_width: Option<(f64, &Stimulus)> = None;
    let mut worst_asc: Option<(f64, &Stimulus)> = None;
    let mut worst_desc: Option<(f64, &Stimulus)> = None;

    for st in &stimuli {
        let sm = s.measure(&st.text, st.family, st.size, st.bold, st.italic);
        let dm = d.measure(&st.text, st.family, st.size, st.bold, st.italic);
        let dw = (sm.width - dm.width).abs();
        let da = (sm.ascent - dm.ascent).abs();
        let dd = (sm.descent - dm.descent).abs();
        width_diffs.push(dw);
        asc_diffs.push(da);
        desc_diffs.push(dd);
        if worst_width.map(|(w, _)| dw > w).unwrap_or(true) {
            worst_width = Some((dw, st));
        }
        if worst_asc.map(|(a, _)| da > a).unwrap_or(true) {
            worst_asc = Some((da, st));
        }
        if worst_desc.map(|(de, _)| dd > de).unwrap_or(true) {
            worst_desc = Some((dd, st));
        }
    }

    let avg = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let max = |v: &[f64]| v.iter().cloned().fold(0.0_f64, f64::max);
    let p99 = |mut v: Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[(v.len() as f64 * 0.99) as usize]
    };

    println!(
        "  width:   avg={:.3}px  max={:.3}px  p99={:.3}px",
        avg(&width_diffs),
        max(&width_diffs),
        p99(width_diffs.clone())
    );
    println!(
        "  ascent:  avg={:.3}px  max={:.3}px  p99={:.3}px",
        avg(&asc_diffs),
        max(&asc_diffs),
        p99(asc_diffs.clone())
    );
    println!(
        "  descent: avg={:.3}px  max={:.3}px  p99={:.3}px",
        avg(&desc_diffs),
        max(&desc_diffs),
        p99(desc_diffs.clone())
    );
    println!();
    println!("Worst cases:");
    if let Some((dw, st)) = worst_width {
        println!(
            "  width:   {:.3}px on '{}' family={} size={} bold={}",
            dw,
            st.text.replace('\n', "\\n"),
            st.family,
            st.size,
            st.bold
        );
    }
    if let Some((da, st)) = worst_asc {
        println!(
            "  ascent:  {:.3}px on '{}' family={} size={} bold={}",
            da,
            st.text.replace('\n', "\\n"),
            st.family,
            st.size,
            st.bold
        );
    }
    if let Some((dd, st)) = worst_desc {
        println!(
            "  descent: {:.3}px on '{}' family={} size={} bold={}",
            dd,
            st.text.replace('\n', "\\n"),
            st.family,
            st.size,
            st.bold
        );
    }
    println!();
    println!("Per-stimulus detail (top 20 by width drift):");
    let mut indexed: Vec<(f64, &Stimulus)> = stimuli
        .iter()
        .map(|st| {
            let sm = s.measure(&st.text, st.family, st.size, st.bold, st.italic);
            let dm = d.measure(&st.text, st.family, st.size, st.bold, st.italic);
            ((sm.width - dm.width).abs(), st)
        })
        .collect();
    indexed.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    for (drift, st) in indexed.iter().take(20) {
        let sm = s.measure(&st.text, st.family, st.size, st.bold, st.italic);
        let dm = d.measure(&st.text, st.family, st.size, st.bold, st.italic);
        println!(
            "  {:>6.3}px  static={:.3} dynamic={:.3} | '{}' [{}, size={}, {}]",
            drift,
            sm.width,
            dm.width,
            st.text.replace('\n', "\\n"),
            st.family,
            st.size,
            if st.bold { "bold" } else { "regular" }
        );
    }
    println!();
    println!("Stimulus category breakdown:");
    let mut by_label: std::collections::BTreeMap<&str, Vec<f64>> =
        std::collections::BTreeMap::new();
    for st in &stimuli {
        let sm = s.measure(&st.text, st.family, st.size, st.bold, st.italic);
        let dm = d.measure(&st.text, st.family, st.size, st.bold, st.italic);
        by_label
            .entry(st.label)
            .or_default()
            .push((sm.width - dm.width).abs());
    }
    for (label, v) in &by_label {
        println!(
            "  [{:<12}] count={:>4}  avg_width_drift={:.3}px  max={:.3}px",
            label,
            v.len(),
            avg(v),
            max(v)
        );
    }
    println!();
    println!("=== End drift report ===");
}
