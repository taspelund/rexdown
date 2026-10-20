use super::*;

#[test]
fn wraps_prose_and_links_without_losing_unicode_or_styles() {
    let renderer = Renderer::new();
    for columns in 2..80 {
        let doc = renderer.render(
            "**Café 界** [link é 界 words](https://example.com)",
            columns,
        );
        let visible: String = doc.rows.iter().map(text).collect();
        assert_eq!(visible.replace(' ', ""), "Café界linké界words");
        assert!(doc.rows.iter().all(|row| width(row) <= columns));
        let linked: String = doc
            .rows
            .iter()
            .flatten()
            .filter(|g| g.link.is_some())
            .map(|g| g.text.as_str())
            .collect();
        assert_eq!(linked.replace(' ', ""), "linké界words");
        assert!(doc.rows[0][0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(doc.links, ["https://example.com"]);
    }
}

#[test]
fn tables_fit_and_keep_alignment_and_links() {
    let renderer = Renderer::new();
    let source = "| Name | Count |\n| :--- | ---: |\n| abc | 12 |\n| [x](#abc) | 3 |\n";
    let doc = renderer.render(source, 18);
    assert!(doc.rows.iter().all(|r| width(r) <= 18));
    assert_eq!(text(&doc.rows[2]), "│ abc  │    12 │");
    assert_eq!(text(&doc.rows[3]), "│ x    │     3 │");
    assert_eq!(doc.links, ["#abc"]);
}

#[test]
fn fences_preserve_diagrams_and_highlight_code_without_terminal_controls() {
    let renderer = Renderer::new();
    let doc = renderer.render("```diagram\n┌──────────────┐\n│ hello world  │\n└──────────────┘\n```\n\n```rust\nlet x = 42;\n```\n\nESC: \u{1b}[31m", 8);
    assert_eq!(text(&doc.rows[0]), "│ ┌──────────────┐");
    assert_eq!(text(&doc.rows[1]), "│ │ hello world  │");
    let code = &doc.rows[4];
    assert_eq!(text(code), "│ let x = 42;");
    assert_ne!(code[2].style.fg, code[10].style.fg);
    assert!(
        !doc.rows
            .iter()
            .map(text)
            .collect::<String>()
            .contains('\u{1b}')
    );
}

#[test]
fn heading_offsets_follow_wrapping_and_duplicate_anchors_are_unique() {
    let renderer = Renderer::new();
    let doc = renderer.render(
        "# One two three\n\n- parent\n  - nested\n\n## One two three\n\n## Explicit {#chosen}\n",
        8,
    );
    assert_eq!(
        doc.headings
            .iter()
            .map(|h| h.anchor.as_str())
            .collect::<Vec<_>>(),
        ["one-two-three", "one-two-three-1", "chosen"]
    );
    for h in &doc.headings {
        assert!(text(&doc.rows[h.row]).starts_with(h.title.split_whitespace().next().unwrap()));
    }
    assert!(doc.rows.iter().any(|r| text(r).starts_with("  • nest")));
}

const NESTED: &str = "# Café 界 heading with several words\n\n> quote é 界 one two\n> - item 界 α β γ\n>   1. deep ñ text here\n>\n>   para in item é\n\n- parent é\n  - nested 界\n    deeper words\n- one\\\n  two\n- top\n  ```rust\n  let é = 1;\n  let x = 2;\n  ```\n- second **bold 界** [lïnk](#x)\n\n<div>\nraw é\n</div>\n\n---\n\n| H 界 | V |\n| --- | :-: |\n| a é b c | 1 |\n";

fn source_of<'a>(source: &'a str, doc: &Document, row: usize) -> &'a str {
    &source[doc.row_sources[row].clone()]
}

/// Contract every rendered document must satisfy, independent of content.
fn assert_provenance(source: &str, doc: &Document) {
    assert_eq!(doc.rows.len(), doc.row_sources.len());
    assert!(doc.rows.last().is_none_or(|r| !r.is_empty()));
    let mut seen: Vec<Range<usize>> = Vec::new();
    for (i, (row, range)) in doc.rows.iter().zip(&doc.row_sources).enumerate() {
        assert!(range.start < range.end, "row {i} has empty range");
        assert!(range.end <= source.len(), "row {i} out of bounds");
        assert!(source.is_char_boundary(range.start) && source.is_char_boundary(range.end));
        if row.is_empty() {
            assert_eq!(Some(range), doc.row_sources.get(i.wrapping_sub(1)));
        }
        if i > 0 {
            assert!(
                doc.row_sources[i - 1].start <= range.start,
                "row {i} goes backwards"
            );
        }
        // A block's rows are contiguous: a range never reappears later.
        if seen.last() != Some(range) {
            assert!(!seen.contains(range), "row {i} range is not contiguous");
            seen.push(range.clone());
        }
        // Every visible word fragment comes from the attributed source.
        for token in text(row).split_whitespace() {
            let token = token.trim_matches(|c| "│•─├┤┼".contains(c));
            assert!(
                token.is_empty() || source[range.clone()].contains(token),
                "row {i} token {token:?} not in {:?}",
                &source[range.clone()]
            );
        }
    }
}

#[test]
fn every_row_has_unicode_safe_provenance_for_nested_blocks_at_any_width() {
    let renderer = Renderer::new();
    let wide = renderer.render(NESTED, 200);
    let mut wide_ranges = wide.row_sources.clone();
    wide_ranges.dedup();
    for columns in 1..90 {
        let doc = renderer.render(NESTED, columns);
        assert_provenance(NESTED, &doc);
        let mut ranges = doc.row_sources.clone();
        ranges.dedup();
        assert_eq!(ranges, wide_ranges, "provenance changed at width {columns}");
        for h in &doc.headings {
            assert!(source_of(NESTED, &doc, h.row).starts_with("# Café"));
        }
    }
    assert_provenance("", &renderer.render("", 10));
    assert!(renderer.render("", 10).row_sources.is_empty());
}

#[test]
fn wrapped_paragraph_rows_share_one_block_and_blanks_follow_it() {
    let renderer = Renderer::new();
    let source = "Première é 界 paragraph with words\n\nSecond 界\n\n\n";
    let doc = renderer.render(source, 6);
    assert_provenance(source, &doc);
    let blank = doc.rows.iter().position(Vec::is_empty).unwrap();
    assert!(blank > 2);
    for row in 0..=blank {
        assert_eq!(
            source_of(source, &doc, row).trim_end(),
            "Première é 界 paragraph with words"
        );
    }
    for row in blank + 1..doc.rows.len() {
        assert_eq!(source_of(source, &doc, row).trim_end(), "Second 界");
    }
    assert_eq!(doc.rows.len(), blank + 3, "trailing blanks are trimmed");
}

#[test]
fn list_items_exclude_nested_blocks_and_code_maps_per_line() {
    let renderer = Renderer::new();
    let doc = renderer.render(NESTED, 80);
    let find = |prefix: &str| {
        doc.rows
            .iter()
            .position(|r| text(r).trim_start().starts_with(prefix))
            .unwrap_or_else(|| panic!("no row {prefix:?}"))
    };
    let parent = source_of(NESTED, &doc, find("• parent"));
    assert!(parent.starts_with("- parent é") && !parent.contains("nested"));
    let nested = source_of(NESTED, &doc, find("• nested"));
    assert!(nested.starts_with("- nested 界") && nested.contains("deeper words"));
    assert!(!nested.contains("- one"));
    // A hard break inside a tight item keeps one range for both rows.
    let one = find("• one");
    assert_eq!(doc.row_sources[one], doc.row_sources[one + 1]);
    assert!(source_of(NESTED, &doc, one).starts_with("- one\\"));
    // Loose item inside a quote: the paragraph after the nested list is its own block.
    let para = source_of(NESTED, &doc, find("│ para in item"));
    assert_eq!(para.trim_end(), "para in item é");
    let deep = source_of(NESTED, &doc, find("│ 1. deep"));
    assert!(deep.starts_with("1. deep ñ") && !deep.contains("para"));
    assert_eq!(
        source_of(NESTED, &doc, find("│ let é")).trim_end(),
        "let é = 1;"
    );
    assert_eq!(
        source_of(NESTED, &doc, find("│ let x")).trim_end(),
        "let x = 2;"
    );
    assert!(source_of(NESTED, &doc, find("<div>")).starts_with("<div>\nraw é\n</div>"));
    assert_eq!(source_of(NESTED, &doc, find("───")).trim_end(), "---");
    let head = find("│ H 界");
    assert_eq!(source_of(NESTED, &doc, head).trim_end(), "| H 界 | V |");
    assert_eq!(doc.row_sources[head], doc.row_sources[head + 1]);
    assert_eq!(
        source_of(NESTED, &doc, head + 2).trim_end(),
        "| a é b c | 1 |"
    );
}
