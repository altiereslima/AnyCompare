use super::*;
use std::collections::{BTreeMap, BTreeSet};

type Cell = (usize, usize);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CellGesture {
    Select,
    Extend,
    Toggle,
    Context,
    Edit,
    DragStart,
    DragExtend,
}

#[derive(Default)]
pub(super) struct Selection {
    anchor: Option<Cell>,
    // Coordinates use the current view order. Rebuilds invalidate this map.
    rectangle: Option<(Cell, Cell)>,
    toggled: BTreeSet<Cell>,
    pub(super) dragging: bool,
}

impl Selection {
    fn contains(&self, cell: Cell) -> bool {
        self.rectangle.is_some_and(|(a, b)| {
            (a.0.min(b.0)..=a.0.max(b.0)).contains(&cell.0)
                && (a.1.min(b.1)..=a.1.max(b.1)).contains(&cell.1)
        }) ^ self.toggled.contains(&cell)
    }

    pub(super) fn explicit(&self) -> bool {
        self.rectangle.is_some()
    }
}

impl TableCompareScreen {
    pub(super) fn selected_in_view(&self, left: bool, position: usize, column: usize) -> bool {
        left == self.active_left && self.selection.contains((position, column))
    }

    pub(super) fn select_cell(
        &mut self,
        left: bool,
        pair: usize,
        column: usize,
        gesture: CellGesture,
    ) {
        let position = self.visible.iter().position(|&index| index == pair);
        let previous = (left == self.active_left)
            .then(|| {
                self.visible
                    .iter()
                    .position(|&index| index == self.selected)
            })
            .flatten()
            .map(|position| (position, self.column));
        let preserve = gesture == CellGesture::Context
            && position.is_some_and(|position| self.selected_in_view(left, position, column));
        if left != self.active_left {
            self.selection = Selection::default();
        }
        self.active_left = left;
        self.selected = pair;
        self.column = column;
        if preserve {
            return;
        }
        if let Some(position) = position {
            let cell = (position, column);
            match gesture {
                CellGesture::Extend | CellGesture::DragExtend => {
                    let anchor = *self
                        .selection
                        .anchor
                        .get_or_insert(previous.unwrap_or(cell));
                    self.selection.rectangle = Some((anchor, cell));
                    self.selection.toggled.clear();
                }
                CellGesture::Toggle => {
                    let original = previous.unwrap_or(cell);
                    self.selection.rectangle.get_or_insert((original, original));
                    if previous.is_some() && !self.selection.toggled.remove(&cell) {
                        self.selection.toggled.insert(cell);
                    }
                    self.selection.anchor = Some(cell);
                }
                _ => {
                    self.selection = Selection {
                        anchor: Some(cell),
                        dragging: gesture == CellGesture::DragStart,
                        ..Selection::default()
                    };
                }
            }
        } else {
            self.selection = Selection::default();
        }
    }

    pub(super) fn select_all_visible(&mut self) {
        let layout = self.column_layout();
        if let (Some(first), Some(last), Some(last_row)) = (
            layout.first(),
            layout.last(),
            self.visible.len().checked_sub(1),
        ) {
            let anchor = (0, first.0);
            if !self.visible.contains(&self.selected) {
                self.selected = self.visible[0];
            }
            if !layout.iter().any(|&(column, _, _)| column == self.column) {
                self.column = first.0;
            }
            self.selection = Selection {
                anchor: Some(anchor),
                rectangle: Some((anchor, (last_row, last.0))),
                ..Selection::default()
            };
        }
    }

    pub(super) fn move_focus(&mut self, delta: isize, vertical: bool, extend: bool) {
        let columns;
        let axis = if vertical {
            &self.visible
        } else {
            columns = self
                .column_layout()
                .into_iter()
                .map(|(column, _, _)| column)
                .collect::<Vec<_>>();
            &columns
        };
        if axis.is_empty() || self.visible.is_empty() {
            return;
        }
        let current = if vertical { self.selected } else { self.column };
        let next = axis
            .iter()
            .position(|&value| value == current)
            .map_or(0, |position| {
                position.saturating_add_signed(delta).min(axis.len() - 1)
            });
        let (pair, column) = if vertical {
            (axis[next], self.column)
        } else {
            (self.selected, axis[next])
        };
        let pair = if self.visible.contains(&pair) {
            pair
        } else {
            self.visible[0]
        };
        self.select_cell(
            self.active_left,
            pair,
            column,
            if extend {
                CellGesture::Extend
            } else {
                CellGesture::Select
            },
        );
        self.jump = true;
    }

    pub(super) fn selected_cells(&self) -> impl Iterator<Item = Cell> + '_ {
        let single = (!self.selection.explicit()
            && self.visible.contains(&self.selected)
            && self.column_visible(self.column))
        .then_some((self.selected, self.column));
        let mut rows = 0..0;
        if let Some((a, b)) = self.selection.rectangle {
            let first = self.selection.toggled.first().map_or(a.0, |cell| cell.0);
            let last = self.selection.toggled.last().map_or(b.0, |cell| cell.0);
            rows = a.0.min(b.0).min(first)..(a.0.max(b.0).max(last) + 1).min(self.visible.len());
        }
        single.into_iter().chain(rows.flat_map(move |position| {
            (0..self.columns.len())
                .filter(move |&column| {
                    self.column_visible(column) && self.selection.contains((position, column))
                })
                .map(move |column| (self.visible[position], column))
        }))
    }

    pub(super) fn selected_pairs(&self) -> BTreeSet<usize> {
        self.selected_cells().map(|(pair, _)| pair).collect()
    }

    pub(super) fn cells_for_menu(
        &self,
        left: bool,
        cell: Option<Cell>,
    ) -> impl Iterator<Item = Cell> + '_ {
        let single = cell.filter(|&(pair, column)| {
            !self
                .visible
                .iter()
                .position(|&index| index == pair)
                .is_some_and(|position| self.selected_in_view(left, position, column))
        });
        let limit = if single.is_some() { 0 } else { usize::MAX };
        single.into_iter().chain(self.selected_cells().take(limit))
    }

    pub(super) fn commit_records(
        &mut self,
        left: bool,
        rows: BTreeMap<usize, Option<Vec<String>>>,
        insertions: BTreeMap<usize, Vec<Vec<String>>>,
        structural: bool,
        dialogs: &mut dyn DialogHost,
    ) {
        if rows.is_empty() && insertions.is_empty() {
            return;
        }
        let mut candidate = self.side(left).clone();
        if let Err(error) = candidate.apply_record_changes(rows, insertions) {
            dialogs.error(&error);
            return;
        }
        if candidate.document.as_ref().map(|doc| &doc.text)
            == self.side(left).document.as_ref().map(|doc| &doc.text)
        {
            return;
        }
        self.remember_edit();
        if structural {
            self.options.manual_rows.clear();
            self.align_pending = None;
        }
        *self.side_mut(left) = candidate;
        self.rebuild();
    }

    pub(super) fn modify_selection(
        &mut self,
        command: table_actions::CellCommand,
        dialogs: &mut dyn DialogHost,
    ) {
        use table_actions::CellCommand;
        let left = self.active_left;
        if self.read_only || !self.side(left).editable() || !self.commit_editor(dialogs) {
            return;
        }
        let table = &self.side(left).table;
        let mut rows = BTreeMap::new();
        let mut columns = BTreeSet::new();
        for (pair, column) in self.selected_cells() {
            let (Some(row), Some(column)) =
                (self.row_at(left, pair), self.physical_column(left, column))
            else {
                continue;
            };
            match command {
                CellCommand::Clear => {
                    let cells = rows
                        .entry(row)
                        .or_insert_with(|| Some(table.rows[row].cells.clone()))
                        .as_mut()
                        .unwrap();
                    if let Some(value) = cells.get_mut(column) {
                        value.clear();
                    }
                }
                CellCommand::DeleteRow => {
                    rows.insert(row, None);
                }
                CellCommand::DeleteColumn => {
                    columns.insert(column);
                }
                _ => return,
            }
        }
        if !columns.is_empty() {
            for (row, record) in table.rows.iter().enumerate() {
                let mut cells = record
                    .cells
                    .iter()
                    .enumerate()
                    .filter_map(|(column, value)| {
                        (!columns.contains(&column)).then_some(value.clone())
                    })
                    .collect::<Vec<_>>();
                if cells.is_empty() {
                    cells.push(String::new());
                }
                rows.insert(row, Some(cells));
            }
        }
        self.commit_records(
            left,
            rows,
            BTreeMap::new(),
            command == CellCommand::DeleteRow,
            dialogs,
        );
    }

    pub(super) fn clipboard_selection(&self) -> Result<Option<String>, String> {
        let left = self.active_left;
        if !self.selection.explicit() {
            return Ok(self.selected_cells().next().and_then(|(pair, column)| {
                let row = self.row_at(left, pair)?;
                self.physical_column(left, column)?;
                Some(
                    self.value(left, &self.side(left).table.rows[row], column)
                        .unwrap_or("")
                        .to_owned(),
                )
            }));
        }
        let mut rows = BTreeMap::new();
        let mut columns = BTreeSet::new();
        for (pair, column) in self.selected_cells() {
            if let Some(row) = self.row_at(left, pair) {
                if self.physical_column(left, column).is_some() {
                    columns.insert(column);
                    rows.entry(pair).or_insert_with(BTreeMap::new).insert(
                        column,
                        self.value(left, &self.side(left).table.rows[row], column)
                            .unwrap_or(""),
                    );
                }
            }
        }
        if rows.len() == 1 && columns.len() == 1 {
            return Ok(rows
                .values()
                .next()
                .and_then(|row| row.values().next())
                .map(|value| (*value).to_owned()));
        }
        let records = self
            .visible
            .iter()
            .filter_map(|pair| rows.get(pair))
            .map(|row| {
                let cells = columns
                    .iter()
                    .map(|column| row.get(column).copied().unwrap_or("").to_owned())
                    .collect::<Vec<_>>();
                table::serialize_record(&cells, b'\t')
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((!records.is_empty()).then(|| records.join("\r\n")))
    }

    pub(super) fn row_at(&self, left: bool, pair: usize) -> Option<usize> {
        let pair = self.pairs.get(pair)?;
        if left {
            pair.left
        } else {
            pair.right
        }
    }
}
