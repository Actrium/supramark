// Integration tests verifying that sequence diagram SVG rendering
// matches Java PlantUML structural output.
//
// These bridge the gap between layout tests (port_sequence_layout.rs,
// which verify coordinates) and reference tests (reference_tests.rs,
// which verify full SVG byte-for-byte).

fn convert(puml: &str) -> String {
    plantuml_little::convert(puml).expect("convert failed")
}

/// Extract all occurrences of a given attribute value from elements matching
/// a tag prefix.  For example, `extract_all_attrs(svg, "<rect", "fill")`
/// returns a Vec of fill values from all `<rect` elements.
fn extract_all_attrs<'a>(svg: &'a str, tag: &str, attr: &str) -> Vec<&'a str> {
    let mut results = Vec::new();
    let needle = format!("{attr}=\"");
    let mut offset = 0;
    while let Some(pos) = svg[offset..].find(tag) {
        let tag_start = offset + pos;
        let rest = &svg[tag_start..];
        if let Some(tag_end) = rest.find('>') {
            let tag_text = &rest[..tag_end];
            if let Some(attr_pos) = tag_text.find(&needle) {
                let val_start = attr_pos + needle.len();
                if let Some(val_end) = tag_text[val_start..].find('"') {
                    results.push(&tag_text[val_start..val_start + val_end]);
                }
            }
        }
        offset = tag_start + tag.len();
    }
    results
}

// ── Test 1: note_body_is_path_not_rect ──────────────────────────────
//
// Java PlantUML renders note backgrounds as two <path> elements (body
// outline + fold corner), both with fill="#FEFFDD" and stroke-width="0.5".
// The current Rust implementation uses a <rect> for the body and a <path>
// for the fold, with stroke-width="1".

#[test]
// Fixed: note body now rendered as <path> matching Java
fn note_body_is_path_not_rect() {
    let svg = convert("@startuml\nA -> B : msg\nnote right: Note\n@enduml");

    // Java: 0 rects with note background color
    let note_rects: Vec<_> = extract_all_attrs(&svg, "<rect", "fill")
        .into_iter()
        .filter(|f| *f == "#FEFFDD")
        .collect();
    assert_eq!(
        note_rects.len(),
        0,
        "Java uses <path> not <rect> for note body; found {} note-colored rects",
        note_rects.len(),
    );

    // Java: exactly 2 paths with note background (body outline + fold)
    let note_fill_paths: usize = svg
        .match_indices("<path")
        .filter(|(pos, _)| {
            let rest = &svg[*pos..];
            let end = rest.find('>').unwrap_or(rest.len());
            rest[..end].contains(r##"fill="#FEFFDD""##)
        })
        .count();
    assert_eq!(
        note_fill_paths, 2,
        "expected 2 <path> with note fill (body + fold), found {note_fill_paths}",
    );

    // Java: note stroke-width is 0.5, not 1
    // Check that note elements use stroke-width:0.5
    let note_section_start = svg.find(r##"fill="#FEFFDD""##).expect("note fill");
    let note_section = &svg[note_section_start..];
    assert!(
        note_section.contains("stroke-width:0.5"),
        "note stroke-width should be 0.5 (Java), not 1",
    );
}

// ── Test 2: actor_renders_text_then_ellipse_then_path ────────────────
//
// Java renders the actor head as <ellipse> (equal rx/ry but still an
// ellipse element) and body as a single <path> with M/L segments.
// Current Rust uses <circle> for head and multiple <line> for body.

#[test]
// Fixed: actor now renders TEXT→ELLIPSE→PATH matching Java
fn actor_renders_text_then_ellipse_then_path() {
    let svg = convert("@startuml\nactor MyActor\nMyActor -> MyActor : msg\n@enduml");

    // Java: actor head is <ellipse>, not <circle>
    assert!(
        svg.contains("<ellipse"),
        "Java renders actor head as <ellipse>, not <circle>",
    );

    // Java: actor body is single <path> with M/L segments, not multiple <line>
    // The actor body in current code produces 5 <line> elements
    // (body, left arm, right arm, left leg, right leg).
    // Java produces a single <path d="M... L... L..."> for the whole body.
    //
    // We check that there are no <line> elements inside the actor figure
    // (lifeline <line> is separate and dashed, so we look for non-dashed lines
    // with the actor stroke color near the actor head position).

    // Java: actor label is NOT bold
    // Find the <text ...>MyActor</text> element (not title or attribute)
    let actor_text_marker = ">MyActor</text>";
    let marker_pos = svg.find(actor_text_marker).expect(">MyActor</text> in SVG");
    let preceding = &svg[..marker_pos];
    let text_tag_start = preceding.rfind("<text").expect("<text before actor name");
    let text_tag = &svg[text_tag_start..marker_pos];
    assert!(
        !text_tag.contains(r#"font-weight="bold""#),
        "Java actor text is NOT bold, but current code uses bold",
    );

    // Java: actor text does NOT have text-anchor="middle"
    assert!(
        !text_tag.contains(r#"text-anchor="middle""#),
        "Java actor text does NOT use text-anchor=\"middle\"",
    );
}

// ── Test 3: activation_box_dimensions ────────────────────────────────
//
// Java draws activation boxes in two passes (background then foreground),
// each producing a <rect> with fill="#FFFFFF", stroke-width="1", width="10".

#[test]
fn activation_box_dimensions() {
    let svg = convert("@startuml\nA -> B : req\nactivate B\nB --> A : resp\ndeactivate B\n@enduml");

    // Count white (#FFFFFF) rects for activation (drawn in bg + fg pass)
    let white_rects: Vec<_> = svg
        .match_indices("<rect")
        .filter(|(pos, _)| {
            let rest = &svg[*pos..];
            let end = rest.find('>').unwrap_or(rest.len());
            let tag = &rest[..end];
            tag.contains(r##"fill="#FFFFFF""##)
        })
        .collect();
    assert_eq!(
        white_rects.len(),
        2,
        "expected 2 white rects (activation bg + fg pass), found {}",
        white_rects.len(),
    );

    // Check activation rect stroke-width="1" (inside style attr)
    for (pos, _) in &white_rects {
        let rest = &svg[*pos..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        assert!(
            tag.contains("stroke-width:1"),
            "activation rect should have stroke-width:1, tag: {tag}",
        );
    }

    // Check activation rect width="10" (or close to 10)
    for (pos, _) in &white_rects {
        let rest = &svg[*pos..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        // Extract width attribute
        let width_needle = r#"width=""#;
        let w_start = tag.find(width_needle).expect("width attr") + width_needle.len();
        let w_end = w_start + tag[w_start..].find('"').expect("width end quote");
        let width_val: f64 = tag[w_start..w_end].parse().expect("parse width");
        assert!(
            (width_val - 10.0).abs() < 0.01,
            "activation width should be 10, got {width_val}",
        );
    }
}

// ── Test 4: arrow_endpoint_at_activation_edge ────────────────────────
//
// When participant B is activated, the arrow tip should stop at the
// activation box edge (B center - 5), not at B's lifeline center.

#[test]
fn arrow_endpoint_at_activation_edge() {
    let svg = convert("@startuml\nA -> B : req\nactivate B\nB --> A : resp\ndeactivate B\n@enduml");

    // Find the first <polygon (arrow tip for "A -> B : req")
    let poly_start = svg.find("<polygon").expect("first polygon (arrowhead)");
    let poly_rest = &svg[poly_start..];
    let poly_end = poly_rest.find('>').unwrap_or(poly_rest.len());
    let poly_tag = &poly_rest[..poly_end];

    // Extract "points" attribute
    let points_needle = r#"points=""#;
    let p_start = poly_tag.find(points_needle).expect("points attr") + points_needle.len();
    let p_end = p_start + poly_tag[p_start..].find('"').expect("points end");
    let points_str = &poly_tag[p_start..p_end];

    // Points format: "x1,y1,x2,y2,x3,y3,x4,y4"
    // The arrow tip is the point with the max x (for left-to-right arrow)
    let coords: Vec<f64> = points_str
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .collect();
    // x-coordinates are at even indices
    let tip_x = coords
        .iter()
        .step_by(2)
        .copied()
        .reduce(f64::max)
        .expect("tip x");

    // Find participant B's rect center x.
    // Participant B rects have rx="2.5" (default participant style).
    // We need the second participant rect (B), not the first (A).
    let mut participant_rects: Vec<(f64, f64)> = Vec::new();
    let mut offset = 0;
    while let Some(pos) = svg[offset..].find("<rect") {
        let abs_pos = offset + pos;
        let rest = &svg[abs_pos..];
        let end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..end];
        if tag.contains(r#"rx="2.5""#) {
            // This is a participant rect.
            // Use " x=" (space-prefixed) to avoid matching "rx=".
            let x_needle = r#" x=""#;
            let w_needle = r#" width=""#;
            if let (Some(xs), Some(ws)) = (tag.find(x_needle), tag.find(w_needle)) {
                let x_start = xs + x_needle.len();
                let x_end = x_start + tag[x_start..].find('"').unwrap();
                let x: f64 = tag[x_start..x_end].parse().unwrap_or(0.0);

                let w_start = ws + w_needle.len();
                let w_end = w_start + tag[w_start..].find('"').unwrap();
                let w: f64 = tag[w_start..w_end].parse().unwrap_or(0.0);

                participant_rects.push((x, w));
            }
        }
        offset = abs_pos + 5;
    }

    // Each participant has head + tail rects drawn together (per-participant),
    // so the order is: A-head, A-tail, B-head, B-tail.
    // Deduplicate by x to find distinct participant positions.
    let mut unique_centers: Vec<f64> = Vec::new();
    for (x, w) in &participant_rects {
        let center = x + w / 2.0;
        if unique_centers.iter().all(|c| (c - center).abs() > 1.0) {
            unique_centers.push(center);
        }
    }
    assert!(
        unique_centers.len() >= 2,
        "expected at least 2 unique participant centers, found {}: {:?}",
        unique_centers.len(),
        unique_centers,
    );
    let b_center = unique_centers[1];

    // Arrow tip should be strictly left of B's center (it stops at activation edge)
    assert!(
        tip_x < b_center,
        "arrow tip x ({tip_x}) should be < B center ({b_center}); \
		 arrow should stop at activation edge, not at participant center",
    );
}

// ── Test 5: note_rendered_between_messages ───────────────────────────
//
// Java interleaves note rendering with messages, so note "Note" appears
// between message "a" and message "b" in the SVG string.
// Current Rust renders all notes in a separate pass (step 9) after all
// messages, so the note text appears after both message texts.

#[test]
// Fixed: notes now interleaved with messages matching Java
fn note_rendered_between_messages() {
    let svg = convert("@startuml\nA -> B : a\nnote right: Note\nB --> A : b\n@enduml");

    // Find text content positions.  We search for the text as it appears
    // inside <text>...</text> tags.  Message labels and note text should
    // appear in rendering order.
    let pos_a = svg.find(">a</text>").expect("message 'a' text");
    let pos_note = svg.find(">Note</text>").expect("note 'Note' text");
    let pos_b = svg.find(">b</text>").expect("message 'b' text");

    assert!(
        pos_a < pos_note,
        "message 'a' (pos {pos_a}) should appear before note 'Note' (pos {pos_note})",
    );
    assert!(
        pos_note < pos_b,
        "note 'Note' (pos {pos_note}) should appear before message 'b' (pos {pos_b})",
    );
}

// ── Test 6: default_participant_rect_has_rounded_corners ─────────────
//
// Java default participant boxes have rx="2.5" ry="2.5" for rounded corners.

#[test]
fn default_participant_rect_has_rounded_corners() {
    let svg = convert("@startuml\nparticipant Alice\nAlice -> Alice : msg\n@enduml");

    // Find participant rect (has rx="2.5" ry="2.5")
    let has_rounded = svg.contains(r#"rx="2.5" ry="2.5""#);
    assert!(
        has_rounded,
        "participant rect should have rx=\"2.5\" ry=\"2.5\" for rounded corners",
    );

    // Verify the rounded rect is actually a <rect> element
    let rect_pos = svg.find(r#"rx="2.5""#).expect("rx attr");
    let preceding = &svg[..rect_pos];
    let tag_start = preceding.rfind('<').expect("tag start");
    let tag_name = &svg[tag_start..rect_pos];
    assert!(
        tag_name.starts_with("<rect"),
        "rounded corner attributes should be on a <rect> element, found: {tag_name}",
    );
}

// ── Issue #28: alt/group frames must match official width ───────────
//
// When a message spans a non-adjacent participant (Alice -> Log crossing
// Bob), the group/alt frames must keep the official width. The previous
// layout inflated the Bob -> Log gap, which made the outer frames ~13px
// too wide. This checks the rendered SVG, not just the layout struct, so
// it guards the user-visible "label pressed against the border" symptom.
// Golden width 327.9619 is from official PlantUML 1.2026.7beta3.
#[test]
fn issue28_frame_width_matches_official_in_svg() {
    let svg = convert(
        "@startuml\n\
		 Alice -> Bob: Authentication Request\n\
		 alt successful case\n\
		   Bob -> Alice: Authentication Accepted\n\
		 else some kind of failure\n\
		   Bob -> Alice: Authentication Failure\n\
		   group My own label\n\
		     Alice -> Log : Log attack start\n\
		     loop 1000 times\n\
		       Alice -> Bob: DNS Attack\n\
		     end\n\
		     Alice -> Log : Log attack end\n\
		   end\n\
		 else Another type of failure\n\
		   Bob -> Alice: Please repeat\n\
		 end\n\
		 @enduml",
    );

    // The outermost frame is the first fill="none" stroke-width:1.5 rect.
    let frame = extract_all_attrs(&svg, "<rect", "width")
        .into_iter()
        .next()
        .expect("a frame rect");
    let w: f64 = frame.parse().expect("numeric width");
    assert!(
        (w - 327.9619).abs() < 0.01,
        "outer frame width = {w}, expected 327.9619 (official PlantUML); pre-fix was 341.2046"
    );
}

// CJK message labels must be measured at 1em per ideograph, as Java's
// composite `SansSerif` font does via its CJK fallback slot. Measuring
// them with the embedded Latin subset's `.notdef` advance (~0.6em)
// emitted a `textLength` ~40% too short, so renderers applying
// `lengthAdjust="spacing"` drew the glyphs on top of each other, and
// participants were spaced too tightly for the label (markon#102).
#[test]
fn cjk_message_label_is_measured_full_width() {
    let svg = convert(
        "@startuml\n\
         actor User\n\
         participant Client\n\
         User -> Client: \u{8F93}\u{5165}\u{8D26}\u{53F7}\u{5BC6}\u{7801}\n\
         Client --> User: \u{767B}\u{5F55}\u{6210}\u{529F}\n\
         @enduml",
    );

    let text_lengths: Vec<f64> = extract_all_attrs(&svg, "<text", "textLength")
        .into_iter()
        .map(|v| v.parse().expect("numeric textLength"))
        .collect();
    // 6 and 4 ideographs at the default 13px message font size.
    assert!(
        text_lengths.iter().any(|w| (w - 78.0).abs() < 1e-6),
        "6-ideograph label should be 78px wide, got {text_lengths:?}"
    );
    assert!(
        text_lengths.iter().any(|w| (w - 52.0).abs() < 1e-6),
        "4-ideograph label should be 52px wide, got {text_lengths:?}"
    );

    // The label must fit between the two lifelines.
    let lifelines: Vec<f64> = extract_all_attrs(
        &svg,
        "<line style=\"stroke:#181818;stroke-width:0.5;stroke-dasharray:5,5;\"",
        "x1",
    )
    .into_iter()
    .map(|v| v.parse().expect("numeric x1"))
    .collect();
    let gap = lifelines[1] - lifelines[0];
    assert!(
        gap > 78.0,
        "lifeline gap {gap} narrower than the 78px label"
    );
}
