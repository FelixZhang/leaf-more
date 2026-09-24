use std::path::Path;

use image::ImageReader;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use crate::theme::MarkdownTheme;

use super::blocks::{block_prefix, BlockLayout};
use super::lists::{list_item_prefix, ItemState, ListKind};
use super::width::{display_width, truncate_display_width};

const MIN_IMAGE_BLOCK_HEIGHT: usize = 12;
const MAX_IMAGE_BLOCK_HEIGHT: usize = 40;
const DEFAULT_CELL_HEIGHT_RATIO: u64 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImageBlockInfo {
    pub(crate) id: usize,
    pub(crate) source: String,
    pub(crate) alt: String,
    pub(crate) title: String,
    pub(crate) source_line: usize,
    pub(crate) rendered_start: usize,
    pub(crate) rendered_end: usize,
    pub(crate) rendered_width: usize,
    pub(crate) prefix_width: usize,
    pub(crate) renderable: bool,
}

pub(super) struct ImageRenderContext<'a> {
    pub(super) render_width: usize,
    pub(super) blockquote_depth: usize,
    pub(super) list_stack: &'a [ListKind],
    pub(super) blockquote_color: Option<Color>,
    pub(super) theme: &'a MarkdownTheme,
    pub(super) image_base_dir: Option<&'a Path>,
}

pub(super) fn push_image_placeholder(
    lines: &mut Vec<Line<'static>>,
    item_stack: &mut [ItemState],
    alt: &str,
    source: &str,
    context: ImageRenderContext<'_>,
) -> BlockLayout {
    let ImageRenderContext {
        render_width,
        blockquote_depth,
        list_stack,
        blockquote_color,
        theme,
        image_base_dir,
    } = context;
    let prefix = if !item_stack.is_empty() {
        list_item_prefix(
            blockquote_depth,
            list_stack,
            item_stack,
            theme,
            blockquote_color,
        )
    } else if blockquote_depth > 0 {
        block_prefix(blockquote_depth, theme, blockquote_color)
    } else {
        Vec::new()
    };
    let prefix_width: usize = prefix
        .iter()
        .map(|span| display_width(span.content.as_ref()))
        .sum();
    let frame_width = render_width.saturating_sub(prefix_width).max(1);
    let inner_width = frame_width.saturating_sub(2).max(1);
    let block_height = image_block_height(source, image_base_dir, inner_width);
    let frame_style = Style::default().fg(theme.code_frame);
    let label_style = Style::default().fg(theme.code_label);

    let header_prefix = "┌─ image ";
    let header = format!(
        "{header_prefix}{}┐",
        "─".repeat(inner_width.saturating_sub(display_width(header_prefix)))
    );
    let header = if display_width(&header) <= frame_width {
        header
    } else {
        format!("┌{}┐", "─".repeat(inner_width))
    };
    let alt_line = if alt.trim().is_empty() {
        "alt".to_string()
    } else {
        format!("alt: {}", alt.trim())
    };
    let source_line = if source.is_empty() {
        "source: (empty)".to_string()
    } else {
        format!("source: {source}")
    };
    let row_text = [alt_line, source_line];

    for row in 0..block_height {
        let text = if row == 0 {
            header.clone()
        } else if row == block_height - 1 {
            format!("└{}┘", "─".repeat(inner_width))
        } else if row <= row_text.len() {
            let content_width = inner_width.saturating_sub(2);
            let content = truncate_display_width(&row_text[row - 1], content_width);
            let padding = content_width.saturating_sub(display_width(&content));
            format!("│ {content}{}│", " ".repeat(padding))
        } else {
            let padding = inner_width.saturating_sub(2);
            format!("│{}│", " ".repeat(padding))
        };
        let mut spans = prefix.clone();
        spans.push(Span::styled(text, frame_style));
        if row == 1 {
            if let Some(label) = spans.last_mut() {
                label.style = label_style;
            }
        }
        lines.push(Line::from(spans));
    }

    BlockLayout {
        prefix_width,
        rendered_width: frame_width,
    }
}

pub(super) fn image_block_height(
    source: &str,
    base_dir: Option<&Path>,
    inner_width: usize,
) -> usize {
    image_dimensions(source, base_dir)
        .and_then(|(pixel_width, pixel_height)| {
            let inner_width = u64::try_from(inner_width).ok()?;
            let pixel_width = u64::from(pixel_width);
            let pixel_height = u64::from(pixel_height);
            if pixel_width == 0 || pixel_height == 0 {
                return None;
            }
            let image_rows = inner_width
                .saturating_mul(pixel_height)
                .div_ceil(pixel_width.saturating_mul(DEFAULT_CELL_HEIGHT_RATIO));
            usize::try_from(image_rows.saturating_add(2)).ok()
        })
        .unwrap_or(MIN_IMAGE_BLOCK_HEIGHT)
        .clamp(MIN_IMAGE_BLOCK_HEIGHT, MAX_IMAGE_BLOCK_HEIGHT)
}

fn image_dimensions(source: &str, base_dir: Option<&Path>) -> Option<(u32, u32)> {
    let source = source.trim();
    if source.is_empty() || source.starts_with("data:") || source.contains("://") {
        return None;
    }
    let source_path = Path::new(source);
    let path = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        base_dir.unwrap_or_else(|| Path::new(".")).join(source_path)
    };
    let metadata = std::fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 20 * 1024 * 1024 {
        return None;
    }
    ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_image_height_uses_aspect_ratio_and_terminal_cell_ratio() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(
            image_block_height("images/preview.png", Some(manifest_dir), 78),
            23
        );
        assert_eq!(
            image_block_height("src/tests/fixtures/kitty-image.png", Some(manifest_dir), 78),
            40
        );
    }

    #[test]
    fn unavailable_or_narrow_images_use_minimum_height() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(
            image_block_height("images/missing.png", Some(manifest_dir), 78),
            12
        );
        assert_eq!(
            image_block_height("images/preview.png", Some(manifest_dir), 20),
            12
        );
    }
}
