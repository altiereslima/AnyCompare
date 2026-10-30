use super::text_view::TextView;
#[cfg(test)]
use crate::diff::DiffStatus;
use crate::diff::{DiffEngine, DiffResult, FilterMode, InlineChunkKind};
use egui::{text::LayoutJob, Align, Color32, FontId, Galley, Id, Rect, TextFormat, Ui};
use std::ops::Range;
use std::sync::Arc;

pub const ROW_HEIGHT: f32 = 16.0;
pub const FONT_SIZE: f32 = 12.4;
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
}

#[derive(Default)]
pub struct EditorCache {
    entry: Option<CacheEntry>,
    pub widget_id: Option<Id>,
    pub widget_rect: Option<Rect>,
    pending: Option<PendingComparison>,
}

struct PendingComparison {
    other: String,
    diff: DiffResult,
    ms: u128,
}

struct CacheEntry {
    text: String,
    options: LayoutOptions,
    scale: f32,
    layout: EditorLayout,
    signature: u64,
}

#[derive(Clone)]
pub struct EditorLayout {
    pub galley: Arc<Galley>,
    pub leading_y: f32,
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

/// All document characters remain addressable by the native widget. Only rows
/// near the viewport receive glyph meshes; offscreen rows retain cursor metrics.
fn virtual_layout(
    ui: &Ui,
    text: &str,
    diff: &DiffResult,
    options: LayoutOptions,
    view: &TextView,
) -> EditorLayout {
    use egui::epaint::{
        text::{Row, RowVisuals},
        Mesh,
    };
    use egui::{pos2, vec2};
    use std::collections::HashMap;
    let mut mapped = vec![None; text.lines().count()];
    for &index in &view.visible {
        let line = &diff.lines[index];
        if let Some(number) = if options.left {
            line.left_line_no
        } else {
            line.right_line_no
        } {
            if let Some(slot) = mapped.get_mut(number - 1) {
                *slot = Some((view.line_y[index].unwrap(), index));
            }
        }
    }
    let leading_y = mapped
        .iter()
        .flatten()
        .next()
        .map(|(y, _)| *y)
        .unwrap_or(view.height);
    let mut heights = vec![0.0; mapped.len()];
    let mut next_y = view.height;
    for (index, map) in mapped.iter().enumerate().rev() {
        if let Some((y, _)) = map {
            heights[index] = (next_y - y).max(ROW_HEIGHT);
            next_y = *y;
        }
    }
    let font = FontId::monospace(FONT_SIZE);
    let mut job = LayoutJob::simple(text.to_owned(), font.clone(), Color32::BLACK, f32::INFINITY);
    job.first_row_min_height = ROW_HEIGHT;
    let galley = ui.fonts(|fonts| {
        let sample = fonts.layout_job(LayoutJob::simple(
            "M".into(),
            font.clone(),
            Color32::BLACK,
            f32::INFINITY,
        ));
        let template = sample.rows[0].glyphs[0];
        let mut widths = [0.0; 128];
        for (index, width) in widths.iter_mut().enumerate().skip(1) {
            *width = fonts.glyph_width(&font, char::from(index as u8));
        }
        let mut unicode_widths = HashMap::new();
        let mut rows = Vec::with_capacity(mapped.len() + 1);
        let mut max_x = 0.0_f32;
        let mut mesh_bounds = Rect::NOTHING;
        let mut num_vertices = 0;
        let mut num_indices = 0;
        let mut previous_y = 0.0;
        let physical_count = text.bytes().filter(|&b| b == b'\n').count() + 1;
        for (physical, content) in text.split('\n').enumerate() {
            let mapping = mapped.get(physical).copied().flatten();
            let eof = physical >= mapped.len();
            let y = mapping.map(|(y, _)| y - leading_y).unwrap_or_else(|| {
                if eof {
                    (view.height - leading_y).max(0.0)
                } else {
                    previous_y
                }
            });
            let height = heights.get(physical).copied().unwrap_or(ROW_HEIGHT);
            let paint = mapping.is_some_and(|(y, _)| {
                y + ROW_HEIGHT >= options.paint_range[0] as f32
                    && y <= options.paint_range[1] as f32
            });
            let mut row = if paint {
                let (_, index) = mapping.unwrap();
                let line = &diff.lines[index];
                let chunks = if options.left {
                    &line.left_inline
                } else {
                    &line.right_inline
                };
                let mut line_job = LayoutJob::default();
                line_job.wrap.max_width = f32::INFINITY;
                for chunk in chunks.iter() {
                    line_job.append(
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
                if content.is_empty() {
                    line_job.first_row_min_height = ROW_HEIGHT;
                }
                debug_assert_eq!(line_job.text, content);
                let line_galley = fonts.layout_job(line_job);
                let mut row = line_galley.rows[0].clone();
                let shift = vec2(0.0, y - row.rect.top());
                row.rect = row.rect.translate(shift);
                row.visuals.mesh_bounds = row.visuals.mesh_bounds.translate(shift);
                for glyph in &mut row.glyphs {
                    glyph.pos += shift;
                    glyph.section_index = 0;
                }
                for vertex in &mut row.visuals.mesh.vertices {
                    vertex.pos += shift;
                }
                row
            } else {
                let mut x = 0.0;
                let glyphs = content
                    .chars()
                    .map(|chr| {
                        let width = if chr.is_ascii() {
                            widths[chr as usize]
                        } else {
                            *unicode_widths
                                .entry(chr)
                                .or_insert_with(|| fonts.glyph_width(&font, chr))
                        };
                        let mut glyph = template;
                        glyph.chr = chr;
                        glyph.pos = pos2(x, y + template.pos.y);
                        glyph.advance_width = width;
                        glyph.uv_rect = Default::default();
                        glyph.section_index = 0;
                        x = ((x + width) * fonts.pixels_per_point()).round()
                            / fonts.pixels_per_point();
                        glyph
                    })
                    .collect();
                let glyphs: Vec<egui::epaint::text::Glyph> = glyphs;
                let right = glyphs.last().map(|glyph| glyph.max_x()).unwrap_or(0.0);
                Row {
                    glyphs,
                    section_index_at_start: 0,
                    rect: Rect::from_min_size(pos2(0.0, y), vec2(right, height)),
                    visuals: RowVisuals {
                        mesh: Mesh::default(),
                        ..Default::default()
                    },
                    ends_with_newline: false,
                }
            };
            row.section_index_at_start = 0;
            row.rect.max.y = y + height;
            row.ends_with_newline = physical + 1 < physical_count;
            max_x = max_x.max(row.rect.right());
            previous_y = y;
            mesh_bounds = mesh_bounds.union(row.visuals.mesh_bounds);
            num_vertices += row.visuals.mesh.vertices.len();
            num_indices += row.visuals.mesh.indices.len();
            rows.push(row);
        }
        Arc::new(Galley {
            job: Arc::new(job),
            rows,
            elided: false,
            rect: Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(
                    max_x,
                    (view.height - leading_y + ROW_HEIGHT).max(ROW_HEIGHT),
                ),
            ),
            mesh_bounds,
            num_vertices,
            num_indices,
            pixels_per_point: fonts.pixels_per_point(),
        })
    });
    EditorLayout { galley, leading_y }
}

impl EditorCache {
    pub fn take_comparison(&mut self, left: &str, right: &str) -> Option<(DiffResult, u128)> {
        let entry = self.entry.as_ref()?;
        let pending = self.pending.take()?;
        let (own, other) = if entry.options.left {
            (left, right)
        } else {
            (right, left)
        };
        (entry.text == own && pending.other == other).then_some((pending.diff, pending.ms))
    }
    pub fn layout(
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
        let view = TextView::new(current, options.filter, options.context, revealed);
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
        let layout = virtual_layout(ui, text, current, options, &view);
        self.entry = Some(CacheEntry {
            text: text.to_owned(),
            options,
            scale,
            layout: layout.clone(),
            signature,
        });
        if let Some(diff) = temporary {
            self.pending = Some(PendingComparison {
                other: other.to_owned(),
                diff,
                ms: comparison_ms,
            });
        }
        layout
    }
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
    let current = cursor.rcursor.row + 1;
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
    Some(galley.from_rcursor(egui::epaint::text::cursor::RCursor {
        row: target - 1,
        column: cursor.rcursor.column,
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
            font_id: FontId::monospace(FONT_SIZE),
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
                font_id: FontId::monospace(FONT_SIZE),
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
    fn virtual_layout_preserves_full_cursor_metrics_but_only_meshes_visible_rows() {
        let text = (0..320)
            .map(|i| format!("linha {i} ação\t🙂\n"))
            .collect::<String>();
        let diff = DiffEngine::compare(&text, &text);
        let view = TextView::new(&diff, FilterMode::All, false, &[]);
        let ctx = egui::Context::default();
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
                };
                let layout = virtual_layout(ui, &text, &diff, options, &view);
                assert_eq!(layout.galley.job.text, text);
                assert_eq!(
                    layout
                        .galley
                        .rows
                        .iter()
                        .map(|row| row.char_count_including_newline())
                        .sum::<usize>(),
                    text.chars().count()
                );
                assert!(!layout.galley.rows[0].visuals.mesh.vertices.is_empty());
                assert!(layout.galley.rows[140].visuals.mesh.vertices.is_empty());
                let scrolled = virtual_layout(
                    ui,
                    &text,
                    &diff,
                    LayoutOptions {
                        paint_range: [2200, 2400],
                        ..options
                    },
                    &view,
                );
                assert!(scrolled.galley.rows[0].visuals.mesh.vertices.is_empty());
                assert!(!scrolled.galley.rows[140].visuals.mesh.vertices.is_empty());
                assert_eq!(layout.galley.rows[140].rect, scrolled.galley.rows[140].rect);
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
