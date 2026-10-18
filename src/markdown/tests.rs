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
