use d2_little::fonts::{FONT_SIZE_M, Font, FontFamily, FontStyle};
use d2_little::textmeasure::Ruler;

fn main() {
    let mut ruler = Ruler::new().unwrap();
    let font = Font {
        family: FontFamily::SourceSansPro,
        style: FontStyle::Regular,
        size: FONT_SIZE_M,
    };
    let tests = [
        "a",
        "b",
        "c",
        "h",
        "l",
        "ab",
        "Hello",
        "Hello World",
        "The quick brown fox",
    ];
    println!("=== Rust d2-textmeasure vs Go textmeasure ===");
    for t in &tests {
        let (w, h) = ruler.measure_precise(font, t);
        println!("{:<22} width={:<12} height={}", t, w, h);
    }

    // Golden values from Go:
    //   'a'           -> (7.0,  20.125)
    //   'b'           -> (8.0,  20.125)
    //   'c'           -> (7.0,  20.125)
    //   'h'           -> (7.0,  20.125)
    //   'l'           -> (3.0,  20.125)
    //   'ab'          -> (17.0625, 20.125)
    //   'Hello'       -> (33.53125, 20.125)
    //   'Hello World' -> (76.28125, 20.125)
    let checks: &[(&str, (f64, f64))] = &[
        ("a", (7.0, 20.125)),
        ("b", (8.0, 20.125)),
        ("c", (7.0, 20.125)),
        ("h", (7.0, 20.125)),
        ("l", (3.0, 20.125)),
        ("ab", (17.0625, 20.125)),
        ("Hello", (33.53125, 20.125)),
        ("Hello World", (76.28125, 20.125)),
    ];

    let mut all_ok = true;
    for (s, (ew, eh)) in checks {
        let (w, h) = ruler.measure_precise(font, s);
        let ok = w == *ew && h == *eh;
        if !ok {
            println!(
                "MISMATCH '{}': got ({}, {}), want ({}, {})",
                s, w, h, ew, eh
            );
            all_ok = false;
        }
    }
    if all_ok {
        println!("\nAll golden values match Go.");
    } else {
        println!("\nMismatches present.");
    }
}
