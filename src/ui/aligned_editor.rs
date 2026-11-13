use super::text_view::TextView;
#[cfg(test)]
use crate::diff::DiffStatus;
use crate::diff::{DiffEngine, DiffResult, FilterMode, InlineChunkKind};
use egui::{text::LayoutJob, Align, Color32, FontId, Galley, Id, Rect, TextFormat, Ui};
use std::ops::Range;
use std::sync::Arc;

pub mod wrap;

pub const ROW_HEIGHT: f32 = 16.0;
/// Consolas at LOGFONT -13 (10 pt at 96 DPI). Runtime evidence, 13/11/2026: the
/// original with a fresh portable profile draws the text panes in Consolas; its
/// glyph outlines match a GDI Consolas -13 rendering, while Courier New does not.
/// The executable lists "Consolas" next to "Courier New" (fallback).
pub const FONT_SIZE: f32 = 13.0;

/// The Text Compare editor font (family registered by `theme::apply`).
pub fn editor_font() -> FontId {
    FontId::new(FONT_SIZE, egui::FontFamily::Name("Text Mono".into()))
}
// Measured on the original at 100%: 35 px gutter, text starts at its edge.
pub const GUTTER_WIDTH: f32 = 35.0;
pub const TEXT_OFFSET: f32 = 35.0;

#[derive(Clone, Copy, PartialEq)]
pub struct LayoutOptions {
    pub left: bool,
    pub active: bool,
    pub filter: FilterMode,
    pub context: bool,
    pub revision: u64,
    pub view_revision: u64,
    pub paint_range: [i32; 2],
    pub unchanged: bool,
    pub rules: crate::diff::DiffRules,
    pub wrap_widths: Option<[f32; 2]>,
}

#[derive(Default)]
pub struct EditorCache {
    /// Word-wrap mode: galley with meshes for the painted range.
    entry: Option<CacheEntry>,
    /// Unwrapped mode: document-sized galley holding cursor metrics only.
    plain: Option<PlainEntry>,
    pub widget_id: Option<Id>,
    pub widget_rect: Option<Rect>,
    pending: Option<PendingComparison>,
    pending_left: Option<bool>,
    /// The unedited buffer while TextEdit applies this frame's edit events.
    edit_base: Option<String>,
    wrap_cache: wrap::WrapCache,
}

struct PendingComparison {
    other: String,
    diff: DiffResult,
    view: TextView,
    ms: u128,
}

struct CacheEntry {
    text: String,
    options: LayoutOptions,
    scale: f32,
    layout: EditorLayout,
    signature: u64,
}

#[derive(Clone, Copy, PartialEq)]
struct GeometryKey {
    revision: u64,
    view_revision: u64,
    filter: FilterMode,
    context: bool,
    left: bool,
}

/// The unwrapped editor never re-creates per-character data while scrolling:
/// rows carry only cursor metrics, the visible text is painted separately
/// ([`paint_rows`]) and an edit replaces just the edited lines in place.
struct PlainEntry {
    text: String,
    line_starts: Vec<usize>,
    galley: Arc<Galley>,
    leading_y: f32,
    geometry: Option<GeometryKey>,
    scale: f32,
}

#[derive(Clone)]
pub struct EditorLayout {
    pub galley: Arc<Galley>,
    pub leading_y: f32,
}

/// Advance widths of the monospace editor font, shared by all builder threads.
struct GlyphMetrics {
    template: egui::epaint::text::Glyph,
    ascii: [f32; 128],
    unicode: std::collections::HashMap<char, f32>,
    pixels_per_point: f32,
}

impl GlyphMetrics {
    fn new(fonts: &egui::epaint::Fonts, text: &str) -> Self {
        let font = editor_font();
        let sample = fonts.layout_job(LayoutJob::simple(
            "M".into(),
            font.clone(),
            Color32::BLACK,
            f32::INFINITY,
        ));
        let mut template = sample.rows[0].glyphs[0];
        template.uv_rect = Default::default();
        template.section_index = 0;
        let mut ascii = [0.0; 128];
        for (index, width) in ascii.iter_mut().enumerate().skip(1) {
            *width = fonts.glyph_width(&font, char::from(index as u8));
        }
        let mut unicode = std::collections::HashMap::new();
        if !text.is_ascii() {
            for chr in text.chars().filter(|c| !c.is_ascii()) {
                unicode
                    .entry(chr)
                    .or_insert_with(|| fonts.glyph_width(&font, chr));
            }
        }
        Self {
            template,
            ascii,
            unicode,
            pixels_per_point: fonts.pixels_per_point(),
        }
    }

    /// One row of cursor metrics (no mesh). Glyph y is row-relative; only
    /// `row.rect` carries the document position, which is all egui reads.
    fn row(&self, content: &str) -> egui::epaint::text::Row {
        use egui::epaint::text::{Row, RowVisuals};
        let ppp = self.pixels_per_point;
        let mut x = 0.0;
        let glyphs: Vec<_> = content
            .chars()
            .map(|chr| {
                let width = if chr.is_ascii() {
                    self.ascii[chr as usize]
                } else {
                    self.unicode
                        .get(&chr)
                        .copied()
                        .unwrap_or(self.ascii[b'M' as usize])
                };
                let mut glyph = self.template;
                glyph.chr = chr;
                glyph.pos = egui::pos2(x, self.template.pos.y);
                glyph.advance_width = width;
                x = ((x + width) * ppp).round() / ppp;
                glyph
            })
            .collect();
        let right = glyphs.last().map(|glyph| glyph.max_x()).unwrap_or(0.0);
        Row {
            glyphs,
            section_index_at_start: 0,
            rect: Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(right, ROW_HEIGHT)),
            visuals: RowVisuals::default(),
            ends_with_newline: false,
        }
    }

    /// Rows for consecutive lines, built on all cores for large documents.
    fn rows(&self, lines: &[&str]) -> Vec<egui::epaint::text::Row> {
        let bytes: usize = lines.iter().map(|line| line.len()).sum();
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(16);
        if threads < 2 || bytes < 256 * 1024 {
            return lines.iter().map(|line| self.row(line)).collect();
        }
        let chunk = lines.len().div_ceil(threads * 4).max(1);
        let parts: Vec<Vec<_>> = std::thread::scope(|scope| {
            let handles: Vec<_> = lines
                .chunks(chunk)
                .map(|part| scope.spawn(move || part.iter().map(|line| self.row(line)).collect()))
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let mut rows = Vec::with_capacity(lines.len());
        for part in parts {
            rows.extend(part);
        }
        rows
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = Vec::with_capacity(text.len() / 32 + 1);
    starts.push(0);
    starts.extend(
        text.bytes()
            .enumerate()
            .filter(|(_, b)| *b == b'\n')
            .map(|(i, _)| i + 1),
    );
    starts
}

use crate::diff::{common_prefix, common_suffix};

fn line_of(starts: &[usize], byte: usize) -> usize {
    starts
        .partition_point(|&start| start <= byte)
        .saturating_sub(1)
}

/// Document rows from the aligned view: real lines take the vertical position of
/// their aligned row and the height of the gaps that follow. Same geometry as the
/// previous whole-document layout; only rects change, glyphs are untouched.
fn apply_geometry(galley: &mut Galley, diff: &DiffResult, view: &TextView, left: bool) -> f32 {
    let physical_count = galley.rows.len();
    let logical = if galley.job.text.ends_with('\n') || galley.job.text.is_empty() {
        physical_count - 1
    } else {
        physical_count
    };
    let mut mapped: Vec<Option<f32>> = vec![None; logical];
    for &index in &view.visible {
        let line = &diff.lines[index];
        if let Some(number) = if left {
            line.left_line_no
        } else {
            line.right_line_no
        } {
            if let Some(slot) = mapped.get_mut(number - 1) {
                *slot = view.line_y[index];
            }
        }
    }
    let leading_y = mapped
        .iter()
        .flatten()
        .next()
        .copied()
        .unwrap_or(view.height);
    let mut heights = vec![0.0; logical];
    let mut next_y = view.height;
    for (index, y) in mapped.iter().enumerate().rev() {
        if let Some(y) = y {
            heights[index] = (next_y - y).max(ROW_HEIGHT);
            next_y = *y;
        }
    }
    let mut previous_y = 0.0;
    let mut max_x = 0.0_f32;
    for (physical, row) in galley.rows.iter_mut().enumerate() {
        let eof = physical >= logical;
        let y = match mapped.get(physical).copied().flatten() {
            Some(y) => y - leading_y,
            None if eof => (view.height - leading_y).max(0.0),
            None => previous_y,
        };
        let height = heights.get(physical).copied().unwrap_or(ROW_HEIGHT);
        // Rows start at x = 0 and keep their width from the build, so the glyph
        // vectors (scattered over the heap) are not touched here.
        let right = row.rect.width();
        row.rect = Rect::from_min_max(egui::pos2(0.0, y), egui::pos2(right, y + height));
        row.ends_with_newline = physical + 1 < physical_count;
        max_x = max_x.max(right);
        previous_y = y;
    }
    galley.rect = Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(
            max_x,
            (view.height - leading_y + ROW_HEIGHT).max(ROW_HEIGHT),
        ),
    );
    galley.mesh_bounds = Rect::NOTHING;
    galley.num_vertices = 0;
    galley.num_indices = 0;
    leading_y
}

fn plain_job(text: &str) -> Arc<LayoutJob> {
    let mut job = LayoutJob::simple(
        text.to_owned(),
        editor_font(),
        Color32::BLACK,
        f32::INFINITY,
    );
    job.first_row_min_height = ROW_HEIGHT;
    Arc::new(job)
}

/// Paint the colored text of the rows inside the painter's clip (the editor
/// galley has no meshes). Lines go through egui's galley cache, so a scroll
/// only shapes the lines that just became visible.
#[allow(clippy::too_many_arguments)]
pub fn paint_rows(
    ui: &Ui,
    painter: &egui::Painter,
    origin: egui::Pos2,
    diff: &DiffResult,
    view: &TextView,
    left: bool,
    show_whitespace: bool,
) {
    use super::text_view::RowKind;
    let clip = painter.clip_rect();
    let top = clip.top() - origin.y - ROW_HEIGHT;
    let bottom = clip.bottom() - origin.y;
    let first = view
        .rows
        .partition_point(|row| row.top + row.height() < top);
    let font = editor_font();
    for row in view.rows[first..].iter() {
        if row.top > bottom {
            break;
        }
        let RowKind::Line(index) = row.kind else {
            continue;
        };
        let line = &diff.lines[index];
        let (number, chunks) = if left {
            (line.left_line_no, &line.left_inline)
        } else {
            (line.right_line_no, &line.right_inline)
        };
        if number.is_none() {
            continue;
        }
        let mut job = LayoutJob::default();
        job.wrap.max_width = f32::INFINITY;
        job.first_row_min_height = ROW_HEIGHT;
        for chunk in chunks.iter() {
            job.append(
                &chunk.text,
                0.0,
                TextFormat {
                    font_id: font.clone(),
                    line_height: Some(ROW_HEIGHT),
                    valign: Align::TOP,
                    color: match chunk.kind {
                        InlineChunkKind::Diff => Color32::RED,
                        InlineChunkKind::Unimportant => Color32::BLUE,
                        InlineChunkKind::Same => Color32::BLACK,
                    },
                    ..Default::default()
                },
            );
        }
        let galley = ui.fonts(|fonts| fonts.layout_job(job));
        let pos = egui::pos2(
            origin.x,
            origin.y + row.top - galley.rows.first().map_or(0.0, |r| r.rect.top()),
        );
        if show_whitespace {
            if let Some(first_row) = galley.rows.first() {
                for glyph in &first_row.glyphs {
                    let marker = match glyph.chr {
                        ' ' => "·",
                        '\t' => "→",
                        _ => continue,
                    };
                    painter.text(
                        pos + glyph.pos.to_vec2(),
                        egui::Align2::LEFT_BOTTOM,
                        marker,
                        font.clone(),
                        Color32::from_gray(130),
                    );
                }
            }
        }
        painter.galley(pos, galley, Color32::BLACK);
    }
}

impl EditorCache {
    #[cfg(test)]
    pub fn test_layout(&self) -> EditorLayout {
        if let Some(plain) = &self.plain {
            return EditorLayout {
                galley: plain.galley.clone(),
                leading_y: plain.leading_y,
            };
        }
        self.entry.as_ref().unwrap().layout.clone()
    }

    /// Whether the visible text comes from [`paint_rows`] (unwrapped mode).
    pub fn paints_rows(&self) -> bool {
        self.plain.is_some()
    }

    pub fn take_comparison(
        &mut self,
        left: &str,
        right: &str,
    ) -> Option<(DiffResult, u128, TextView)> {
        let pending = self.pending.take()?;
        let is_left = self.pending_left?;
        let own = match (&self.plain, &self.entry) {
            (Some(plain), _) => plain.text.as_str(),
            (None, Some(entry)) => entry.text.as_str(),
            _ => return None,
        };
        let (mine, other) = if is_left {
            (left, right)
        } else {
            (right, left)
        };
        (own == mine && pending.other == other).then_some((pending.diff, pending.ms, pending.view))
    }

    /// The diff and aligned view to paint this frame: the comparison of the
    /// buffer just edited (not yet committed by the screen), when there is one.
    pub fn painted<'a>(
        &'a self,
        diff: &'a DiffResult,
        view: &'a TextView,
    ) -> (&'a DiffResult, &'a TextView) {
        match &self.pending {
            Some(pending) => (&pending.diff, &pending.view),
            None => (diff, view),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn layout(
        &mut self,
        ui: &Ui,
        text: &str,
        other: &str,
        diff: &DiffResult,
        options: LayoutOptions,
        revealed: &[Range<usize>],
        view: Option<&TextView>,
    ) -> EditorLayout {
        if options.wrap_widths.is_some() {
            self.plain = None;
            return self.layout_wrapped(ui, text, other, diff, options, revealed);
        }
        self.entry = None;
        self.wrap_cache.clear();
        let scale = ui.ctx().pixels_per_point();
        let timer = std::time::Instant::now();
        if options.unchanged {
            self.edit_base = None;
        }
        let temporary = (!options.unchanged).then(|| match &self.plain {
            // The buffer as of this frame's unchanged layout is the base of every
            // edit event of the frame; `diff` describes exactly that buffer.
            Some(plain) if options.rules == Default::default() => {
                let base = self.edit_base.get_or_insert_with(|| plain.text.clone());
                DiffEngine::update_edit(base, text, other, options.left, diff)
            }
            _ if options.left => DiffEngine::update_rules(text, other, diff, options.rules),
            _ => DiffEngine::update_rules(other, text, diff, options.rules),
        });
        let comparison_ms = timer.elapsed().as_millis();
        let key = options.unchanged.then_some(GeometryKey {
            revision: options.revision,
            view_revision: options.view_revision,
            filter: options.filter,
            context: options.context,
            left: options.left,
        });
        let reusable = self
            .plain
            .as_ref()
            .is_some_and(|plain| plain.scale == scale);
        let text_changed = !reusable || self.plain.as_ref().unwrap().text != text;
        let geometry_changed =
            !reusable || key.is_none() || self.plain.as_ref().unwrap().geometry != key;
        if !text_changed && !geometry_changed {
            let plain = self.plain.as_ref().unwrap();
            return EditorLayout {
                galley: plain.galley.clone(),
                leading_y: plain.leading_y,
            };
        }
        let current = temporary.as_ref().unwrap_or(diff);
        let temporary_view = temporary
            .as_ref()
            .map(|diff| TextView::new(diff, options.filter, options.context, revealed));
        let owned_view;
        let view = match (&temporary_view, view) {
            (Some(view), _) => view,
            (None, Some(view)) => view,
            (None, None) => {
                owned_view = TextView::new(current, options.filter, options.context, revealed);
                &owned_view
            }
        };
        if !reusable {
            let lines: Vec<&str> = text.split('\n').collect();
            let rows = ui.fonts(|fonts| GlyphMetrics::new(fonts, text).rows(&lines));
            self.plain = Some(PlainEntry {
                text: text.to_owned(),
                line_starts: line_starts(text),
                galley: Arc::new(Galley {
                    job: plain_job(text),
                    rows,
                    elided: false,
                    rect: Rect::NOTHING,
                    mesh_bounds: Rect::NOTHING,
                    num_vertices: 0,
                    num_indices: 0,
                    pixels_per_point: scale,
                }),
                leading_y: 0.0,
                geometry: None,
                scale,
            });
        } else if text_changed {
            let plain = self.plain.as_mut().unwrap();
            let (old, new) = (plain.text.as_bytes(), text.as_bytes());
            let prefix = common_prefix(old, new);
            let suffix = common_suffix(old, new, old.len().min(new.len()) - prefix);
            let first = line_of(&plain.line_starts, prefix);
            let last_old = line_of(&plain.line_starts, old.len() - suffix).max(first);
            let start = plain.line_starts[first];
            let delta = new.len() as isize - old.len() as isize;
            let end_new = plain
                .line_starts
                .get(last_old + 1)
                .map_or(new.len(), |&next| {
                    next.checked_add_signed(delta).unwrap() - 1
                });
            let region = &text[start..end_new];
            let lines: Vec<&str> = region.split('\n').collect();
            let rows = ui.fonts(|fonts| GlyphMetrics::new(fonts, region).rows(&lines));
            let mut starts = Vec::with_capacity(lines.len());
            let mut at = start;
            for line in &lines {
                starts.push(at);
                at += line.len() + 1;
            }
            let tail: Vec<usize> = plain.line_starts[last_old + 1..]
                .iter()
                .map(|&s| s.checked_add_signed(delta).unwrap())
                .collect();
            plain.line_starts.truncate(first);
            plain.line_starts.extend(starts);
            plain.line_starts.extend(tail);
            let galley = Arc::make_mut(&mut plain.galley);
            galley.rows.splice(first..=last_old, rows);
            galley.job = plain_job(text);
            plain.text.clear();
            plain.text.push_str(text);
            debug_assert_eq!(plain.line_starts.len(), galley.rows.len());
        }
        let plain = self.plain.as_mut().unwrap();
        let galley = Arc::make_mut(&mut plain.galley);
        plain.leading_y = apply_geometry(galley, current, view, options.left);
        plain.geometry = key;
        let layout = EditorLayout {
            galley: plain.galley.clone(),
            leading_y: plain.leading_y,
        };
        if let (Some(diff), Some(view)) = (temporary, temporary_view) {
            self.pending_left = Some(options.left);
            self.pending = Some(PendingComparison {
                other: other.to_owned(),
                view,
                diff,
                ms: comparison_ms,
            });
        }
        layout
    }

    fn layout_wrapped(
        &mut self,
        ui: &Ui,
        text: &str,
        other: &str,
        diff: &DiffResult,
        options: LayoutOptions,
        revealed: &[Range<usize>],
    ) -> EditorLayout {
        let scale = ui.ctx().pixels_per_point();
        // TextEdit can ask for a new galley while processing an input event, before
        // the screen commits the edit. Recompare that exact buffer for immediate color updates.
        let timer = std::time::Instant::now();
        let temporary = (!options.unchanged).then(|| {
            if options.left {
                DiffEngine::update_rules(text, other, diff, options.rules)
            } else {
                DiffEngine::update_rules(other, text, diff, options.rules)
            }
        });
        let comparison_ms = timer.elapsed().as_millis();
        let current = temporary.as_ref().unwrap_or(diff);
        let mut view = TextView::new(current, options.filter, options.context, revealed);
        if !options.unchanged {
            self.wrap_cache.clear();
        }
        let widths = options.wrap_widths.unwrap();
        let geometry = self.wrap_cache.get(ui, current, options.revision, widths);
        view = view.with_heights(&geometry.heights);
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for &index in &view.visible {
            let line = &current.lines[index];
            let number = if options.left {
                line.left_line_no
            } else {
                line.right_line_no
            };
            if let Some(number) = number {
                number.hash(&mut hash);
                let y = view.line_y[index].unwrap();
                y.to_bits().hash(&mut hash);
                if y + ROW_HEIGHT >= options.paint_range[0] as f32
                    && y <= options.paint_range[1] as f32
                {
                    for chunk in (if options.left {
                        &line.left_inline
                    } else {
                        &line.right_inline
                    })
                    .iter()
                    {
                        chunk.text.hash(&mut hash);
                        chunk.kind.hash(&mut hash);
                    }
                }
            }
        }
        view.height.to_bits().hash(&mut hash);
        let signature = hash.finish();
        if let Some(entry) = &self.entry {
            let same_options = LayoutOptions {
                revision: entry.options.revision,
                unchanged: entry.options.unchanged,
                ..options
            } == entry.options;
            if entry.text == text
                && same_options
                && entry.scale == scale
                && entry.signature == signature
            {
                return entry.layout.clone();
            }
        }
        let layout = wrap::layout(ui, text, current, options, &view, &geometry);
        self.entry = Some(CacheEntry {
            text: text.to_owned(),
            options,
            scale,
            layout: layout.clone(),
            signature,
        });
        if let Some(diff) = temporary {
            self.pending_left = Some(options.left);
            self.pending = Some(PendingComparison {
                other: other.to_owned(),
                view: TextView::new(&diff, options.filter, options.context, revealed),
                diff,
                ms: comparison_ms,
            });
        }
        layout
    }
}

#[cfg(test)]
pub fn visible_rows(diff: &DiffResult, mode: FilterMode, context: bool) -> Vec<usize> {
    diff.lines
        .iter()
        .enumerate()
        .filter(|(index, line)| match mode {
            FilterMode::All => true,
            FilterMode::Same => line.status == DiffStatus::Same,
            FilterMode::Differences => {
                line.status != DiffStatus::Same
                    || context && {
                        let start = index.saturating_sub(3);
                        let end = (*index + 4).min(diff.lines.len());
                        diff.lines[start..end]
                            .iter()
                            .any(|line| line.status != DiffStatus::Same)
                    }
            }
        })
        .map(|(index, _)| index)
        .collect()
}

pub fn visible_cursor(
    galley: &Galley,
    cursor: egui::epaint::text::cursor::Cursor,
    diff: &DiffResult,
    left: bool,
    visible: &[usize],
    backwards: bool,
) -> Option<egui::epaint::text::cursor::Cursor> {
    let number = |index: usize| {
        if left {
            diff.lines[index].left_line_no
        } else {
            diff.lines[index].right_line_no
        }
    };
    let current = cursor.pcursor.paragraph + 1;
    // The final empty paragraph is an editable EOF position.
    if !diff.lines.iter().any(|line| {
        (if left {
            line.left_line_no
        } else {
            line.right_line_no
        }) == Some(current)
    }) || visible.iter().any(|&index| number(index) == Some(current))
    {
        return None;
    }
    let numbers: Vec<_> = visible.iter().filter_map(|&index| number(index)).collect();
    let target = if backwards {
        numbers
            .iter()
            .rev()
            .find(|&&n| n < current)
            .or_else(|| numbers.first())
    } else {
        numbers
            .iter()
            .find(|&&n| n > current)
            .or_else(|| numbers.last())
    }?;
    Some(galley.from_pcursor(egui::epaint::text::cursor::PCursor {
        paragraph: target - 1,
        offset: cursor.pcursor.offset,
        prefer_next_row: true,
    }))
}

/// Keep the buffer byte-for-byte intact. Extra height on a real line represents
/// alignment gaps; no placeholder newline or character is ever inserted into text.
#[cfg(test)]
pub fn layout_job(
    text: &str,
    diff: &DiffResult,
    left: bool,
    _active: bool,
    visible: &[usize],
) -> (LayoutJob, usize) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    let mut mapped = vec![None; text.lines().count()];
    for (screen_row, &aligned_index) in visible.iter().enumerate() {
        let line = &diff.lines[aligned_index];
        if let Some(number) = if left {
            line.left_line_no
        } else {
            line.right_line_no
        } {
            if let Some(slot) = mapped.get_mut(number - 1) {
                *slot = Some((screen_row, aligned_index));
            }
        }
    }
    let leading = mapped
        .iter()
        .flatten()
        .next()
        .map(|(row, _)| *row)
        .unwrap_or(0);
    let mut next_row = visible.len();
    let mut heights = vec![0.0; mapped.len()];
    for (index, mapped) in mapped.iter().enumerate().rev() {
        if let Some((screen_row, _)) = mapped {
            heights[index] = (next_row - screen_row).max(1) as f32 * ROW_HEIGHT;
            next_row = *screen_row;
        }
    }
    for (index, raw_line) in text.split_inclusive('\n').enumerate() {
        let mapping = mapped.get(index).copied().flatten();
        let height = heights.get(index).copied().unwrap_or(ROW_HEIGHT);
        let format = TextFormat {
            font_id: editor_font(),
            line_height: Some(height),
            valign: Align::TOP,
            color: if mapping.is_none() {
                Color32::TRANSPARENT
            } else {
                Color32::BLACK
            },
            ..Default::default()
        };
        let content = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        if let Some((_, aligned_index)) = mapping {
            let line = &diff.lines[aligned_index];
            let chunks = if left {
                &line.left_inline
            } else {
                &line.right_inline
            };
            for chunk in chunks.iter() {
                let mut chunk_format = format.clone();
                chunk_format.color = match chunk.kind {
                    InlineChunkKind::Diff => Color32::RED,
                    InlineChunkKind::Unimportant => Color32::BLUE,
                    InlineChunkKind::Same => Color32::BLACK,
                };
                job.append(&chunk.text, 0.0, chunk_format);
            }
        } else {
            job.append(content, 0.0, format.clone());
        }
        if raw_line.ends_with('\n') {
            job.append("\n", 0.0, format);
        }
    }
    if text.ends_with('\n') {
        job.append(
            "",
            0.0,
            TextFormat {
                font_id: editor_font(),
                line_height: Some(ROW_HEIGHT),
                valign: Align::TOP,
                ..Default::default()
            },
        );
    }
    if text.is_empty() {
        job.first_row_min_height = ROW_HEIGHT;
    }
    debug_assert_eq!(
        job.text, text,
        "Editor layout must never synthesize document characters"
    );
    (job, leading)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_layout_keeps_cursor_metrics_and_edits_rows_in_place() {
        let text = (0..320)
            .map(|i| format!("linha {i} ação\t🙂\n"))
            .collect::<String>();
        let other = text.replace("linha 7 ", "linha sete ");
        let diff = DiffEngine::compare(&text, &other);
        let view = TextView::new(&diff, FilterMode::All, false, &[]);
        let ctx = egui::Context::default();
        super::super::theme::apply(&ctx);
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let options = LayoutOptions {
                    left: true,
                    active: true,
                    filter: FilterMode::All,
                    context: false,
                    revision: 1,
                    view_revision: 0,
                    paint_range: [0, 128],
                    unchanged: true,
                    rules: Default::default(),
                    wrap_widths: None,
                };
                let mut cache = EditorCache::default();
                let layout = cache.layout(ui, &text, &other, &diff, options, &[], Some(&view));
                assert_eq!(layout.galley.job.text, text);
                let chars = |galley: &Galley| {
                    galley
                        .rows
                        .iter()
                        .map(|row| row.char_count_including_newline())
                        .sum::<usize>()
                };
                assert_eq!(chars(&layout.galley), text.chars().count());
                assert!(layout.galley.rows.iter().all(|r| r.visuals.mesh.is_empty()));
                // Cursor metrics match the glyphs that paint_rows draws.
                let shaped = ui.fonts(|fonts| {
                    fonts.layout_job(LayoutJob::simple(
                        "linha 1 ação\t🙂".into(),
                        editor_font(),
                        Color32::BLACK,
                        f32::INFINITY,
                    ))
                });
                let metric: Vec<f32> = layout.galley.rows[1]
                    .glyphs
                    .iter()
                    .map(|g| g.pos.x)
                    .collect();
                let painted: Vec<f32> = shaped.rows[0].glyphs.iter().map(|g| g.pos.x).collect();
                assert_eq!(metric, painted);
                assert_eq!(layout.galley.rows[140].rect.top(), 140.0 * ROW_HEIGHT);
                // A scroll (other paint range) reuses the same galley.
                let scrolled = cache.layout(
                    ui,
                    &text,
                    &other,
                    &diff,
                    LayoutOptions {
                        paint_range: [2200, 2400],
                        ..options
                    },
                    &[],
                    Some(&view),
                );
                assert!(Arc::ptr_eq(&layout.galley, &scrolled.galley));
                drop((layout, scrolled));
                // Edits within a line, across lines and at both ends give the same
                // rows as a complete rebuild.
                let mut edited = text.clone();
                for (at, insert, remove) in [
                    (10, "x", 0),
                    (0, "início\n", 0),
                    (200, "", 30),
                    (500, "dois\nnovos\n", 3),
                ] {
                    let at = edited.char_indices().nth(at).unwrap().0;
                    let end = edited[at..]
                        .char_indices()
                        .nth(remove)
                        .map_or(edited.len(), |(i, _)| at + i);
                    edited.replace_range(at..end, insert);
                    let edit = LayoutOptions {
                        unchanged: false,
                        ..options
                    };
                    let incremental = cache.layout(ui, &edited, &other, &diff, edit, &[], None);
                    let fresh =
                        EditorCache::default().layout(ui, &edited, &other, &diff, edit, &[], None);
                    assert_eq!(incremental.galley.job.text, edited);
                    assert_eq!(incremental.leading_y, fresh.leading_y);
                    assert_eq!(incremental.galley.rect, fresh.galley.rect);
                    assert_eq!(incremental.galley.rows.len(), fresh.galley.rows.len());
                    for (a, b) in incremental.galley.rows.iter().zip(&fresh.galley.rows) {
                        assert_eq!(a.rect, b.rect);
                        assert_eq!(a.ends_with_newline, b.ends_with_newline);
                        assert_eq!(
                            a.glyphs
                                .iter()
                                .map(|g| (g.chr, g.pos.x))
                                .collect::<Vec<_>>(),
                            b.glyphs
                                .iter()
                                .map(|g| (g.chr, g.pos.x))
                                .collect::<Vec<_>>()
                        );
                    }
                }
            });
        });
    }

    #[test]
    fn editor_layout_preserves_unicode_newlines_and_alignment_gaps() {
        let left = "ação\nfim\n";
        let right = "ação\ninserido\nfim\n";
        let diff = DiffEngine::compare(left, right);
        let visible = visible_rows(&diff, FilterMode::All, false);
        let (left_job, leading) = layout_job(left, &diff, true, true, &visible);
        assert_eq!(left_job.text, left);
        assert_eq!(leading, 0);
        let ctx = egui::Context::default();
        super::super::theme::apply(&ctx);
        let _ = ctx.run(Default::default(), |ctx| {
            let galley = ctx.fonts(|fonts| fonts.layout_job(left_job.clone()));
            assert_eq!(galley.rows[1].rect.top(), 2.0 * ROW_HEIGHT);
        });
        let (right_job, _) = layout_job(right, &diff, false, true, &visible);
        assert_eq!(right_job.text, right);
        assert!(right_job
            .sections
            .iter()
            .any(|section| section.format.color == Color32::RED));
    }

    #[test]
    fn editor_layout_handles_leading_gaps_and_filtered_lines_without_changing_text() {
        for (left, right) in [
            ("fim", "novo\nfim"),
            ("", "novo\n"),
            ("a\n\nfim\n", "a\nnovo\n\nfim\n"),
        ] {
            let diff = DiffEngine::compare(left, right);
            for filter in [FilterMode::All, FilterMode::Differences, FilterMode::Same] {
                let visible = visible_rows(&diff, filter, false);
                let (job, _) = layout_job(left, &diff, true, true, &visible);
                assert_eq!(job.text, left);
            }
        }
        let diff = DiffEngine::compare("fim", "novo\nfim");
        assert_eq!(
            layout_job(
                "fim",
                &diff,
                true,
                true,
                &visible_rows(&diff, FilterMode::All, false)
            )
            .1,
            1
        );
    }
}
