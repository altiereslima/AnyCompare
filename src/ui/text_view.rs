use crate::diff::{DiffResult, DiffStatus, FilterMode};
use std::ops::Range;

pub const LINE_HEIGHT: f32 = 16.0;
pub const FOLD_HEIGHT: f32 = 9.0;

#[derive(Clone, Debug, PartialEq)]
pub enum RowKind {
    Line(usize),
    Fold { range: Range<usize>, expanded: bool },
}

#[derive(Clone, Debug)]
pub struct ViewRow {
    pub top: f32,
    pub kind: RowKind,
    height: f32,
}

impl ViewRow {
    pub fn height(&self) -> f32 {
        self.height
    }
}

pub struct TextView {
    pub rows: Vec<ViewRow>,
    pub visible: Vec<usize>,
    pub line_y: Vec<Option<f32>>,
    pub height: f32,
    pub line_heights: Vec<f32>,
}

impl TextView {
    pub fn new(
        diff: &DiffResult,
        mode: FilterMode,
        context: bool,
        revealed: &[Range<usize>],
    ) -> Self {
        let len = diff.lines.len();
        if mode == FilterMode::All {
            let mut rows = Vec::with_capacity(len);
            let mut visible = Vec::with_capacity(len);
            let mut line_y = Vec::with_capacity(len);
            for index in 0..len {
                let top = index as f32 * LINE_HEIGHT;
                rows.push(ViewRow {
                    top,
                    kind: RowKind::Line(index),
                    height: LINE_HEIGHT,
                });
                visible.push(index);
                line_y.push(Some(top));
            }
            return Self {
                rows,
                visible,
                line_y,
                height: len as f32 * LINE_HEIGHT,
                line_heights: vec![LINE_HEIGHT; len],
            };
        }
        let included: Vec<bool> = diff
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| match mode {
                FilterMode::All => true,
                FilterMode::Same => line.status == DiffStatus::Same,
                FilterMode::Differences => {
                    line.status != DiffStatus::Same
                        || context && {
                            diff.lines[index.saturating_sub(3)..(index + 4).min(len)]
                                .iter()
                                .any(|line| line.status != DiffStatus::Same)
                        }
                }
            })
            .collect();
        let mut view = Self {
            rows: Vec::with_capacity(len),
            visible: Vec::with_capacity(len),
            line_y: vec![None; len],
            height: 0.0,
            line_heights: vec![LINE_HEIGHT; len],
        };
        let mut index = 0;
        while index < included.len() {
            if included[index] {
                view.add_line(index);
                index += 1;
            } else {
                let start = index;
                while index < included.len() && !included[index] {
                    index += 1;
                }
                let range = start..index;
                let expanded = revealed.iter().any(|area| overlaps(area, &range));
                view.rows.push(ViewRow {
                    top: view.height,
                    height: FOLD_HEIGHT,
                    kind: RowKind::Fold {
                        range: range.clone(),
                        expanded,
                    },
                });
                view.height += FOLD_HEIGHT;
                if expanded {
                    for index in range {
                        view.add_line(index);
                    }
                }
            }
        }
        view
    }

    fn add_line(&mut self, index: usize) {
        self.rows.push(ViewRow {
            top: self.height,
            kind: RowKind::Line(index),
            height: LINE_HEIGHT,
        });
        self.visible.push(index);
        self.line_y[index] = Some(self.height);
        self.height += LINE_HEIGHT;
    }

    /// Both panes use the greater wrapped height of each aligned pair. These
    /// are display metrics only; the diff and physical documents stay intact.
    pub fn with_heights(mut self, heights: &[f32]) -> Self {
        self.height = 0.0;
        for row in &mut self.rows {
            row.top = self.height;
            if let RowKind::Line(index) = row.kind {
                row.height = heights[index].max(LINE_HEIGHT);
                self.line_heights[index] = row.height;
                self.line_y[index] = Some(self.height);
            }
            self.height += row.height;
        }
        self
    }
}

pub fn toggle(revealed: &mut Vec<Range<usize>>, range: Range<usize>) {
    if revealed.iter().any(|area| overlaps(area, &range)) {
        revealed.retain(|area| !overlaps(area, &range));
    } else {
        revealed.push(range);
    }
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::DiffEngine;

    #[test]
    fn filtered_runs_have_one_counted_marker_and_expand_without_changing_documents() {
        let diff = DiffEngine::compare("header\nname\nold\ntail\n", "header\nname\nnew\ntail\n");
        let mut revealed = Vec::new();
        let view = TextView::new(&diff, FilterMode::Differences, false, &revealed);
        assert_eq!(
            view.rows.iter().map(|row| &row.kind).collect::<Vec<_>>(),
            [
                &RowKind::Fold {
                    range: 0..2,
                    expanded: false
                },
                &RowKind::Line(2),
                &RowKind::Fold {
                    range: 3..4,
                    expanded: false
                }
            ]
        );
        assert_eq!(view.line_y[2], Some(FOLD_HEIGHT));
        toggle(&mut revealed, 0..2);
        let expanded = TextView::new(&diff, FilterMode::Differences, false, &revealed);
        assert_eq!(expanded.visible, vec![0, 1, 2]);
        assert_eq!(expanded.line_y[2], Some(FOLD_HEIGHT + 2.0 * LINE_HEIGHT));
        toggle(&mut revealed, 0..2);
        assert_eq!(
            TextView::new(&diff, FilterMode::Differences, false, &revealed).visible,
            vec![2]
        );
        assert_eq!(&*diff.lines[0].left_text, "header");
    }
}
