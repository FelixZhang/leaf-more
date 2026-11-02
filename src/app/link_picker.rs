use super::App;
use crate::markdown::{display_width, iter_cluster_widths, LinkSpan};
use crate::theme::app_theme;
use ratatui::text::Line;

pub(crate) const MSG_NO_LINKS: &str = "No links in this document";

/// What activating a link should do: open a local markdown file in leaf, or
/// hand a URL to the system opener.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LinkAction {
    Load(std::path::PathBuf),
    Open(String),
}

fn is_external_url(url: &str) -> bool {
    url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("mailto:")
        || url.contains("://")
}

fn has_markdown_ext(path: &std::path::Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("md") | Some("markdown") | Some("mdown") | Some("mkd")
    )
}

#[derive(Default)]
pub(crate) struct LinkPickerState {
    pub(crate) open: bool,
    pub(crate) entries: Vec<LinkEntry>,
    pub(crate) filtered: Vec<usize>,
    pub(crate) match_positions: Vec<Vec<usize>>,
    pub(crate) matched_in_url: Vec<bool>,
    pub(crate) query: String,
    pub(crate) index: usize,
    pub(crate) error: Option<&'static str>,
}

pub(crate) struct LinkEntry {
    pub(crate) text: String,
    pub(crate) url: String,
    pub(crate) text_lower: String,
    pub(crate) url_lower: String,
}

/// Recover the visible label of a link from its rendered line, skipping the
/// link icon marker spans (`fg == link_icon`) that `LinkSpan` columns include.
fn link_label_text(line: &Line<'_>, span: &LinkSpan, icon: ratatui::style::Color) -> String {
    let mut out = String::new();
    let mut col = 0usize;
    for part in &line.spans {
        let part_width = display_width(part.content.as_ref());
        let part_end = col + part_width;
        if part.style.fg != Some(icon) && part_end > span.start_col && col < span.end_col {
            let lo = col.max(span.start_col);
            let hi = part_end.min(span.end_col);
            let mut offset = 0usize;
            for (cluster, w) in iter_cluster_widths(part.content.as_ref()) {
                let cluster_start = col + offset;
                offset += w;
                if cluster_start + w <= lo {
                    continue;
                }
                if cluster_start >= hi {
                    break;
                }
                out.push_str(cluster);
            }
        }
        col = part_end;
    }
    out.trim().to_string()
}

impl App {
    pub(crate) fn open_link_picker(&mut self) {
        let icon = app_theme().markdown.link_icon;
        let mut entries = Vec::new();
        for (line_idx, line) in self.lines.iter().enumerate() {
            let Some(spans) = self.link_spans_by_line.get(&line_idx) else {
                continue;
            };
            for span in spans {
                let text = link_label_text(line, span, icon);
                entries.push(LinkEntry {
                    text_lower: text.to_lowercase(),
                    url_lower: span.url.to_lowercase(),
                    text,
                    url: span.url.clone(),
                });
            }
        }
        let error = if entries.is_empty() {
            Some(MSG_NO_LINKS)
        } else {
            None
        };
        self.link_picker = LinkPickerState {
            open: true,
            error,
            entries,
            ..LinkPickerState::default()
        };
        self.refresh_link_picker_matches();
    }

    pub(crate) fn is_link_picker_open(&self) -> bool {
        self.link_picker.open
    }

    pub(crate) fn close_link_picker(&mut self) {
        self.link_picker = LinkPickerState::default();
    }

    pub(crate) fn link_picker_entries(&self) -> &[LinkEntry] {
        &self.link_picker.entries
    }

    pub(crate) fn link_picker_filtered_indices(&self) -> &[usize] {
        &self.link_picker.filtered
    }

    pub(crate) fn link_picker_match_positions(&self, filtered_idx: usize) -> &[usize] {
        self.link_picker
            .match_positions
            .get(filtered_idx)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(crate) fn link_picker_matched_in_url(&self, filtered_idx: usize) -> bool {
        self.link_picker
            .matched_in_url
            .get(filtered_idx)
            .copied()
            .unwrap_or(false)
    }

    pub(crate) fn link_picker_index(&self) -> usize {
        self.link_picker.index
    }

    pub(crate) fn link_picker_query(&self) -> &str {
        &self.link_picker.query
    }

    pub(crate) fn link_picker_error(&self) -> Option<&'static str> {
        self.link_picker.error
    }

    pub(crate) fn move_link_picker_up(&mut self) {
        let total = self.link_picker.filtered.len();
        if total == 0 {
            return;
        }
        if self.link_picker.index == 0 {
            self.link_picker.index = total - 1;
        } else {
            self.link_picker.index -= 1;
        }
    }

    pub(crate) fn move_link_picker_down(&mut self) {
        let total = self.link_picker.filtered.len();
        if total == 0 {
            return;
        }
        self.link_picker.index = (self.link_picker.index + 1) % total;
    }

    pub(crate) fn move_link_picker_page_up(&mut self) {
        if self.link_picker.filtered.is_empty() {
            return;
        }
        self.link_picker.index = self
            .link_picker
            .index
            .saturating_sub(super::file_picker::PICKER_PAGE_SIZE);
    }

    pub(crate) fn move_link_picker_page_down(&mut self) {
        let total = self.link_picker.filtered.len();
        if total == 0 {
            return;
        }
        self.link_picker.index =
            (self.link_picker.index + super::file_picker::PICKER_PAGE_SIZE).min(total - 1);
    }

    pub(crate) fn push_link_picker_query(&mut self, ch: char) {
        self.link_picker.query.push(ch);
        self.refresh_link_picker_matches();
    }

    pub(crate) fn pop_link_picker_query(&mut self) {
        self.link_picker.query.pop();
        self.refresh_link_picker_matches();
    }

    pub(crate) fn clear_link_picker_query(&mut self) {
        self.link_picker.query.clear();
        self.refresh_link_picker_matches();
    }

    pub(crate) fn refresh_link_picker_matches(&mut self) {
        let query = self.link_picker.query.trim().to_lowercase();
        let total = self.link_picker.entries.len();

        if query.is_empty() {
            self.link_picker.filtered = (0..total).collect();
            self.link_picker.match_positions = vec![Vec::new(); total];
            self.link_picker.matched_in_url = vec![false; total];
        } else {
            let mut filtered = Vec::new();
            let mut match_positions = Vec::new();
            let mut matched_in_url = Vec::new();
            for (idx, entry) in self.link_picker.entries.iter().enumerate() {
                if let Some((_, positions)) =
                    super::fuzzy::fuzzy_component_match(&entry.text_lower, &query)
                {
                    filtered.push(idx);
                    match_positions.push(positions);
                    matched_in_url.push(false);
                } else if let Some((_, positions)) =
                    super::fuzzy::fuzzy_component_match(&entry.url_lower, &query)
                {
                    filtered.push(idx);
                    match_positions.push(positions);
                    matched_in_url.push(true);
                }
            }
            self.link_picker.filtered = filtered;
            self.link_picker.match_positions = match_positions;
            self.link_picker.matched_in_url = matched_in_url;
        }

        if self.link_picker.filtered.is_empty()
            || self.link_picker.index >= self.link_picker.filtered.len()
        {
            self.link_picker.index = 0;
        }
    }

    /// Activate the selected link. Internal `#anchor` links scroll the view
    /// to the matching heading (GitHub-style slug) and return `None`. Links
    /// resolving to a local markdown file (relative to the document) are
    /// returned as `LinkAction::Load` so the caller can open them in leaf;
    /// everything else is handed to the system opener.
    pub(crate) fn activate_link_picker_selection(&mut self) -> Option<LinkAction> {
        let &idx = self.link_picker.filtered.get(self.link_picker.index)?;
        let url = self.link_picker.entries[idx].url.clone();
        self.close_link_picker();
        if let Some(anchor) = url.strip_prefix('#') {
            if let Some(line) = self.resolve_anchor_line(anchor) {
                self.scroll = line.min(self.max_scroll());
            }
            return None;
        }
        if is_external_url(&url) {
            return Some(LinkAction::Open(url));
        }
        if let Some(dir) = self.filepath().and_then(|p| p.parent()) {
            if let Ok(path) = std::fs::canonicalize(dir.join(&url)) {
                if path.is_file() && has_markdown_ext(&path) {
                    return Some(LinkAction::Load(path));
                }
            }
        }
        self.set_link_flash(crate::app::LinkFlash::TargetNotFound(url.clone()));
        Some(LinkAction::Open(url))
    }

    fn resolve_anchor_line(&self, anchor: &str) -> Option<usize> {
        let target = slugify(anchor);
        if target.is_empty() {
            return None;
        }
        let exact = self.toc.iter().find(|e| slugify(&e.title) == target);
        let loose = || {
            self.toc.iter().find(|e| {
                let slug = slugify(&e.title);
                !slug.is_empty() && (slug.contains(&target) || target.contains(&slug))
            })
        };
        exact.or_else(loose).map(|e| e.line)
    }
}

/// GitHub-style heading slug: lowercase, drop punctuation, spaces become `-`.
fn slugify(text: &str) -> String {
    let mut slug = String::new();
    for ch in text.trim().chars() {
        if ch.is_whitespace() {
            slug.push('-');
        } else if ch.is_alphanumeric() || ch == '-' || ch == '_' {
            slug.extend(ch.to_lowercase());
        }
    }
    slug
}
