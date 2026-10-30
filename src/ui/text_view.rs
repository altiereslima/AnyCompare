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
}

impl ViewRow {
    pub fn height(&self) -> f32 {
        match self.kind {
            RowKind::Line(_) => LINE_HEIGHT,
            RowKind::Fold { .. } => FOLD_HEIGHT,
        }
    }
}

pub struct TextView {
    pub rows: Vec<ViewRow>,
    pub visible: Vec<usize>,
    pub line_y: Vec<Option<f32>>,
    pub height: f32,
}

impl TextView {
    pub fn new(
        diff: &DiffResult,
        mode: FilterMode,
        context: bool,
        revealed: &[Range<usize>],
    ) -> Self {
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
                            diff.lines[index.saturating_sub(3)..(index + 4).min(diff.lines.len())]
                                .iter()
                                .any(|line| line.status != DiffStatus::Same)
                        }
                }
            })
            .collect();
        let mut view = Self {
            rows: Vec::new(),
            visible: Vec::new(),
            line_y: vec![None; diff.lines.len()],
            height: 0.0,
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
        });
        self.visible.push(index);
        self.line_y[index] = Some(self.height);
        self.height += LINE_HEIGHT;
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
        assert_eq!(diff.lines[0].left_text, "header");
    }
}
