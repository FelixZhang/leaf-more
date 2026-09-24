use super::{rendered_non_empty_lines, test_assets, test_md_theme};
use crate::markdown::{line_plain_text, parse_markdown, parse_markdown_with_width};

#[test]
fn image_becomes_fixed_height_placeholder_without_duplicate_alt() {
    let (ss, theme) = test_assets();
    let src = "Before\n\n![diagram alt](images/diagram.svg \"diagram title\")\n\nAfter\n";
    let parsed = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true);
    let block = parsed.image_blocks.first().expect("image block");
    assert_eq!(block.id, 0);
    assert_eq!(block.source, "images/diagram.svg");
    assert_eq!(block.alt, "diagram alt");
    assert_eq!(block.title, "diagram title");
    assert_eq!(block.source_line, 3);
    assert_eq!(block.rendered_end - block.rendered_start + 1, 12);
    assert_eq!(block.prefix_width, 0);
    assert!(block.renderable);
    assert_eq!(block.rendered_width, 80);
    assert!(parsed.source_line_map[block.rendered_start] >= 3);

    let rendered: String = parsed.lines.iter().map(line_plain_text).collect();
    assert_eq!(rendered.matches("diagram alt").count(), 1);
    assert!(rendered.contains("source: images/diagram.svg"));
    assert!(rendered.contains("After"));
}

#[test]
fn image_placeholder_preserves_list_and_blockquote_prefixes() {
    let (ss, theme) = test_assets();
    let src = "- ![list image](list.png)\n\n> ![quote image](quote.png)\n";
    let parsed = parse_markdown_with_width(src, &ss, &theme, 50, &test_md_theme(), false, true);
    assert_eq!(parsed.image_blocks.len(), 2);
    assert!(parsed
        .image_blocks
        .iter()
        .all(|block| block.prefix_width > 0));
    let rendered = rendered_non_empty_lines(&parsed.lines);
    assert!(rendered.iter().any(|line| line.contains("list image")));
    assert!(rendered
        .iter()
        .any(|line| line.starts_with("▏ ") && line.contains("quote image")));
}

#[test]
fn image_in_heading_is_rendered_as_placeholder() {
    let (ss, theme) = test_assets();
    let parsed = parse_markdown(
        "## ![heading image](heading.png)\n",
        &ss,
        &theme,
        &test_md_theme(),
        false,
        true,
    );
    assert_eq!(parsed.image_blocks.len(), 1);
    let rendered = rendered_non_empty_lines(&parsed.lines);
    assert!(rendered.iter().any(|line| line.contains("heading image")));
    assert!(parsed.toc.iter().any(|entry| entry.title.is_empty()));
}

#[test]
fn table_image_falls_back_to_alt_text() {
    let (ss, theme) = test_assets();
    let src = "| Image |\n| --- |\n| ![table alt](table.png) |\n";
    let parsed = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true);
    assert!(parsed.image_blocks.is_empty());
    let rendered = rendered_non_empty_lines(&parsed.lines);
    assert!(rendered.iter().any(|line| line.contains("table alt")));
    assert!(!rendered.iter().any(|line| line.contains("source:")));
}

#[test]
fn footnote_image_state_does_not_shift_following_source_lines() {
    let (ss, theme) = test_assets();
    let src = "Body[^n]\n\n[^n]: ![note image](note.png)\n\nAfter\n";
    let parsed = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true);
    assert_eq!(parsed.image_blocks.len(), 1);
    assert_eq!(parsed.image_blocks[0].source, "note.png");
    let block = &parsed.image_blocks[0];
    assert!(!block.renderable);
    assert!(block.rendered_start < parsed.lines.len());
    assert!(block.rendered_end < parsed.lines.len());
    assert!(block.rendered_end - block.rendered_start + 1 >= 12);
    let rendered = rendered_non_empty_lines(&parsed.lines);
    let after_idx = rendered
        .iter()
        .position(|line| line.contains("After"))
        .expect("After line");
    assert!(parsed.source_line_map.contains(&5));
    assert!(after_idx > 0);
}

#[test]
fn image_alt_collects_nested_images_and_inline_formatting() {
    let (ss, theme) = test_assets();
    let src = "![outer *em* `code` $x$ [link](https://example.com) <b>bold</b><br><span>raw</span> ![inner](inner.png)](outer.png)\n";
    let parsed = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true);
    assert_eq!(parsed.image_blocks.len(), 1);
    let block = &parsed.image_blocks[0];
    assert_eq!(block.source, "outer.png");
    assert!(block.alt.contains("outer"));
    assert!(block.alt.contains("em"));
    assert!(block.alt.contains("code"));
    assert!(block.alt.contains("x"));
    assert!(block.alt.contains("link"));
    assert!(block.alt.contains("bold"));
    assert!(block.alt.contains("<span>raw</span>"));
    assert!(!block.alt.contains("<b>"));
    assert!(!block.alt.contains("<br>"));
    assert!(block.alt.contains("inner"));
    let rendered: String = parsed.lines.iter().map(line_plain_text).collect();
    assert!(!rendered.contains("inner.png"));
    assert!(!rendered.contains('#'));
}

#[test]
fn footnote_and_body_images_have_unique_ids() {
    let (ss, theme) = test_assets();
    let src = "Body[^n]\n\n[^n]: ![note](note.png)\n\n![body](body.png)\n";
    let parsed = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true);
    assert_eq!(parsed.image_blocks.len(), 2);
    let ids: std::collections::HashSet<_> =
        parsed.image_blocks.iter().map(|block| block.id).collect();
    assert_eq!(ids.len(), 2);
}

#[test]
fn multiline_image_maps_all_placeholder_rows_to_start_line() {
    let (ss, theme) = test_assets();
    let src = "Before\n\n![multi\nline](image.png)\n";
    let parsed = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true);
    let block = &parsed.image_blocks[0];
    assert_eq!(block.source_line, 3);
    let line_numbers: std::collections::HashSet<_> = parsed.line_number_map
        [block.rendered_start..=block.rendered_end]
        .iter()
        .copied()
        .collect();
    assert_eq!(line_numbers.len(), 1);
    assert!(
        parsed.source_line_map[block.rendered_start..=block.rendered_end]
            .iter()
            .all(|line| *line == 3)
    );
}

#[test]
fn table_image_alt_collects_inline_and_nested_content() {
    let (ss, theme) = test_assets();
    let src =
        "| Image |\n| --- |\n| ![outer `code` <b>bold</b> ![inner](inner.png)](outer.png) |\n";
    let parsed = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true);
    let rendered = rendered_non_empty_lines(&parsed.lines);
    assert!(rendered
        .iter()
        .any(|line| line.contains("outer code bold inner")));
    assert!(!rendered.iter().any(|line| line.contains("<b>")));
    assert!(!rendered.iter().any(|line| line.contains("inner.png")));
}
