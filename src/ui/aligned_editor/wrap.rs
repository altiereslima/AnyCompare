//! Wrapped display geometry. Physical newlines and Unicode character offsets
//! remain those of the original buffer, including when filtered or aligned.
use super::*;

#[derive(Default)]
pub struct WrapCache {
    entry: Option<(u64, [f32; 2], f32, Arc<Geometry>)>,
}

pub struct Geometry {
    panes: [Vec<Arc<Galley>>; 2],
    pub heights: Vec<f32>,
}

impl WrapCache {
    pub fn clear(&mut self) {
        self.entry = None;
    }

    pub fn get(
        &mut self,
        ui: &Ui,
        diff: &DiffResult,
        revision: u64,
        widths: [f32; 2],
    ) -> Arc<Geometry> {
        let scale = ui.ctx().pixels_per_point();
        if let Some((r, w, s, geometry)) = &self.entry {
            if *r == revision && *w == widths && *s == scale {
                return geometry.clone();
            }
        }
        let panes = [true, false].map(|left| {
            diff.lines
                .iter()
                .map(|line| {
                    let chunks = if left {
                        &line.left_inline
                    } else {
                        &line.right_inline
                    };
                    let mut job = LayoutJob::default();
                    job.wrap.max_width = widths[usize::from(!left)].max(1.0);
                    job.first_row_min_height = ROW_HEIGHT;
                    for chunk in chunks.iter() {
                        job.append(
                            &chunk.text,
                            0.0,
                            TextFormat {
                                font_id: editor_font(),
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
                    ui.fonts(|fonts| fonts.layout_job(job))
                })
                .collect::<Vec<_>>()
        });
        let heights = panes[0]
            .iter()
            .zip(&panes[1])
            .map(|(a, b)| a.rows.len().max(b.rows.len()).max(1) as f32 * ROW_HEIGHT)
            .collect();
        let geometry = Arc::new(Geometry { panes, heights });
        self.entry = Some((revision, widths, scale, geometry.clone()));
        geometry
    }
}

pub(super) fn layout(
    ui: &Ui,
    text: &str,
    diff: &DiffResult,
    options: LayoutOptions,
    view: &TextView,
    geometry: &Geometry,
) -> EditorLayout {
    use egui::vec2;
    let side = usize::from(!options.left);
    let mut mapped = vec![None; text.split('\n').count()];
    for &index in &view.visible {
        let line = &diff.lines[index];
        if let Some(n) = if options.left {
            line.left_line_no
        } else {
            line.right_line_no
        } {
            if let Some(slot) = mapped.get_mut(n - 1) {
                *slot = Some(index);
            }
        }
    }
    let leading_y = mapped
        .iter()
        .flatten()
        .next()
        .and_then(|&i| view.line_y[i])
        .unwrap_or(view.height);
    let mut next_y = view.height;
    let mut heights = vec![0.0; mapped.len()];
    for (physical, map) in mapped.iter().enumerate().rev() {
        if let Some(index) = map {
            let y = view.line_y[*index].unwrap();
            heights[physical] = (next_y - y).max(view.line_heights[*index]);
            next_y = y;
        }
    }
    let mut rows = Vec::new();
    let mut mesh_bounds = Rect::NOTHING;
    let mut num_vertices = 0;
    let mut num_indices = 0;
    let mut max_x = 0.0_f32;
    let mut previous_y = 0.0;
    let physical_count = mapped.len();
    for (physical, content) in text.split('\n').enumerate() {
        let index = mapped[physical];
        let eof = physical + 1 == physical_count && content.is_empty();
        let y = index
            .and_then(|i| view.line_y[i])
            .map(|y| y - leading_y)
            .unwrap_or(if eof {
                (view.height - leading_y).max(0.0)
            } else {
                previous_y
            });
        let fallback;
        let source = if let Some(index) = index {
            &geometry.panes[side][index]
        } else {
            fallback = ui.fonts(|fonts| {
                fonts.layout_job(LayoutJob::simple(
                    content.to_owned(),
                    editor_font(),
                    Color32::BLACK,
                    f32::INFINITY,
                ))
            });
            &fallback
        };
        let visible = index.is_some() || eof;
        for (visual, source_row) in source.rows.iter().enumerate() {
            let mut row = source_row.clone();
            let top = y + if visible {
                visual as f32 * ROW_HEIGHT
            } else {
                0.0
            };
            let shift = vec2(0.0, top - row.rect.top());
            row.rect = row.rect.translate(shift);
            row.visuals.mesh_bounds = row.visuals.mesh_bounds.translate(shift);
            for glyph in &mut row.glyphs {
                glyph.pos += shift;
                glyph.section_index = 0;
            }
            for vertex in &mut row.visuals.mesh.vertices {
                vertex.pos += shift;
            }
            let last = visual + 1 == source.rows.len();
            row.rect.max.y = if !visible {
                top
            } else if last {
                y + heights[physical].max(source.rows.len() as f32 * ROW_HEIGHT)
            } else {
                top + ROW_HEIGHT
            };
            row.ends_with_newline = last && physical + 1 < physical_count;
            row.section_index_at_start = 0;
            let absolute_top = top + leading_y;
            if !visible
                || absolute_top + ROW_HEIGHT < options.paint_range[0] as f32
                || absolute_top > options.paint_range[1] as f32
            {
                row.visuals = Default::default();
            }
            if visible {
                max_x = max_x.max(row.rect.right());
            }
            mesh_bounds = mesh_bounds.union(row.visuals.mesh_bounds);
            num_vertices += row.visuals.mesh.vertices.len();
            num_indices += row.visuals.mesh.indices.len();
            rows.push(row);
        }
        previous_y = y;
    }
    let galley = Arc::new(Galley {
        job: Arc::new(LayoutJob::simple(
            text.to_owned(),
            editor_font(),
            Color32::BLACK,
            f32::INFINITY,
        )),
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
        pixels_per_point: ui.ctx().pixels_per_point(),
    });
    EditorLayout { galley, leading_y }
}
