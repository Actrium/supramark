fn main() {
    let mut ruler = d2_little::textmeasure::Ruler::new().unwrap();
    let f1 = d2_little::fonts::Font::new(
        d2_little::fonts::FontFamily::SourceSansPro,
        d2_little::fonts::FontStyle::Bold,
        24,
    );
    let f2 = d2_little::fonts::Font::new(
        d2_little::fonts::FontFamily::SourceSansPro,
        d2_little::fonts::FontStyle::Regular,
        24,
    );
    println!("Table bold 24 SSPro: {:?}", ruler.measure(f1, "Table"));
    println!("Table reg 24 SSPro: {:?}", ruler.measure(f2, "Table"));
}
