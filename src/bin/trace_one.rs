use mermaid_little::layout::gantt as layout_mod;
use mermaid_little::parser::gantt as parser_mod;
use mermaid_little::theme::get_theme;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let f = &args[1];
    let source = std::fs::read_to_string(f).unwrap();
    let d = parser_mod::parse(&source).unwrap();
    let theme = get_theme("default");
    let l = layout_mod::layout(&d, &theme).unwrap();
    println!("min_ms={}, max_ms={}, span_days={:.3}", l.min_time_ms, l.max_time_ms, (l.max_time_ms - l.min_time_ms)/86400000.0);
    for t in &l.tasks {
        println!("TASK {}: start={} end={} render_end={:?}", t.id, t.start_ms, t.end_ms, t.render_end_ms);
    }
    println!("ticks: {}", l.axis_ticks.len());
}
