use super::*;
use crate::diff::{DiffEngine, DiffStatus};

#[derive(Default)]
pub(super) struct Navigation {
    pub bookmarks: [Option<usize>; 10],
    edited_lines: Option<Vec<usize>>,
}

impl Navigation {
    pub fn invalidate_edits(&mut self) {
        self.edited_lines = None;
    }

    pub fn text_changed(&mut self, before: &str, after: &str) {
        self.invalidate_edits();
        if !self.bookmarks.iter().any(Option::is_some) {
            return;
        }
        let diff = DiffEngine::compare(before, after);
        for bookmark in self.bookmarks.iter_mut().flatten() {
            // Anchor to the real document line. A deleted line follows the
            // nearest remaining line, including the empty final paragraph.
            if let Some(index) = diff
                .lines
                .iter()
                .position(|l| l.left_line_no == Some(*bookmark))
            {
                *bookmark = diff.lines[index..]
                    .iter()
                    .find_map(|l| l.right_line_no)
                    .unwrap_or_else(|| line_count(after));
            } else {
                *bookmark = line_count(after);
            }
        }
    }

    fn edits(&mut self, before: &str, after: &str) -> &[usize] {
        self.edited_lines.get_or_insert_with(|| {
            if before == after {
                return Vec::new();
            }
            let diff = DiffEngine::compare(before, after);
            let mut following = line_count(after);
            let mut result = Vec::new();
            for row in diff.lines.iter().rev() {
                if let Some(line) = row.right_line_no {
                    following = line;
                }
                if row.status != DiffStatus::Same {
                    result.push(following);
                }
            }
            result.reverse();
            result.dedup();
            if result.is_empty() {
                // The line diff deliberately ignores the final terminator.
                // Adding or deleting it is still an edit to the last line.
                result.push(line_count(before).min(line_count(after)));
            }
            result
        })
    }
}

fn line_count(text: &str) -> usize {
    1 + text.bytes().filter(|&b| b == b'\n').count()
}

impl TextEditScreen {
    pub(super) fn cursor_line(&self, ctx: &Context) -> usize {
        let index = TextEdit::load_state(ctx, self.editor_id)
            .and_then(|state| state.cursor.char_range())
            .map_or(0, |r| r.primary.index);
        1 + self
            .document
            .text
            .chars()
            .take(index)
            .filter(|&c| c == '\n')
            .count()
    }

    pub(super) fn edited_targets(&mut self, ctx: &Context) -> [Option<usize>; 2] {
        let current = self.cursor_line(ctx);
        let edits = self
            .navigation
            .edits(self.document.saved_text(), &self.document.text);
        [
            edits.iter().copied().find(|&n| n > current),
            edits.iter().rev().copied().find(|&n| n < current),
        ]
    }

    pub(super) fn jump_to_line(&mut self, ctx: &Context, line: usize) {
        let offset = self
            .document
            .text
            .split_inclusive('\n')
            .take(line.saturating_sub(1))
            .map(str::len)
            .sum();
        self.select_bytes(ctx, offset..offset);
    }

    pub(super) fn draw_bookmarks(
        &self,
        ui: &Ui,
        gutter_x: f32,
        gutter_width: f32,
        output: &egui::text_edit::TextEditOutput,
    ) {
        for (number, line) in self.navigation.bookmarks.iter().enumerate() {
            let Some(line) = line else {
                continue;
            };
            let cursor = egui::epaint::text::cursor::PCursor {
                paragraph: line - 1,
                offset: 0,
                prefer_next_row: true,
            };
            let rect = output
                .galley
                .pos_from_pcursor(cursor)
                .translate(output.galley_pos.to_vec2());
            let center = pos2(gutter_x + gutter_width / 2.0, rect.center().y);
            let marker = Rect::from_center_size(center, vec2(14.0, 13.0));
            let painter = ui.painter();
            if !marker.intersects(painter.clip_rect()) {
                continue;
            }
            painter.rect_filled(marker, 1.0, Color32::from_rgb(205, 228, 249));
            painter.text(
                center,
                Align2::CENTER_CENTER,
                number.to_string(),
                FontId::proportional(10.0),
                Color32::BLACK,
            );
        }
    }
}
