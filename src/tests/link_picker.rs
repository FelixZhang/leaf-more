use super::{test_assets, test_md_theme};
use crate::app::{App, AppConfig};
use crate::markdown::parse_markdown;

fn app_with_markdown(src: &str) -> App {
    let (ss, theme) = test_assets();
    let crate::markdown::ParseResult {
        lines,
        toc,
        link_spans,
        ..
    } = parse_markdown(src, &ss, &theme, &test_md_theme(), false, true);
    let mut app = App::new_with_source(
        lines,
        toc,
        AppConfig {
            filename: "doc".to_string(),
            source: src.to_string(),
            debug_input: false,
            watch: false,
            filepath: None,
            last_file_state: None,
        },
    );
    app.set_link_spans(link_spans);
    app
}

fn labels(app: &App) -> Vec<String> {
    app.link_picker_filtered_indices()
        .iter()
        .map(|idx| app.link_picker_entries()[*idx].text.clone())
        .collect()
}

#[test]
fn link_picker_lists_links_in_document_order() {
    let mut app = app_with_markdown(
        "intro [first](https://example.com/one) mid [second](https://example.com/two)\n\n\
         [docs/guide](docs/guide.md) and [anchor](#section-2)\n",
    );
    assert!(!app.is_link_picker_open());

    app.open_link_picker();
    assert!(app.is_link_picker_open());
    assert_eq!(app.link_picker_error(), None);
    assert_eq!(app.link_picker_entries().len(), 4);
    assert_eq!(
        labels(&app),
        vec!["first", "second", "docs/guide", "anchor"]
    );
    assert_eq!(app.link_picker_entries()[0].url, "https://example.com/one");
    assert_eq!(app.link_picker_entries()[3].url, "#section-2");

    // navigation wraps
    app.move_link_picker_down();
    app.move_link_picker_down();
    app.move_link_picker_up();
    assert_eq!(app.link_picker_index(), 1);
}

#[test]
fn link_picker_filters_by_text_or_url() {
    let mut app =
        app_with_markdown("[Rivo](https://rivolink.mg) plain [Guide](https://example.com/guide)\n");
    app.open_link_picker();

    // by label text
    for ch in "guide".chars() {
        app.push_link_picker_query(ch);
    }
    assert_eq!(labels(&app), vec!["Guide"]);
    assert!(!app.link_picker_matched_in_url(0));
    assert!(!app.link_picker_match_positions(0).is_empty());

    app.clear_link_picker_query();
    assert_eq!(labels(&app).len(), 2);

    // by url
    app.clear_link_picker_query();
    for ch in "rivolink".chars() {
        app.push_link_picker_query(ch);
    }
    assert_eq!(labels(&app), vec!["Rivo"]);
    assert!(app.link_picker_matched_in_url(0));

    // backspace pops the query
    for _ in 0.."rivolink".len() {
        app.pop_link_picker_query();
    }
    assert_eq!(labels(&app).len(), 2);
}

#[test]
fn link_picker_page_moves_jump_and_clamp() {
    let mut src = String::new();
    for idx in 0..15 {
        src.push_str(&format!("[link{idx:02}](https://example.com/{idx})\n\n"));
    }
    let mut app = app_with_markdown(&src);
    app.open_link_picker();
    assert_eq!(app.link_picker_filtered_indices().len(), 15);

    app.move_link_picker_page_down();
    assert_eq!(app.link_picker_index(), 10);
    app.move_link_picker_page_down();
    assert_eq!(app.link_picker_index(), 14);
    app.move_link_picker_page_up();
    assert_eq!(app.link_picker_index(), 4);
    app.move_link_picker_page_up();
    assert_eq!(app.link_picker_index(), 0);
    app.move_link_picker_page_up();
    assert_eq!(app.link_picker_index(), 0);
}

#[test]
fn link_picker_activate_opens_url_and_closes() {
    let mut app = app_with_markdown("[only](https://example.com/x)\n");
    app.open_link_picker();
    assert_eq!(
        app.activate_link_picker_selection(),
        Some(crate::app::link_picker::LinkAction::Open(
            "https://example.com/x".to_string()
        ))
    );
    assert!(!app.is_link_picker_open());
}

#[test]
fn link_picker_local_markdown_links_load_in_leaf() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("leaf-link-local-{unique}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sub")).unwrap();
    let doc = root.join("doc.md");
    std::fs::write(
        &doc,
        "# Doc\n\n[next](sub/next.md) [broken](gone.md) [note](notes.txt)\n",
    )
    .unwrap();
    std::fs::write(root.join("sub/next.md"), "# Next\n").unwrap();
    std::fs::write(root.join("notes.txt"), "not markdown\n").unwrap();

    let (ss, theme) = test_assets();
    let src = std::fs::read_to_string(&doc).unwrap();
    let crate::markdown::ParseResult {
        lines,
        toc,
        link_spans,
        ..
    } = parse_markdown(src.as_str(), &ss, &theme, &test_md_theme(), false, true);
    let mut app = App::new_with_source(
        lines,
        toc,
        AppConfig {
            filename: "doc.md".to_string(),
            source: src,
            debug_input: false,
            watch: false,
            filepath: Some(doc),
            last_file_state: None,
        },
    );
    app.set_link_spans(link_spans);
    app.open_link_picker();
    assert_eq!(labels(&app), vec!["next", "broken", "note"]);

    // existing local markdown: loaded in leaf
    assert_eq!(
        app.activate_link_picker_selection(),
        Some(crate::app::link_picker::LinkAction::Load(
            root.join("sub/next.md").canonicalize().unwrap()
        ))
    );
    app.open_link_picker();

    // broken local link falls back to the system opener
    app.move_link_picker_down();
    assert_eq!(
        app.activate_link_picker_selection(),
        Some(crate::app::link_picker::LinkAction::Open(
            "gone.md".to_string()
        ))
    );
    app.open_link_picker();

    // existing non-markdown file still goes to the system opener
    app.move_link_picker_down();
    app.move_link_picker_down();
    assert_eq!(
        app.activate_link_picker_selection(),
        Some(crate::app::link_picker::LinkAction::Open(
            "notes.txt".to_string()
        ))
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn link_picker_reports_empty_documents() {
    let mut app = app_with_markdown("just text, no links\n");
    app.open_link_picker();
    assert_eq!(
        app.link_picker_error(),
        Some(crate::app::link_picker::MSG_NO_LINKS)
    );
    assert!(app.link_picker_filtered_indices().is_empty());
    assert_eq!(app.activate_link_picker_selection(), None);

    // filtering an empty list stays safe
    app.push_link_picker_query('x');
    assert!(app.link_picker_filtered_indices().is_empty());
    app.close_link_picker();
    assert!(!app.is_link_picker_open());
}

#[test]
fn link_picker_internal_anchor_jumps_to_heading() {
    let mut app = app_with_markdown(
        "# Top\n\ntext\n\n## Section 2\n\nbody\n\n[Jump](#section-2)\n\n[Missing](#nope)\n",
    );
    app.open_link_picker();
    assert_eq!(labels(&app), vec!["Jump", "Missing"]);
    assert_eq!(app.link_picker_entries()[0].url, "#section-2");

    // internal link: no URL returned, the view scrolls to the heading
    assert_eq!(app.activate_link_picker_selection(), None);
    assert!(!app.is_link_picker_open());
    let section2_line = app
        .toc
        .iter()
        .find(|e| e.title == "Section 2")
        .expect("heading in toc")
        .line;
    assert_eq!(app.scroll, section2_line.min(app.max_scroll()));

    // unresolved anchor: closes without opening anything
    app.open_link_picker();
    for ch in "nope".chars() {
        app.push_link_picker_query(ch);
    }
    assert_eq!(labels(&app), vec!["Missing"]);
    assert_eq!(app.activate_link_picker_selection(), None);
    assert!(!app.is_link_picker_open());
    assert_eq!(app.scroll, section2_line.min(app.max_scroll()));

    // external links still return their URL
    app.open_link_picker();
    app.clear_link_picker_query();
    for ch in "jump".chars() {
        app.push_link_picker_query(ch);
    }
    // "Jump" matches by text; activate scrolls, returns None
    assert_eq!(app.activate_link_picker_selection(), None);

    let mut app2 = app_with_markdown("[ext](https://example.com/x)\n\n## Head\n");
    app2.open_link_picker();
    assert_eq!(
        app2.activate_link_picker_selection(),
        Some(crate::app::link_picker::LinkAction::Open(
            "https://example.com/x".to_string()
        ))
    );
}
