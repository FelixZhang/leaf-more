use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

use anyhow::Result as AnyResult;
use image::{ImageReader, Limits};
use ratatui::{layout::Rect, Frame};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    sliced::{SignedPosition, SlicedImage, SlicedProtocol},
    Resize,
};
use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};

#[cfg(test)]
use crate::app::AppConfig;
use crate::{
    app::{App, ImageFlash},
    markdown::ImageBlockInfo,
};

const MAX_IMAGE_FILE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 16_384;
const MAX_IMAGE_ALLOC_BYTES: u64 = 128 * 1024 * 1024;
const IMAGE_WORKER_POLL: Duration = Duration::from_millis(50);
static IMAGE_DECODE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ImageSize {
    width: u16,
    height: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ImageKey {
    path: PathBuf,
    modified: Option<SystemTime>,
    len: u64,
    size: ImageSize,
}

struct ImageJob {
    block_id: usize,
    key: ImageKey,
    size: ratatui::layout::Size,
    picker: Picker,
    path: PathBuf,
    attempts: u8,
    cancel: Arc<AtomicBool>,
}

struct ImageResult {
    block_id: usize,
    key: ImageKey,
    attempts: u8,
    result: Result<SlicedProtocol, String>,
}

enum ImageState {
    Loading,
    Ready(SlicedProtocol),
    Failed { attempts: u8, retry_at: Instant },
}

struct ImageEntry {
    key: ImageKey,
    state: ImageState,
}

pub(crate) struct ImageRuntime {
    picker: Option<Picker>,
    job_sender: Option<Sender<ImageJob>>,
    result_receiver: Option<Receiver<ImageResult>>,
    entries: HashMap<usize, ImageEntry>,
    document_dir: PathBuf,
    terminal_ready: bool,
    active: bool,
    cancel_worker: Option<Arc<AtomicBool>>,
}

impl ImageRuntime {
    pub(crate) fn new(document_path: Option<&Path>) -> Self {
        Self {
            picker: None,
            job_sender: None,
            result_receiver: None,
            entries: HashMap::new(),
            document_dir: document_dir(document_path),
            terminal_ready: false,
            active: false,
            cancel_worker: None,
        }
    }

    pub(crate) fn set_document_path(&mut self, document_path: Option<&Path>) {
        let next = document_dir(document_path);
        if self.document_dir != next {
            self.document_dir = next;
            self.entries.clear();
        }
    }

    pub(crate) fn enable_kitty(&mut self) -> bool {
        if self.active {
            return true;
        }
        if self.picker.is_none() {
            let mut picker = match Picker::from_query_stdio() {
                Ok(picker) => picker,
                Err(_) => return false,
            };
            picker.set_protocol_type(ProtocolType::Kitty);
            self.picker = Some(picker);
        }
        if self.picker.is_none() {
            return false;
        }
        let (job_sender, job_receiver) = mpsc::channel();
        let (result_sender, result_receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let worker = thread::Builder::new()
            .name("leaf-kitty-images".to_string())
            .spawn(move || {
                while let Ok(job) = job_receiver.recv() {
                    if worker_cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let result = decode_job_guarded(&job).map_err(|err| err.to_string());
                    if worker_cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    if result_sender
                        .send(ImageResult {
                            block_id: job.block_id,
                            key: job.key,
                            attempts: job.attempts,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
        if worker.is_err() {
            return false;
        }
        self.active = true;
        self.cancel_worker = Some(cancel);
        self.job_sender = Some(job_sender);
        self.result_receiver = Some(result_receiver);
        true
    }

    pub(crate) fn disable(&mut self) {
        self.active = false;
        if let Some(cancel) = self.cancel_worker.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.job_sender = None;
        self.result_receiver = None;
        self.entries.clear();
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    pub(crate) fn sync_blocks(&mut self, blocks: &[ImageBlockInfo]) {
        let block_ids: HashSet<usize> = blocks.iter().map(|block| block.id).collect();
        self.entries.retain(|id, _| block_ids.contains(id));
    }

    pub(crate) fn poll_results(&mut self) -> bool {
        if !self.active {
            return false;
        }
        let Some(receiver) = &self.result_receiver else {
            return false;
        };
        let mut changed = false;
        while let Ok(result) = receiver.try_recv() {
            let Some(entry) = self.entries.get_mut(&result.block_id) else {
                continue;
            };
            if entry.key != result.key {
                continue;
            }
            entry.state = match result.result {
                Ok(protocol) => ImageState::Ready(protocol),
                Err(_) => ImageState::Failed {
                    attempts: result.attempts,
                    retry_at: Instant::now() + retry_delay(result.attempts),
                },
            };
            changed = true;
        }
        let now = Instant::now();
        changed
            || self.entries.values().any(|entry| {
                matches!(
                    &entry.state,
                    ImageState::Failed { retry_at, .. } if *retry_at <= now
                )
            })
    }

    pub(crate) fn poll_delay(&self) -> Option<Duration> {
        if !self.active {
            return None;
        }
        let now = Instant::now();
        self.entries
            .values()
            .filter_map(|entry| match &entry.state {
                ImageState::Loading => Some(IMAGE_WORKER_POLL),
                ImageState::Failed { retry_at, .. } => {
                    Some(retry_at.saturating_duration_since(now))
                }
                ImageState::Ready(_) => None,
            })
            .min()
    }

    pub(crate) fn render(
        &mut self,
        frame: &mut Frame<'_>,
        blocks: &[ImageBlockInfo],
        content_area: Rect,
        scroll: usize,
        x_offset: usize,
    ) {
        if !self.active
            || self.picker.is_none()
            || content_area.width == 0
            || content_area.height == 0
        {
            return;
        }
        let visible_end = scroll.saturating_add(content_area.height as usize);
        for block in blocks {
            if !block.renderable {
                continue;
            }
            if block.rendered_end < scroll || block.rendered_start >= visible_end {
                continue;
            }
            let Some((key, path, size)) = self.resolve_key(block) else {
                continue;
            };
            self.ensure_loaded(block.id, key.clone(), path, size);
            let Some(ImageState::Ready(protocol)) = self
                .entries
                .get_mut(&block.id)
                .map(|entry| &mut entry.state)
            else {
                continue;
            };
            let y = block.rendered_start as isize + 1 - scroll as isize;
            let x = x_offset
                .saturating_add(block.prefix_width)
                .saturating_add(1)
                .min(i16::MAX as usize) as i16;
            if y < i16::MIN as isize || y > i16::MAX as isize {
                continue;
            }
            frame.render_widget(
                SlicedImage::new(protocol, SignedPosition { x, y: y as i16 }),
                content_area,
            );
        }
    }

    fn resolve_key(
        &self,
        block: &ImageBlockInfo,
    ) -> Option<(ImageKey, PathBuf, ratatui::layout::Size)> {
        let source = block.source.trim();
        if source.is_empty() || is_remote_source(source) || !has_supported_extension(source) {
            return None;
        }
        let source_path = Path::new(source);
        let joined = if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            self.document_dir.join(source_path)
        };
        let path = fs::canonicalize(joined).ok()?;
        let metadata = fs::metadata(&path).ok()?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_IMAGE_FILE_BYTES {
            return None;
        }
        let width = block
            .rendered_width
            .saturating_sub(2)
            .min(u16::MAX as usize) as u16;
        let block_height = block
            .rendered_end
            .saturating_sub(block.rendered_start)
            .saturating_add(1);
        let height = block_height.saturating_sub(2).min(u16::MAX as usize) as u16;
        if width == 0 || height == 0 {
            return None;
        }
        let size = ratatui::layout::Size { width, height };
        let key = ImageKey {
            path: path.clone(),
            modified: metadata.modified().ok(),
            len: metadata.len(),
            size: ImageSize { width, height },
        };
        Some((key, path, size))
    }

    fn ensure_loaded(
        &mut self,
        block_id: usize,
        key: ImageKey,
        path: PathBuf,
        size: ratatui::layout::Size,
    ) {
        if !self.active {
            return;
        }
        let mut attempts = 0;
        if let Some(entry) = self.entries.get(&block_id) {
            if entry.key == key {
                match &entry.state {
                    ImageState::Loading | ImageState::Ready(_) => return,
                    ImageState::Failed {
                        attempts: previous_attempts,
                        retry_at,
                    } => {
                        if *retry_at > Instant::now() {
                            return;
                        }
                        attempts = previous_attempts.saturating_add(1);
                    }
                }
            }
        }
        let Some(picker) = self.picker.clone() else {
            return;
        };
        let Some(sender) = &self.job_sender else {
            return;
        };
        let Some(cancel) = self.cancel_worker.as_ref().map(Arc::clone) else {
            return;
        };
        self.entries.insert(
            block_id,
            ImageEntry {
                key: key.clone(),
                state: ImageState::Loading,
            },
        );
        let _ = sender.send(ImageJob {
            block_id,
            key,
            size,
            picker,
            path,
            attempts,
            cancel,
        });
    }
}

impl App {
    pub(crate) fn set_kitty_images_enabled(&mut self, enabled: bool) {
        self.kitty_images_enabled = enabled;
        self.kitty_images_enable_requested = false;
        if !enabled {
            self.image_runtime.disable();
        }
    }

    #[cfg(test)]
    pub(crate) fn is_kitty_images_enabled(&self) -> bool {
        self.kitty_images_enabled
    }

    pub(crate) fn is_kitty_images_rendering(&self) -> bool {
        self.kitty_images_enabled && self.image_runtime.is_active()
    }

    #[cfg(test)]
    pub(crate) fn set_kitty_images_rendering_for_test(&mut self) {
        self.image_runtime.terminal_ready = true;
        self.image_runtime.picker = Some(ratatui_image::picker::Picker::halfblocks());
        self.image_runtime.active = true;
    }

    #[cfg(test)]
    pub(crate) fn kitty_images_enable_requested(&self) -> bool {
        self.kitty_images_enable_requested
    }

    pub(crate) fn toggle_kitty_images(&mut self, ss: &SyntaxSet, themes: &ThemeSet) {
        if self.kitty_images_enabled {
            self.kitty_images_enabled = false;
            self.kitty_images_enable_requested = false;
            self.image_runtime.disable();
            self.set_image_flash(ImageFlash::Disabled);
            self.reparse_source(ss, themes);
            return;
        }
        if !is_kitty_terminal() {
            self.set_image_flash(ImageFlash::Unavailable);
            return;
        }
        self.kitty_images_enabled = true;
        self.kitty_images_enable_requested = true;
        self.reparse_source(ss, themes);
        if !self.image_blocks.iter().any(|block| block.renderable) {
            self.kitty_images_enabled = false;
            self.kitty_images_enable_requested = false;
            self.image_runtime.disable();
            self.reparse_source(ss, themes);
            self.set_image_flash(ImageFlash::NoImages);
        }
    }

    pub(crate) fn reconcile_kitty_images_after_content_change(&mut self) -> bool {
        if self.kitty_images_enable_requested
            || !self.kitty_images_enabled
            || !self.image_runtime.terminal_ready
            || self.image_runtime.is_active()
            || !self.image_blocks.iter().any(|block| block.renderable)
        {
            return false;
        }
        self.kitty_images_enable_requested = true;
        true
    }

    pub(crate) fn process_pending_kitty_images(&mut self) -> bool {
        if !std::mem::take(&mut self.kitty_images_enable_requested) {
            return false;
        }
        if self.try_enable_kitty_images() {
            self.set_image_flash(ImageFlash::Enabled);
        } else {
            self.kitty_images_enabled = false;
            self.image_runtime.disable();
            self.set_image_flash(ImageFlash::Unavailable);
        }
        true
    }

    pub(crate) fn initialize_kitty_images(&mut self) -> bool {
        self.image_runtime.terminal_ready = true;
        if !self.kitty_images_enabled || !self.image_blocks.iter().any(|block| block.renderable) {
            return self.image_runtime.is_active();
        }
        if self.image_runtime.enable_kitty() {
            return true;
        }
        self.kitty_images_enabled = false;
        self.image_runtime.disable();
        false
    }

    pub(crate) fn try_enable_kitty_images(&mut self) -> bool {
        if !self.kitty_images_enabled
            || !self.image_runtime.terminal_ready
            || !self.image_blocks.iter().any(|block| block.renderable)
            || !is_kitty_terminal()
        {
            return self.image_runtime.is_active();
        }
        self.image_runtime.enable_kitty()
    }

    pub(crate) fn poll_image_results(&mut self) -> bool {
        self.image_runtime.poll_results()
    }

    pub(crate) fn image_poll_delay(&self) -> Option<Duration> {
        self.image_runtime.poll_delay()
    }

    pub(crate) fn set_image_document_path(&mut self, path: Option<&Path>) {
        self.image_runtime.set_document_path(path);
    }

    pub(crate) fn render_images(&mut self, frame: &mut Frame<'_>, content_area: Rect) {
        let scroll = self.scroll();
        let x_offset = self.line_number_gutter_width();
        let blocks = self.image_blocks.clone();
        self.image_runtime
            .render(frame, &blocks, content_area, scroll, x_offset);
    }
}

fn retry_delay(attempts: u8) -> Duration {
    let exponent = u32::from(attempts.min(5));
    Duration::from_secs(1u64 << exponent)
}

fn decode_and_prepare(job: &ImageJob) -> AnyResult<SlicedProtocol> {
    let _decode_guard = IMAGE_DECODE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if job.cancel.load(Ordering::Relaxed) {
        anyhow::bail!("image decode cancelled");
    }
    if job.key.len > MAX_IMAGE_FILE_BYTES {
        anyhow::bail!("image file exceeds {} bytes", MAX_IMAGE_FILE_BYTES);
    }
    let mut reader = ImageReader::open(&job.path)?.with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_IMAGE_ALLOC_BYTES);
    reader.limits(limits);
    let image = reader.decode()?;
    Ok(SlicedProtocol::new_with_resize(
        &job.picker,
        image,
        job.size,
        Resize::Fit(None),
    )?)
}

fn decode_job_guarded(job: &ImageJob) -> AnyResult<SlicedProtocol> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decode_and_prepare(job)))
        .unwrap_or_else(|_| Err(anyhow::anyhow!("image decoder panicked")))
}

fn document_dir(document_path: Option<&Path>) -> PathBuf {
    document_path
        .and_then(Path::parent)
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn has_supported_extension(source: &str) -> bool {
    Path::new(source)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp"
            )
        })
}

fn is_remote_source(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.starts_with("data:") || lower.contains("://")
}

fn is_kitty_terminal() -> bool {
    is_kitty_environment(|name| env::var(name).ok())
}

fn is_kitty_environment(mut lookup: impl FnMut(&str) -> Option<String>) -> bool {
    ["KITTY_PID", "KITTY_WINDOW_ID"]
        .iter()
        .any(|name| lookup(name).is_some_and(|value| !value.is_empty()))
        || lookup("TERM").is_some_and(|term| term.eq_ignore_ascii_case("xterm-kitty"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};

    static IMAGE_ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard(Vec<(&'static str, Option<std::ffi::OsString>)>);

    impl EnvGuard {
        fn save(names: &[&'static str]) -> Self {
            Self(
                names
                    .iter()
                    .map(|name| (*name, std::env::var_os(name)))
                    .collect(),
            )
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (name, value) in &self.0 {
                if let Some(value) = value {
                    std::env::set_var(name, value);
                } else {
                    std::env::remove_var(name);
                }
            }
        }
    }

    fn test_parse_assets() -> (SyntaxSet, ThemeSet) {
        (
            SyntaxSet::load_defaults_newlines(),
            ThemeSet::load_defaults(),
        )
    }

    fn test_image_block(id: usize) -> ImageBlockInfo {
        ImageBlockInfo {
            id,
            source: "fixture.png".to_string(),
            alt: "fixture".to_string(),
            title: String::new(),
            source_line: 1,
            rendered_start: 0,
            rendered_end: 11,
            rendered_width: 22,
            prefix_width: 0,
            renderable: true,
        }
    }

    #[test]
    fn kitty_environment_detection_accepts_supported_markers() {
        for marker in ["KITTY_PID", "KITTY_WINDOW_ID"] {
            let values = HashMap::from([(marker.to_string(), "1".to_string())]);
            assert!(is_kitty_environment(|name| values.get(name).cloned()));
        }
        let values = HashMap::from([("TERM".to_string(), "xterm-kitty".to_string())]);
        assert!(is_kitty_environment(|name| values.get(name).cloned()));
    }

    #[test]
    fn kitty_environment_detection_rejects_empty_or_other_terminals() {
        let values = HashMap::from([
            ("KITTY_PID".to_string(), String::new()),
            ("TERM".to_string(), "xterm-256color".to_string()),
        ]);
        assert!(!is_kitty_environment(|name| values.get(name).cloned()));
    }

    #[test]
    fn remote_and_svg_sources_are_rejected() {
        assert!(is_remote_source("https://example.com/image.png"));
        assert!(is_remote_source("data:image/png;base64,AAAA"));
        assert!(!has_supported_extension("image.svg"));
        assert!(has_supported_extension("image.PNG"));
    }

    #[test]
    fn disable_stops_runtime_and_cancels_worker() {
        let mut runtime = ImageRuntime::new(None);
        let (job_sender, _job_receiver) = mpsc::channel();
        let (_result_sender, result_receiver) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        runtime.picker = Some(Picker::halfblocks());
        runtime.job_sender = Some(job_sender);
        runtime.result_receiver = Some(result_receiver);
        runtime.cancel_worker = Some(Arc::clone(&cancel));
        runtime.entries.insert(
            1,
            ImageEntry {
                key: ImageKey {
                    path: PathBuf::from("fixture.png"),
                    modified: None,
                    len: 1,
                    size: ImageSize {
                        width: 1,
                        height: 1,
                    },
                },
                state: ImageState::Loading,
            },
        );
        runtime.active = true;

        runtime.disable();

        assert!(!runtime.is_active());
        assert!(runtime.job_sender.is_none());
        assert!(runtime.result_receiver.is_none());
        assert!(runtime.entries.is_empty());
        assert!(runtime.picker.is_some());
        assert!(cancel.load(Ordering::Relaxed));
        assert!(!runtime.poll_results());
        assert!(runtime.poll_delay().is_none());
    }

    #[test]
    fn app_starts_with_images_disabled_and_reports_no_images() {
        let _guard = IMAGE_ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::save(&["KITTY_PID", "KITTY_WINDOW_ID", "TERM"]);
        std::env::remove_var("KITTY_WINDOW_ID");
        std::env::set_var("KITTY_PID", "1");
        std::env::set_var("TERM", "xterm-256color");
        let (ss, themes) = test_parse_assets();
        let mut app = App::new(
            Vec::new(),
            Vec::new(),
            "stdin".to_string(),
            false,
            false,
            None,
            None,
        );
        assert!(!app.is_kitty_images_enabled());
        app.toggle_kitty_images(&ss, &themes);
        assert!(!app.is_kitty_images_enabled());
        assert!(app.image_blocks.is_empty());
        assert!(matches!(
            app.image_flash().map(|(flash, _)| flash),
            Some(ImageFlash::NoImages)
        ));
    }

    #[test]
    fn toggle_without_kitty_reports_unavailable_without_enabling() {
        let _guard = IMAGE_ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::save(&["KITTY_PID", "KITTY_WINDOW_ID", "TERM"]);
        std::env::remove_var("KITTY_PID");
        std::env::remove_var("KITTY_WINDOW_ID");
        std::env::set_var("TERM", "xterm-256color");
        let (ss, themes) = test_parse_assets();
        let source = "![fixture](fixture.png)".to_string();
        let mut app = App::new_with_source(
            Vec::new(),
            Vec::new(),
            AppConfig {
                filename: "stdin".to_string(),
                source: source.clone(),
                debug_input: false,
                watch: false,
                filepath: None,
                last_file_state: None,
            },
        );
        app.set_image_blocks(vec![test_image_block(1)]);

        app.toggle_kitty_images(&ss, &themes);

        assert!(!app.is_kitty_images_enabled());
        assert!(!app.kitty_images_enable_requested());
        assert!(!app.image_runtime.is_active());
        assert!(matches!(
            app.image_flash().map(|(flash, _)| flash),
            Some(ImageFlash::Unavailable)
        ));
    }

    #[test]
    fn configured_enabled_state_survives_initial_empty_picker_document() {
        let mut app = App::new(
            Vec::new(),
            Vec::new(),
            "stdin".to_string(),
            false,
            false,
            None,
            None,
        );
        app.set_kitty_images_enabled(true);
        assert!(!app.initialize_kitty_images());
        assert!(app.is_kitty_images_enabled());
        assert!(!app.image_runtime.is_active());
    }

    #[test]
    fn configured_enable_failure_after_picker_load_returns_to_off() {
        let _guard = IMAGE_ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::save(&["KITTY_PID", "KITTY_WINDOW_ID", "TERM"]);
        std::env::remove_var("KITTY_PID");
        std::env::remove_var("KITTY_WINDOW_ID");
        std::env::set_var("TERM", "xterm-256color");

        let mut app = App::new(
            Vec::new(),
            Vec::new(),
            "stdin".to_string(),
            false,
            false,
            None,
            None,
        );
        app.set_kitty_images_enabled(true);
        assert!(!app.initialize_kitty_images());
        assert!(app.is_kitty_images_enabled());
        app.set_image_blocks(vec![test_image_block(1)]);
        assert!(app.kitty_images_enable_requested());
        assert!(!app.image_runtime.is_active());
        app.process_pending_kitty_images();

        assert!(!app.is_kitty_images_enabled());
        assert!(matches!(
            app.image_flash().map(|(flash, _)| flash),
            Some(ImageFlash::Unavailable)
        ));
    }

    #[test]
    fn app_toggle_disables_active_images_immediately() {
        let (ss, themes) = test_parse_assets();
        let mut app = App::new(
            Vec::new(),
            Vec::new(),
            "stdin".to_string(),
            false,
            false,
            None,
            None,
        );
        app.set_image_blocks(vec![test_image_block(1)]);
        app.set_kitty_images_enabled(true);
        app.image_runtime.active = true;
        app.toggle_kitty_images(&ss, &themes);
        assert!(!app.is_kitty_images_enabled());
        assert!(!app.image_runtime.is_active());
        assert!(app.image_blocks.is_empty());
        assert!(matches!(
            app.image_flash().map(|(flash, _)| flash),
            Some(ImageFlash::Disabled)
        ));
    }

    #[test]
    fn app_toggle_defers_kitty_query_until_runtime_processing() {
        let _guard = IMAGE_ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::save(&["KITTY_PID", "KITTY_WINDOW_ID", "TERM"]);
        std::env::remove_var("KITTY_WINDOW_ID");
        std::env::set_var("KITTY_PID", "1");
        std::env::set_var("TERM", "xterm-256color");
        let (ss, themes) = test_parse_assets();
        let source = "![fixture](fixture.png)".to_string();
        let mut app = App::new_with_source(
            Vec::new(),
            Vec::new(),
            AppConfig {
                filename: "stdin".to_string(),
                source: source.clone(),
                debug_input: false,
                watch: false,
                filepath: None,
                last_file_state: None,
            },
        );
        app.set_image_blocks(vec![test_image_block(1)]);
        app.toggle_kitty_images(&ss, &themes);
        assert!(app.is_kitty_images_enabled());
        assert!(app.kitty_images_enable_requested());
        assert!(!app.image_runtime.is_active());
        assert!(app.image_blocks.iter().any(|block| block.renderable));
    }

    #[test]
    fn deferred_request_enables_cached_picker_without_stdio_query() {
        let _guard = IMAGE_ENV_LOCK.lock().unwrap();
        let _env = EnvGuard::save(&["KITTY_PID", "KITTY_WINDOW_ID", "TERM"]);
        std::env::set_var("KITTY_PID", "1");
        let (ss, themes) = test_parse_assets();
        let source = "![fixture](fixture.png)".to_string();
        let mut app = App::new_with_source(
            Vec::new(),
            Vec::new(),
            AppConfig {
                filename: "stdin".to_string(),
                source: source.clone(),
                debug_input: false,
                watch: false,
                filepath: None,
                last_file_state: None,
            },
        );
        app.set_image_blocks(vec![test_image_block(1)]);
        app.toggle_kitty_images(&ss, &themes);
        app.image_runtime.terminal_ready = true;
        app.image_runtime.picker = Some(Picker::halfblocks());

        assert!(app.process_pending_kitty_images());
        assert!(app.is_kitty_images_rendering());
        assert!(matches!(
            app.image_flash().map(|(flash, _)| flash),
            Some(ImageFlash::Enabled)
        ));

        app.image_runtime.disable();
    }

    #[test]
    fn failed_image_is_retried_after_backoff() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut runtime = ImageRuntime::new(Some(&manifest_dir.join("README.md")));
        let block = ImageBlockInfo {
            id: 9,
            source: "src/tests/fixtures/kitty-image.png".to_string(),
            alt: "fixture".to_string(),
            title: String::new(),
            source_line: 1,
            rendered_start: 0,
            rendered_end: 11,
            rendered_width: 22,
            prefix_width: 0,
            renderable: true,
        };
        let (key, path, size) = runtime.resolve_key(&block).expect("fixture image");
        let (sender, receiver) = mpsc::channel();
        runtime.picker = Some(Picker::halfblocks());
        runtime.active = true;
        runtime.job_sender = Some(sender);
        runtime.cancel_worker = Some(Arc::new(AtomicBool::new(false)));
        runtime.entries.insert(
            block.id,
            ImageEntry {
                key: key.clone(),
                state: ImageState::Failed {
                    attempts: 0,
                    retry_at: Instant::now()
                        .checked_sub(Duration::from_secs(1))
                        .expect("past retry time"),
                },
            },
        );

        runtime.ensure_loaded(block.id, key, path, size);
        assert!(matches!(
            runtime.entries.get(&block.id).map(|entry| &entry.state),
            Some(ImageState::Loading)
        ));
        let job = receiver.try_recv().expect("retry job");
        assert_eq!(job.attempts, 1);
    }

    #[test]
    fn kitty_protocol_renders_placeholder_into_test_buffer() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut runtime = ImageRuntime::new(Some(&manifest_dir.join("README.md")));
        let block = ImageBlockInfo {
            id: 7,
            source: "src/tests/fixtures/kitty-image.png".to_string(),
            alt: "fixture".to_string(),
            title: String::new(),
            source_line: 1,
            rendered_start: 0,
            rendered_end: 11,
            rendered_width: 22,
            prefix_width: 0,
            renderable: true,
        };
        let (key, path, size) = runtime.resolve_key(&block).expect("fixture image");
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(ProtocolType::Kitty);
        let protocol = decode_job_guarded(&ImageJob {
            block_id: block.id,
            key: key.clone(),
            size,
            picker: picker.clone(),
            path,
            attempts: 0,
            cancel: Arc::new(AtomicBool::new(false)),
        })
        .expect("decode fixture");
        assert!(matches!(protocol, SlicedProtocol::Kitty(_)));
        runtime.picker = Some(picker);
        runtime.active = true;
        runtime.entries.insert(
            block.id,
            ImageEntry {
                key,
                state: ImageState::Ready(protocol),
            },
        );

        let backend = ratatui::backend::TestBackend::new(30, 12);
        let mut terminal = ratatui::Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| runtime.render(frame, &[block], frame.area(), 0, 4))
            .expect("draw image");
        let buffer = terminal.backend().buffer();
        assert!(
            buffer
                .cell((5, 1))
                .is_some_and(|cell| cell.symbol().contains('\u{10EEEE}')),
            "Kitty placeholder should honor the line-number gutter offset"
        );
    }
}
