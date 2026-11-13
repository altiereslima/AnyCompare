//! Cell commands shared by context menus and keyboard shortcuts.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CellCommand {
    Align,
    CopyRows,
    CopyCells,
    Cut,
    Copy,
    Paste,
    Clear,
    DeleteRow,
    DeleteColumn,
    InsertRow(bool),
    InsertColumn(bool),
    Modify,
    Open,
    OpenClipboard,
    Save,
    SaveAs,
    OpenDefault,
    Explore(bool),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dialogs::TestDialogs;
    struct Harness {
        ctx: Context,
        screen: TableCompareScreen,
        icons: IconManager,
        dialogs: TestDialogs,
        time: f64,
    }
    impl Harness {
        fn new() -> Self {
            let ctx = Context::default();
            crate::ui::theme::apply(&ctx);
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tables");
            let mut screen = TableCompareScreen::new();
            screen.options.header = true;
            screen.options.key = Some(0);
            screen
                .load_files(&root.join("left.csv"), &root.join("right.csv"))
                .unwrap();
            let mut h = Self {
                ctx,
                screen,
                icons: IconManager::new(),
                dialogs: TestDialogs::default(),
                time: 0.0,
            };
            h.frame(vec![]);
            h.frame(vec![]);
            h
        }
        fn frame(&mut self, events: Vec<Event>) -> FullOutput {
            self.time += 0.03;
            let modifiers = events
                .iter()
                .rev()
                .find_map(|event| match event {
                    Event::Key { modifiers, .. } | Event::PointerButton { modifiers, .. } => {
                        Some(*modifiers)
                    }
                    _ => None,
                })
                .unwrap_or_default();
            self.ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1150.0, 720.0))),
                    time: Some(self.time),
                    modifiers,
                    events,
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default().show(ctx, |ui| {
                        self.screen.ui(ctx, ui, &mut self.icons, &mut self.dialogs);
                    });
                },
            )
        }
        fn cell(&self, left: bool, pair: usize, column: usize) -> Rect {
            self.ctx
                .data(|d| d.get_temp::<Rect>(Id::new("table_test_cell").with((left, pair, column))))
                .unwrap()
        }
        fn click(&mut self, pos: Pos2, button: PointerButton) {
            self.modified_click(pos, button, Modifiers::NONE);
        }
        fn modified_click(&mut self, pos: Pos2, button: PointerButton, modifiers: Modifiers) {
            for pressed in [true, false] {
                self.frame(vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers,
                    },
                ]);
            }
        }
        fn key(&mut self, key: Key) {
            self.modified_key(key, Modifiers::NONE);
        }
        fn modified_key(&mut self, key: Key, modifiers: Modifiers) {
            self.frame(vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }]);
            self.frame(vec![Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            }]);
        }
    }
    #[test]
    fn extended_selection_copies_quoted_tsv_and_context_delete_undoes_as_one() {
        let mut h = Harness::new();
        let a = h.screen.visible[0];
        let b = h.screen.visible[1];
        let before = h.screen.snapshot();
        h.click(h.cell(true, a, 1).center(), PointerButton::Primary);
        h.modified_click(
            h.cell(true, b, 2).center(),
            PointerButton::Primary,
            Modifiers::SHIFT,
        );
        let expected = [(a, 1), (a, 2), (b, 1), (b, 2)];
        assert_eq!(h.screen.selected_cells().collect::<Vec<_>>(), expected);
        let copied = h.frame(vec![Event::Copy]).platform_output.copied_text;
        let records = table::parse(&copied, b'\t').unwrap();
        assert_eq!(records.rows.len(), 2);
        for (record, pair) in records.rows.iter().zip([a, b]) {
            let row = h.screen.row_at(true, pair).unwrap();
            assert_eq!(record.cells, h.screen.left.table.rows[row].cells[1..3]);
        }
        h.click(h.cell(true, a, 1).center(), PointerButton::Secondary);
        h.frame(vec![]);
        assert_eq!(h.screen.selected_cells().collect::<Vec<_>>(), expected);
        let (rect, enabled) = h
            .ctx
            .data(|d| d.get_temp::<(Rect, bool)>(Id::new("table_test_context").with("Excluir")))
            .unwrap();
        assert!(enabled);
        let cleared_rows = [a, b].map(|pair| h.screen.row_at(true, pair).unwrap());
        h.click(rect.center(), PointerButton::Primary);
        for row in cleared_rows {
            assert_eq!(h.screen.left.table.rows[row].cells[1..3], ["", ""]);
        }
        assert_eq!(h.screen.undo.len(), 1);
        h.screen.restore(false);
        assert_eq!(h.screen.snapshot(), before);
    }

    #[test]
    fn ctrl_click_disjoint_copy_changes_only_selected_cells() {
        let mut h = Harness::new();
        let first = h
            .screen
            .pairs
            .iter()
            .position(|pair| pair.left == Some(1))
            .unwrap();
        let second = h
            .screen
            .pairs
            .iter()
            .position(|pair| pair.left == Some(3))
            .unwrap();
        let before = h.screen.snapshot();
        let mut expected = h
            .screen
            .right
            .table
            .rows
            .iter()
            .map(|row| row.cells.clone())
            .collect::<Vec<_>>();
        for (pair, column) in [(first, 2), (second, 1)] {
            let target = h.screen.row_at(false, pair).unwrap();
            let source = h.screen.row_at(true, pair).unwrap();
            expected[target][column] = h.screen.left.table.rows[source].cells[column].clone();
        }
        h.click(h.cell(true, first, 2).center(), PointerButton::Primary);
        h.modified_click(
            h.cell(true, second, 1).center(),
            PointerButton::Primary,
            Modifiers::CTRL,
        );
        assert_eq!(h.screen.selected_cells().count(), 2);
        h.modified_key(Key::R, Modifiers::CTRL | Modifiers::SHIFT);
        let actual = h
            .screen
            .right
            .table
            .rows
            .iter()
            .map(|row| row.cells.clone())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        assert_eq!(h.screen.undo.len(), 1);
        h.screen.restore(false);
        assert_eq!(h.screen.snapshot(), before);
    }

    #[test]
    fn copy_selected_rows_updates_matches_and_inserts_orphans_in_one_undo() {
        let mut h = Harness::new();
        let before = h.screen.snapshot();
        let header = h.screen.right.table.rows[0].cells.clone();
        h.modified_key(Key::A, Modifiers::CTRL);
        h.modified_key(Key::R, Modifiers::CTRL);
        assert_eq!(h.screen.right.table.rows[0].cells, header);
        assert_eq!(h.screen.right.table.rows.len(), 6);
        assert!(h
            .screen
            .right
            .table
            .rows
            .iter()
            .any(|row| row.cells[0] == "only-right"));
        for row in &h.screen.left.table.rows[1..] {
            assert!(h
                .screen
                .right
                .table
                .rows
                .iter()
                .any(|target| target.cells == row.cells));
        }
        assert_eq!(h.screen.undo.len(), 1);
        h.screen.restore(false);
        assert_eq!(h.screen.snapshot(), before);
    }

    #[test]
    fn pointer_drag_and_shift_arrows_select_cells_and_ctrl_a_respects_filter() {
        let mut h = Harness::new();
        let a = h.screen.visible[0];
        let b = h.screen.visible[1];
        let from = h.cell(true, a, 1).center();
        let to = h.cell(true, b, 2).center();
        h.frame(vec![
            Event::PointerMoved(from),
            Event::PointerButton {
                pos: from,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ]);
        h.frame(vec![Event::PointerMoved(to)]);
        h.frame(vec![Event::PointerButton {
            pos: to,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }]);
        assert_eq!(
            h.screen.selected_cells().collect::<Vec<_>>(),
            [(a, 1), (a, 2), (b, 1), (b, 2)]
        );
        h.click(from, PointerButton::Primary);
        h.modified_key(Key::ArrowDown, Modifiers::SHIFT);
        assert_eq!(
            h.screen.selected_cells().collect::<Vec<_>>(),
            [(a, 1), (b, 1)]
        );
        h.screen.set_filter(Filter::Differences);
        h.screen.options.hide_same_columns = true;
        h.modified_key(Key::A, Modifiers::CTRL);
        let selected = h.screen.selected_cells().collect::<Vec<_>>();
        assert_eq!(
            selected.len(),
            h.screen.visible.len() * h.screen.column_layout().len()
        );
        assert!(selected
            .iter()
            .all(|(pair, column)| h.screen.visible.contains(pair)
                && h.screen.column_visible(*column)));
    }
    #[test]
    fn drag_scrolls_both_axes_keeps_its_pane_and_stops_on_release() {
        let mut h = Harness::new();
        let mut text = String::from("ID,B,C,D,E,F\n");
        for row in 0..45 {
            text.push_str(&format!("{row:03},b,c,d,e,f\n"));
        }
        for side in [&mut h.screen.left, &mut h.screen.right] {
            side.document.as_mut().unwrap().text = text.clone();
            side.reparse().unwrap();
        }
        h.screen.options.widths = vec![220.0; 6];
        h.screen.rebuild();
        h.frame(vec![]);
        let grid = h
            .ctx
            .data(|d| d.get_temp::<Rect>(Id::new("table_test_grid")))
            .unwrap();
        let data = table_grid::data_rect(table_grid::pane_rects(grid)[0]);
        let first = h.screen.visible[0];
        let last = *h.screen.visible.last().unwrap();
        let outside_end = data.max + vec2(100.0, 45.0);
        let outside_start = data.min - vec2(100.0, 45.0);
        let press = |h: &mut Harness, pos, pressed| {
            h.frame(vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ]);
        };

        let start = h.cell(true, first, 0).center();
        press(&mut h, start, true);
        h.frame(vec![Event::PointerMoved(outside_end)]);
        for _ in 0..48 {
            h.frame(vec![]);
        }
        assert!(h.screen.scroll_y > 0.0);
        assert!(h.screen.scroll_x[0] > 0.0);
        assert_eq!(h.screen.scroll_x[1], 0.0);
        assert!(h.screen.active_left);
        assert_eq!((h.screen.selected, h.screen.column), (last, 5));
        assert_eq!(h.screen.selected_cells().count(), 45 * 6);
        press(&mut h, outside_end, false);
        let end_offsets = (h.screen.scroll_x, h.screen.scroll_y);
        for _ in 0..3 {
            h.frame(vec![Event::PointerMoved(outside_start)]);
        }
        assert!(!h.screen.selection.dragging);
        assert_eq!((h.screen.scroll_x, h.screen.scroll_y), end_offsets);
        assert_eq!((h.screen.selected, h.screen.column), (last, 5));

        let end = h.cell(true, last, 5).center();
        press(&mut h, end, true);
        h.frame(vec![Event::PointerMoved(outside_start)]);
        for _ in 0..48 {
            h.frame(vec![]);
        }
        assert_eq!((h.screen.scroll_x, h.screen.scroll_y), ([0.0; 2], 0.0));
        assert_eq!((h.screen.selected, h.screen.column), (first, 0));
        assert_eq!(h.screen.selected_cells().count(), 45 * 6);
        press(&mut h, outside_start, false);
    }

    #[test]
    fn double_click_edits_in_place_selects_all_and_enter_commits_without_viewport_jump() {
        let mut h = Harness::new();
        let pos = h.cell(true, 1, 1).center();
        let details = h
            .ctx
            .data(|d| d.get_temp::<Rect>(Id::new("table_test_details")))
            .unwrap();
        h.click(pos, PointerButton::Primary);
        h.click(pos, PointerButton::Primary);
        h.frame(vec![]);
        let edited_row = h.screen.editor.as_ref().unwrap().row;
        assert!(
            h.screen.editor.is_some(),
            "double click must enter edit mode"
        );
        let editor = h
            .ctx
            .data(|d| d.get_temp::<Rect>(Id::new("table_test_editor")))
            .unwrap();
        assert!(editor.contains(pos));
        assert_eq!(h.screen.scroll_y, 0.0);
        assert_eq!(
            h.ctx
                .data(|d| d.get_temp::<Rect>(Id::new("table_test_details")))
                .unwrap(),
            details
        );
        h.frame(vec![Event::Text("nova tradução ç漢字".into())]);
        assert_eq!(
            h.screen.editor.as_ref().unwrap().value,
            "nova tradução ç漢字",
            "typing replaces the selected entire value"
        );
        h.key(Key::Enter);
        assert!(h.screen.editor.is_none());
        assert_eq!(
            h.screen.left.table.rows[edited_row].cells[1],
            "nova tradução ç漢字"
        );
        assert!(h.screen.left.dirty());
        assert!(!h.screen.right.dirty());
        h.screen.restore(false);
        assert!(!h.screen.left.dirty());
        assert!(h.dialogs.errors.is_empty());
    }
    #[test]
    fn escape_cancels_cell_edit_and_read_only_blocks_both_edit_entry_and_mutation() {
        let mut h = Harness::new();
        let before = h.screen.snapshot();
        let pos = h.cell(false, 0, 1).center();
        h.click(pos, PointerButton::Primary);
        h.click(pos, PointerButton::Primary);
        h.frame(vec![]);
        h.frame(vec![Event::Text("cancel this".into())]);
        h.key(Key::Escape);
        assert_eq!(h.screen.snapshot(), before);
        assert!(!h.screen.dirty());
        h.screen.read_only = true;
        h.screen.begin_edit(&mut h.dialogs);
        assert!(h.screen.editor.is_none());
        h.screen
            .run_cell_command(&h.ctx, CellCommand::DeleteRow, &mut h.dialogs);
        h.screen.paste_cells("do not paste", &mut h.dialogs);
        assert_eq!(h.screen.snapshot(), before);
    }
    #[test]
    fn context_menu_targets_right_clicked_cell_and_delete_is_undoable() {
        let mut h = Harness::new();
        let before = h.screen.snapshot();
        let pos = h.cell(false, 1, 1).center();
        h.click(pos, PointerButton::Secondary);
        h.frame(vec![]);
        assert_eq!(
            (h.screen.active_left, h.screen.selected, h.screen.column),
            (false, 1, 1)
        );
        assert!(h.ctx.is_context_menu_open());
        let edited_row = h.screen.selected_row(false).unwrap();
        let (rect, enabled) = h
            .ctx
            .data(|d| d.get_temp::<(Rect, bool)>(Id::new("table_test_context").with("Excluir")))
            .unwrap();
        assert!(enabled);
        h.click(rect.center(), PointerButton::Primary);
        h.frame(vec![]);
        assert_eq!(h.screen.right.table.rows[edited_row].cells[1], "");
        assert_eq!(
            h.screen.left.document.as_ref().unwrap().text,
            before[0].as_ref().unwrap().as_str()
        );
        h.screen.restore(false);
        assert_eq!(h.screen.snapshot(), before);
    }
    #[test]
    fn align_with_marks_one_row_then_clicks_the_other_side_without_changing_either_document() {
        let mut h = Harness::new();
        let before = h.screen.snapshot();
        h.screen.selected = 0;
        h.screen.active_left = true;
        let origin = h.screen.selected_row(true).unwrap();
        h.screen
            .run_cell_command(&h.ctx, CellCommand::Align, &mut h.dialogs);
        assert_eq!(h.screen.align_pending, Some((true, origin)));
        let target = h
            .screen
            .pairs
            .iter()
            .enumerate()
            .find(|(i, p)| *i != 0 && p.right.is_some())
            .map(|(i, p)| (i, p.right.unwrap()))
            .unwrap();
        let pos = h.cell(false, target.0, 1).center();
        h.click(pos, PointerButton::Primary);
        assert!(h.screen.align_pending.is_none());
        assert!(h
            .screen
            .pairs
            .iter()
            .any(|p| p.left == Some(origin) && p.right == Some(target.1)));
        assert_eq!(h.screen.options.manual_rows, vec![[origin, target.1]]);
        let seen_left: std::collections::BTreeSet<_> =
            h.screen.pairs.iter().filter_map(|p| p.left).collect();
        let seen_right: std::collections::BTreeSet<_> =
            h.screen.pairs.iter().filter_map(|p| p.right).collect();
        assert_eq!(seen_left.len(), h.screen.left.table.rows.len() - 1);
        assert_eq!(seen_right.len(), h.screen.right.table.rows.len() - 1);
        assert_eq!(h.screen.snapshot(), before);
        assert!(!h.screen.dirty());
    }
    #[test]
    fn structural_edits_save_utf16_and_record_terminators_without_touching_other_rows() {
        let path = std::env::temp_dir().join(format!(
            "anycompare-table-encoding-{}-{}.csv",
            std::process::id(),
            crate::history::now()
        ));
        let source = "ID;Texto\r\na;\"ação\nsegunda linha\"\r\nb;inalterado\r\n";
        let mut bytes = vec![0xff, 0xfe];
        for word in source.encode_utf16() {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        std::fs::write(&path, &bytes).unwrap();
        let mut side = Side::load(&path, Some(b';'), 0).unwrap();
        side.edit_cell(1, 1, "nova ç漢字\nlinha").unwrap();
        side.document.as_mut().unwrap().save(&path).unwrap();
        let saved = std::fs::read(&path).unwrap();
        assert!(saved.starts_with(&[0xff, 0xfe]));
        let reopened = Side::load(&path, Some(b';'), 0).unwrap();
        assert_eq!(reopened.table.rows[1].cells[1], "nova ç漢字\nlinha");
        assert_eq!(reopened.table.rows[2].cells, vec!["b", "inalterado"]);
        assert_eq!(
            reopened.document.as_ref().unwrap().encoding,
            Encoding::Utf16Le
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn structural_commands_and_tabular_paste_preserve_quotes_newlines_and_undo() {
        let mut h = Harness::new();
        let before = h.screen.snapshot();
        h.screen.column = 1;
        let row = h.screen.selected_row(true).unwrap();
        h.screen
            .paste_cells("\"first\nsecond\"\tç\nnext\t漢字", &mut h.dialogs);
        assert_eq!(
            &h.screen.left.table.rows[row].cells[1..3],
            ["first\nsecond", "ç"]
        );
        assert_eq!(
            &h.screen.left.table.rows[row + 1].cells[1..3],
            ["next", "漢字"]
        );
        h.screen.restore(false);
        assert_eq!(h.screen.snapshot(), before);
        for command in [
            CellCommand::InsertRow(true),
            CellCommand::InsertRow(false),
            CellCommand::DeleteRow,
            CellCommand::InsertColumn(true),
            CellCommand::InsertColumn(false),
            CellCommand::DeleteColumn,
        ] {
            h.screen.selected = 0;
            h.screen.column = 1;
            h.screen.run_cell_command(&h.ctx, command, &mut h.dialogs);
            assert_ne!(h.screen.snapshot(), before, "{command:?}");
            h.screen.restore(false);
            assert_eq!(h.screen.snapshot(), before);
        }
        assert!(h.dialogs.errors.is_empty());
    }
}
#[derive(Clone)]
pub(super) struct CellRequest {
    pub left: bool,
    pub pair: usize,
    pub column: usize,
    pub command: CellCommand,
}
pub(super) fn request_id() -> Id {
    Id::new("table_cell_command")
}
pub(super) fn editor_rect_id() -> Id {
    Id::new("table_inline_editor_rect")
}
fn clipboard() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}
pub(super) fn prepare_context(ctx: &Context) {
    let available = clipboard().is_some_and(|s| !s.is_empty());
    ctx.data_mut(|d| d.insert_temp(Id::new("table_clipboard_available"), available));
}

impl TableCompareScreen {
    pub(super) fn align_rows(&mut self, anchor: [usize; 2], dialogs: &mut dyn DialogHost) {
        let mut anchors = self.options.manual_rows.clone();
        anchors.retain(|a| a[0] != anchor[0] && a[1] != anchor[1]);
        anchors.push(anchor);
        anchors.sort_unstable();
        if anchors.windows(2).any(|w| w[0][1] >= w[1][1]) {
            dialogs.error("O alinhamento cruza outro par marcado. Remova o alinhamento anterior antes de continuar.");
            return;
        }
        self.options.manual_rows = anchors;
        self.options.sort = None;
        self.align_pending = None;
        self.rebuild();
        if let Some(index) = self
            .pairs
            .iter()
            .position(|p| p.left == Some(anchor[0]) && p.right == Some(anchor[1]))
        {
            self.selected = index;
            self.jump = true;
        }
    }
    pub(super) fn cell_shortcuts(&mut self, ctx: &Context, dialogs: &mut dyn DialogHost) {
        if self.editor.is_some() || ctx.memory(|m| m.any_popup_open()) || ctx.is_context_menu_open()
        {
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.align_pending = None;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::A)) {
            self.select_all_visible();
        }
        let mut opened_clipboard = false;
        for (modifiers, key, command) in [
            (Modifiers::NONE, Key::F7, CellCommand::Align),
            (Modifiers::NONE, Key::Delete, CellCommand::Clear),
            (Modifiers::CTRL, Key::Enter, CellCommand::InsertRow(false)),
            (
                Modifiers::CTRL | Modifiers::SHIFT,
                Key::Enter,
                CellCommand::InsertRow(true),
            ),
            (Modifiers::NONE, Key::F12, CellCommand::SaveAs),
            (
                Modifiers::CTRL | Modifiers::SHIFT,
                Key::V,
                CellCommand::OpenClipboard,
            ),
        ] {
            if ctx.input_mut(|i| i.consume_key(modifiers, key)) {
                opened_clipboard |= command == CellCommand::OpenClipboard;
                self.run_cell_command(ctx, command, dialogs);
            }
        }
        for (key, left) in [(Key::R, true), (Key::L, false)] {
            for (modifiers, command) in [
                (Modifiers::CTRL | Modifiers::SHIFT, CellCommand::CopyCells),
                (Modifiers::CTRL, CellCommand::CopyRows),
            ] {
                if ctx.input_mut(|i| i.consume_key(modifiers, key)) {
                    self.active_left = left;
                    self.run_cell_command(ctx, command, dialogs);
                }
            }
        }
        let events = ctx.input_mut(|i| {
            let mut events = Vec::new();
            i.events.retain(|e| {
                if matches!(e, Event::Copy | Event::Cut | Event::Paste(_)) {
                    events.push(e.clone());
                    false
                } else {
                    true
                }
            });
            events
        });
        for event in events {
            match event {
                Event::Copy => self.run_cell_command(ctx, CellCommand::Copy, dialogs),
                Event::Cut => self.run_cell_command(ctx, CellCommand::Cut, dialogs),
                Event::Paste(value) if !opened_clipboard => self.paste_cells(&value, dialogs),
                _ => {}
            }
        }
    }
    fn editable_cell(&self, left: bool, pair: usize, column: usize) -> bool {
        !self.read_only
            && self.side(left).editable()
            && self
                .pairs
                .get(pair)
                .and_then(|p| if left { p.left } else { p.right })
                .is_some()
            && self.physical_column(left, column).is_some()
    }
    pub(super) fn cell_context(
        &self,
        ui: &mut Ui,
        icons: &mut IconManager,
        left: bool,
        pair: usize,
        column: usize,
    ) {
        // Measured native cell menu: 323 px outer width, 20 px action rows.
        // Reserve shortcut space explicitly, rather than letting the long
        // translated Copy Cells caption collide with Ctrl+Shift+R/L.
        ui.set_min_width(309.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.spacing_mut().button_padding = vec2(4.0, 1.0);
        ui.spacing_mut().interact_size.y = 20.0;
        let present = self
            .pairs
            .get(pair)
            .and_then(|p| if left { p.left } else { p.right })
            .is_some();
        let editable = self.editable_cell(left, pair, column);
        let selected_present =
            self.cells_for_menu(left, Some((pair, column)))
                .any(|(pair, column)| {
                    self.row_at(left, pair).is_some()
                        && self.physical_column(left, column).is_some()
                });
        let selection_editable = selected_present && !self.read_only && self.side(left).editable();
        let file = !self.side(left).path.is_empty();
        let mut item = |ui: &mut Ui, label: &str, enabled: bool, command| {
            let response = chrome::menu_item(ui, icons, label, enabled);
            #[cfg(test)]
            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    Id::new("table_test_context").with(label),
                    (response.rect, enabled),
                )
            });
            if response.clicked() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        request_id(),
                        CellRequest {
                            left,
                            pair,
                            column,
                            command,
                        },
                    )
                });
                ui.close_menu();
            }
        };
        item(ui, "Alinhar Com...  F7", present, CellCommand::Align);
        item(
            ui,
            if left {
                "Copiar para Direita  Ctrl+R"
            } else {
                "Copiar para Esquerda  Ctrl+L"
            },
            self.can_copy_rows_from(left, Some((pair, column))),
            CellCommand::CopyRows,
        );
        item(
            ui,
            if left {
                "Copiar células para a direita  Ctrl+Shift+R"
            } else {
                "Copiar células para a esquerda  Ctrl+Shift+L"
            },
            self.can_copy_cells_from(left, Some((pair, column))),
            CellCommand::CopyCells,
        );
        ui.separator();
        item(ui, "Recortar  Ctrl+X", selection_editable, CellCommand::Cut);
        item(ui, "Copiar  Ctrl+C", selected_present, CellCommand::Copy);
        item(
            ui,
            "Colar  Ctrl+V",
            editable
                && ui
                    .ctx()
                    .data(|d| d.get_temp::<bool>(Id::new("table_clipboard_available")))
                    .unwrap_or(false),
            CellCommand::Paste,
        );
        item(ui, "Excluir", selection_editable, CellCommand::Clear);
        item(
            ui,
            "Excluir Linha",
            selection_editable,
            CellCommand::DeleteRow,
        );
        item(
            ui,
            "Excluir Coluna",
            selection_editable,
            CellCommand::DeleteColumn,
        );
        item(
            ui,
            "Inserir Linha Antes  Ctrl+Shift+Enter",
            editable,
            CellCommand::InsertRow(true),
        );
        item(
            ui,
            "Inserir Linha Após  Ctrl+Enter",
            editable,
            CellCommand::InsertRow(false),
        );
        item(
            ui,
            "Inserir Coluna Antes",
            editable,
            CellCommand::InsertColumn(true),
        );
        item(
            ui,
            "Inserir Coluna Após",
            editable,
            CellCommand::InsertColumn(false),
        );
        item(ui, "Modificar...  F2", editable, CellCommand::Modify);
        item(ui, "Classificar...", false, CellCommand::Modify);
        ui.separator();
        item(ui, "Abrir Arquivo...  Ctrl+O", true, CellCommand::Open);
        item(
            ui,
            "Abrir Área de Transferência  Ctrl+Shift+V",
            ui.ctx()
                .data(|d| d.get_temp::<bool>(Id::new("table_clipboard_available")))
                .unwrap_or(false),
            CellCommand::OpenClipboard,
        );
        ui.menu_button(tr("Abrir Com"), |ui| {
            item(ui, "Aplicativo padrão", file, CellCommand::OpenDefault)
        });
        item(
            ui,
            "Salvar Arquivo  Ctrl+S",
            !self.read_only
                && self.side(left).editable()
                && (self.side(left).dirty() || self.editor_changed()),
            CellCommand::Save,
        );
        item(
            ui,
            "Salvar Arquivo Como...  F12",
            !self.read_only && self.side(left).editable(),
            CellCommand::SaveAs,
        );
        ui.separator();
        ui.menu_button(tr("Explorer"), |ui| {
            item(
                ui,
                "Selecionar arquivo no Explorador",
                file,
                CellCommand::Explore(true),
            );
            item(
                ui,
                "Abrir pasta no Explorador",
                file,
                CellCommand::Explore(false),
            );
        });
    }
    pub(super) fn run_cell_command(
        &mut self,
        ctx: &Context,
        command: CellCommand,
        dialogs: &mut dyn DialogHost,
    ) {
        match command {
            CellCommand::Align => {
                if self.commit_editor(dialogs) {
                    self.align_pending = self
                        .selected_row(self.active_left)
                        .map(|row| (self.active_left, row));
                }
            }
            CellCommand::Modify => self.begin_edit(dialogs),
            CellCommand::CopyRows => self.copy_row(dialogs),
            CellCommand::CopyCells => self.copy_cell(dialogs),
            CellCommand::Open => self.pick_file(self.active_left, dialogs),
            CellCommand::Save => {
                self.save_side(self.active_left, false, dialogs);
            }
            CellCommand::SaveAs => {
                self.save_side(self.active_left, true, dialogs);
            }
            CellCommand::OpenDefault | CellCommand::Explore(_) => {
                let path = PathBuf::from(&self.side(self.active_left).path);
                let select = matches!(command, CellCommand::Explore(true));
                let path = if matches!(command, CellCommand::Explore(false)) {
                    path.parent().unwrap_or(&path).to_path_buf()
                } else {
                    path
                };
                if let Err(error) = dialogs.open_path(&path, select) {
                    dialogs.error(&error);
                }
            }
            CellCommand::OpenClipboard => {
                if let Some(value) = clipboard() {
                    if self.commit_editor(dialogs) && self.confirm_side(self.active_left, dialogs) {
                        match table::Side::from_text(value) {
                            Ok(side) => {
                                *self.side_mut(self.active_left) = side;
                                self.paths[usize::from(!self.active_left)].clear();
                                self.undo.clear();
                                self.redo.clear();
                                self.rebuild();
                            }
                            Err(error) => dialogs.error(&error),
                        }
                    }
                }
            }
            CellCommand::Copy | CellCommand::Cut => {
                if (command == CellCommand::Cut
                    && (self.read_only || !self.side(self.active_left).editable()))
                    || !self.commit_editor(dialogs)
                {
                    return;
                }
                match self.clipboard_selection() {
                    Ok(Some(value)) => {
                        ctx.copy_text(value);
                        if command == CellCommand::Cut {
                            self.modify_selected(CellCommand::Clear, dialogs);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => dialogs.error(&error),
                }
            }
            CellCommand::Paste => {
                if let Some(value) = clipboard() {
                    self.paste_cells(&value, dialogs);
                }
            }
            _ => self.modify_selected(command, dialogs),
        }
        ctx.request_repaint();
    }
    fn modify_selected(&mut self, command: CellCommand, dialogs: &mut dyn DialogHost) {
        if matches!(
            command,
            CellCommand::Clear | CellCommand::DeleteRow | CellCommand::DeleteColumn
        ) {
            self.modify_selection(command, dialogs);
            return;
        }
        let left = self.active_left;
        if !self.editable_cell(left, self.selected, self.column) || !self.commit_editor(dialogs) {
            return;
        }
        let row = self.selected_row(left).unwrap();
        let column = self.physical_column(left, self.column).unwrap();
        let mut candidate = self.side(left).clone();
        let result = match command {
            CellCommand::InsertColumn(before) => {
                candidate.change_column(column + usize::from(!before), true)
            }
            CellCommand::InsertRow(before) => {
                let position = row + usize::from(!before);
                let before = (position < candidate.table.rows.len()).then_some(position);
                let cells = vec![String::new(); candidate.table.columns.max(1)];
                candidate.insert_row(before, &cells)
            }
            _ => return,
        };
        match result {
            Ok(()) => {
                self.remember_edit();
                if matches!(command, CellCommand::InsertRow(_)) {
                    self.options.manual_rows.clear();
                    self.align_pending = None;
                }
                *self.side_mut(left) = candidate;
                self.rebuild();
            }
            Err(error) => dialogs.error(&error),
        }
    }
    fn paste_cells(&mut self, value: &str, dialogs: &mut dyn DialogHost) {
        let left = self.active_left;
        if !self.editable_cell(left, self.selected, self.column) || !self.commit_editor(dialogs) {
            return;
        }
        let row = self.selected_row(left).unwrap();
        let column = self.physical_column(left, self.column).unwrap();
        let rows = if !value.contains(['\n', '\t', '\r']) {
            vec![vec![value.to_owned()]]
        } else {
            match table::parse(&value.replace("\r\n", "\n").replace('\r', "\n"), b'\t') {
                Ok(table) => table.rows.into_iter().map(|r| r.cells).collect(),
                Err(error) => {
                    dialogs.error(&error);
                    return;
                }
            }
        };
        let mut candidate = self.side(left).clone();
        for (offset, values) in rows.iter().enumerate() {
            let target = row + offset;
            let mut cells = candidate
                .table
                .rows
                .get(target)
                .map(|r| r.cells.clone())
                .unwrap_or_default();
            cells.resize(cells.len().max(column + values.len()), String::new());
            for (offset, value) in values.iter().enumerate() {
                cells[column + offset] = value.clone();
            }
            let target = (target < candidate.table.rows.len()).then_some(target);
            if let Err(error) = candidate.replace_row(target, &cells) {
                dialogs.error(&error);
                return;
            }
        }
        self.remember_edit();
        *self.side_mut(left) = candidate;
        self.rebuild();
    }
    /// Overlay a real text editor inside the selected cell. The bottom details
    /// and viewport retain their dimensions throughout editing.
    pub(super) fn inline_editor(&mut self, ui: &mut Ui, dialogs: &mut dyn DialogHost) {
        let Some(rect) = ui.ctx().data(|d| d.get_temp::<Rect>(editor_rect_id())) else {
            return;
        };
        let Some(edit) = &mut self.editor else {
            return;
        };
        let escape = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let enter = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        let id = ui.id().with("table_inline_editor");
        let mut editor_ui = ui.new_child(UiBuilder::new().max_rect(rect));
        editor_ui.set_clip_rect(rect.intersect(ui.clip_rect()));
        editor_ui.spacing_mut().item_spacing = Vec2::ZERO;
        let output = TextEdit::multiline(&mut edit.value)
            .id(id)
            .font(table_grid::font())
            .desired_width(rect.width())
            .desired_rows(1)
            .margin(vec2(3.0, 0.0))
            .frame(false)
            .return_key(None)
            .show(&mut editor_ui);
        if edit.focus {
            output.response.request_focus();
            let mut state = output.state;
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(edit.value.chars().count()),
                )));
            state.store(ui.ctx(), id);
            edit.focus = false;
        }
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(Id::new("table_test_editor"), rect));
        if escape {
            self.editor = None;
        } else if enter || output.response.lost_focus() {
            self.commit_editor(dialogs);
        }
    }
}
