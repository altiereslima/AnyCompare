//! BC5 table geometry checked against the isolated native Table Compare view.
//! Keep the row number gutter fixed while columns scroll; both panes share rows.
use super::*;
use crate::ui::text_chrome::{self, Axis};

const HEADER_HEIGHT: f32 = 27.0;
const STATUS_HEIGHT: f32 = 18.0;
const SCROLLBAR_WIDTH: f32 = 17.0;
const DRAG_EDGE: f32 = 14.0;

fn body_rect(pane: Rect) -> Rect {
    Rect::from_min_max(
        pane.min + vec2(0.0, HEADER_HEIGHT),
        pane.max - vec2(SCROLLBAR_WIDTH, STATUS_HEIGHT),
    )
}

pub(super) fn data_rect(pane: Rect) -> Rect {
    let body = body_rect(pane);
    Rect::from_min_max(body.min + vec2(NUMBER_WIDTH, 0.0), body.max)
}

fn drag_speed(position: f32, start: f32, end: f32) -> f32 {
    let edge = DRAG_EDGE.min((end - start) * 0.25);
    let toward_end = (position - (end - edge)).clamp(0.0, 96.0);
    let toward_start = ((start + edge) - position).clamp(0.0, 96.0);
    (toward_end - toward_start) * 12.0
}

/// Selected-column marker, at the top of both the main and detail headers.
fn selected_column(ui: &Ui, header: Rect) {
    let x = header.center().x.floor();
    let y = header.top().floor() + 1.0;
    ui.painter().add(Shape::convex_polygon(
        vec![pos2(x - 5.0, y), pos2(x + 5.0, y), pos2(x, y + 5.0)],
        Color32::from_rgb(0, 136, 238),
        Stroke::NONE,
    ));
}

/// Metadata selectors reserve their arrow inside the same clickable menu,
/// including when the label is translated or the DPI changes.
fn selector<R>(
    ui: &mut Ui,
    label: impl Into<WidgetText>,
    menu: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<Option<R>> {
    let label: WidgetText = label.into();
    let text = format!("{}    ", label.text());
    let response = ui.menu_button(text, menu);
    let rect = response.response.rect;
    let x = (rect.right() - 6.0).floor();
    let y = (rect.center().y - 1.0).floor();
    ui.painter().add(Shape::convex_polygon(
        vec![pos2(x - 3.0, y), pos2(x + 3.0, y), pos2(x, y + 3.0)],
        Color32::from_gray(50),
        Stroke::NONE,
    ));
    response
}

// Painted viewports have already reserved their space. An allocating child
// would move the parent's flow cursor back to the top of the grid.
pub(super) fn overlay<R>(ui: &mut Ui, builder: UiBuilder, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut child = ui.new_child(builder);
    add(&mut child)
}
pub(super) fn pane_rects(area: Rect) -> [Rect; 2] {
    let left_width = ((area.width() - OVERVIEW_WIDTH) / 2.0).floor();
    let left = Rect::from_min_size(
        area.min + vec2(OVERVIEW_WIDTH, 0.0),
        vec2(left_width, area.height()),
    );
    let right = Rect::from_min_max(left.right_top() + vec2(3.0, 0.0), area.max);
    [left, right]
}
fn path_icon(
    ui: &mut Ui,
    icons: &mut IconManager,
    name: &str,
    enabled: bool,
    tooltip: &str,
    width: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 23.0), Sense::click());
    if enabled && response.hovered() {
        ui.painter()
            .rect_filled(rect.shrink(0.5), 0.0, Color32::from_rgb(229, 243, 255));
        ui.painter().rect_stroke(
            rect.shrink(0.5),
            0.0,
            Stroke::new(1.0_f32, Color32::from_rgb(204, 232, 255)),
        );
    }
    if name == "load_spec" {
        // Small bent load arrow belongs to the path control, not the toolbar's
        // yellow Reload action. It is disabled when the displayed path is loaded.
        let points = [
            (-6.0, -5.0),
            (0.0, -5.0),
            (3.0, -2.0),
            (3.0, 1.0),
            (6.0, 1.0),
            (1.0, 6.0),
            (-4.0, 1.0),
            (-1.0, 1.0),
            (-1.0, -1.0),
            (-3.0, -3.0),
            (-6.0, -3.0),
        ];
        let center = ui
            .painter()
            .round_pos_to_pixels(pos2(rect.center().x, rect.top() + 11.0));
        ui.painter().add(Shape::Path(egui::epaint::PathShape {
            points: points
                .into_iter()
                .map(|(x, y)| center + vec2(x, y))
                .collect(),
            closed: true,
            fill: if enabled {
                Color32::from_rgb(247, 218, 126)
            } else {
                Color32::from_gray(232)
            },
            stroke: Stroke::new(
                1.0_f32,
                if enabled {
                    Color32::from_gray(128)
                } else {
                    Color32::from_gray(190)
                },
            )
            .into(),
        }));
    } else if let Some(texture) = icons.get_icon_with_state(ui.ctx(), name, enabled) {
        let rect = Rect::from_center_size(
            ui.painter()
                .round_pos_to_pixels(pos2(rect.center().x, rect.top() + 11.0)),
            vec2(16.0, 16.0),
        );
        ui.painter().image(
            texture.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            Id::new("table_test_path_icon").with((ui.id(), name)),
            (rect, enabled),
        )
    });
    response.on_hover_text(tr(tooltip))
}

pub(super) fn column_letter(mut column: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push((b'A' + (column % 26) as u8) as char);
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    letters.iter().rev().collect()
}
pub(super) fn font() -> FontId {
    FontId::new(13.0, FontFamily::Name("Table Mono".into()))
}
fn inline_chunks(a: &str, b: &str, ignore_case: bool) -> InlinePair {
    use crate::diff::{InlineChunk, InlineChunkKind};
    let a: Vec<_> = a.chars().collect();
    let b: Vec<_> = b.chars().collect();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(30);
    let mut result = (Vec::new(), Vec::new());
    for op in
        similar::capture_diff_slices_deadline(similar::Algorithm::Myers, &a, &b, Some(deadline))
    {
        let (tag, old, new) = op.as_tag_tuple();
        let left: String = a[old].iter().collect();
        let right: String = b[new].iter().collect();
        let kind = if tag == similar::DiffTag::Equal {
            InlineChunkKind::Same
        } else if ignore_case && left.to_lowercase() == right.to_lowercase() {
            InlineChunkKind::Unimportant
        } else {
            InlineChunkKind::Diff
        };
        if !left.is_empty() {
            result.0.push(InlineChunk {
                text: left,
                kind: kind.clone(),
            });
        }
        if !right.is_empty() {
            result.1.push(InlineChunk { text: right, kind });
        }
    }
    result
}
fn marker(ui: &Ui, rect: Rect, color: Color32) {
    ui.painter().rect_filled(
        Rect::from_center_size(rect.center(), vec2(5.0, 5.0)),
        0.0,
        color,
    );
}
fn hatch(ui: &Ui, rect: Rect) {
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        painter.line_segment(
            [pos2(x, rect.bottom()), pos2(x + rect.height(), rect.top())],
            Stroke::new(1.0_f32, Color32::from_gray(195)),
        );
        x += 7.0;
    }
}
fn row_color(pair: &Pair) -> Color32 {
    if pair.important {
        BcTheme::DIFF_TEXT
    } else if pair.different {
        BcTheme::MINOR_TEXT
    } else {
        Color32::from_gray(100)
    }
}

impl TableCompareScreen {
    pub(super) fn path_bar(
        &mut self,
        ui: &mut Ui,
        icons: &mut IconManager,
        dialogs: &mut dyn DialogHost,
    ) {
        let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 23.0), Sense::hover());
        let recent = crate::ui::session_picker::paths(ui.ctx(), crate::history::SessionKind::Table);
        for (i, rect) in pane_rects(bar).into_iter().enumerate() {
            let left = i == 0;
            overlay(
                ui,
                UiBuilder::new()
                    .id_salt(("table_specs", left))
                    .max_rect(rect)
                    .layout(Layout::left_to_right(Align::Center)),
                |ui| {
                    ui.set_clip_rect(rect);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    let show_save = self.side(left).editable() && !self.read_only;
                    let control_width = 23.0 + 34.0 + if show_save { 23.0 } else { 0.0 };
                    let history = history_combo::show_sized(
                        ui,
                        ("table_path", left),
                        &mut self.paths[i],
                        &recent,
                        vec2((rect.width() - control_width - 1.0).max(45.0), 23.0),
                        if left {
                            "table.left_file"
                        } else {
                            "table.right_file"
                        },
                    );
                    #[cfg(test)]
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(Id::new("table_test_path").with(left), history.response.rect)
                    });
                    if history.response.has_focus() && self.active_left != left {
                        self.selection = Selection::default();
                        self.active_left = left;
                    }
                    if let Some(path) = history.selected {
                        self.open_side(left, Path::new(&path), dialogs);
                    } else if history.submitted {
                        self.open_side(left, &PathBuf::from(&self.paths[i]), dialogs);
                    }
                    let pending =
                        !self.paths[i].is_empty() && self.paths[i] != self.side(left).path;
                    let load = path_icon(ui, icons, "load_spec", pending, "picture.load_file", 23.0);
                    if pending && load.clicked() {
                        self.open_side(left, &PathBuf::from(&self.paths[i]), dialogs);
                    }
                    let browse = path_icon(ui, icons, "imgBrowse", true, "menu.file.open", 23.0);
                    if browse.clicked() {
                        self.pick_file(left, dialogs);
                    }
                    let (drop_rect, drop) =
                        ui.allocate_exact_size(vec2(11.0, 23.0), Sense::click());
                    #[cfg(test)]
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            Id::new("table_test_path_controls").with(left),
                            [history.field_rect, load.rect, browse.rect, drop_rect],
                        )
                    });
                    let center = ui.painter().round_pos_to_pixels(drop_rect.center());
                    ui.painter().add(Shape::convex_polygon(
                        vec![
                            center + vec2(-3.0, -1.0),
                            center + vec2(3.0, -1.0),
                            center + vec2(0.0, 2.0),
                        ],
                        Color32::BLACK,
                        Stroke::NONE,
                    ));
                    let popup = ui.id().with("table_browse_popup");
                    if drop.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(popup));
                    }
                    popup::popup_below_widget(
                        ui,
                        popup,
                        &drop,
                        PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            ui.set_min_width(240.0);
                            if ui.button(tr("file.open_on_file_system")).clicked() {
                                self.pick_file(left, dialogs);
                                ui.close_menu();
                            }
                            if ui.button(tr("file.open_using_profile")).clicked() {
                                self.pick_file(left, dialogs);
                                ui.close_menu();
                            }
                            let path = PathBuf::from(&self.side(left).path);
                            if ui
                                .add_enabled(
                                    path.exists(),
                                    Button::new(tr("table.show_in_explorer")),
                                )
                                .clicked()
                            {
                                if let Err(e) = crate::ui::native_files::open(&path, true) {
                                    dialogs.error(&e);
                                }
                                ui.close_menu();
                            }
                            ui.add_enabled_ui(path.exists(), |ui| {
                                super::super::chrome::submenu_item(
                                    ui,
                                    icons,
                                    "menu.actions.open_with",
                                    |ui, icons| {
                                        if super::super::chrome::menu_item(
                                            ui,
                                            icons,
                                            "session.text_edit",
                                            true,
                                        )
                                        .clicked()
                                        {
                                            super::super::session_menu::request(
                                                ui.ctx(),
                                                super::super::session_menu::Request::EditText(
                                                    path.clone(),
                                                ),
                                            );
                                            ui.close_menu();
                                        }
                                        if super::super::chrome::menu_item(
                                            ui,
                                            icons,
                                            "file.associated_application",
                                            true,
                                        )
                                        .clicked()
                                        {
                                            if let Err(e) = dialogs.open_path(&path, false) {
                                                dialogs.error(&e);
                                            }
                                            ui.close_menu();
                                        }
                                    },
                                );
                            });
                            if !recent.is_empty() {
                                ui.separator();
                            }
                            for path in &recent {
                                if ui.button(path).clicked() {
                                    self.open_side(left, Path::new(path), dialogs);
                                    ui.close_menu();
                                }
                            }
                        },
                    );
                    if show_save {
                        let dirty = self.side(left).dirty()
                            || self.editor.as_ref().is_some_and(|e| e.left == left)
                                && self.editor_changed();
                        if path_icon(ui, icons, "imgSave", dirty, "menu.file.save", 23.0).clicked()
                            && dirty
                        {
                            self.save_side(left, false, dialogs);
                        }
                    }
                },
            );
        }
    }
    pub(super) fn metadata_bar(&mut self, ui: &mut Ui, dialogs: &mut dyn DialogHost) {
        let mut delimiter_action = None;
        let mut encoding_action = None;
        let mut formulas = self.options.formulas;
        let mut format_action = false;
        let mut sheet_action = None;
        let mut date_order = self.options.date_order;
        let mut number_locale = self.options.number_locale;
        let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 23.0), Sense::hover());
        for (i, rect) in pane_rects(bar).into_iter().enumerate() {
            let left = i == 0;
            overlay(
                ui,
                UiBuilder::new()
                    .max_rect(rect)
                    .layout(Layout::left_to_right(Align::Center)),
                |ui| {
                    ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                    ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
                    ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                    ui.spacing_mut().item_spacing.x = 7.0;
                    ui.spacing_mut().button_padding = vec2(1.0, 0.0);
                    let side = self.side(left);
                    ui.add_space(8.0);
                    ui.add(Label::new(side.metadata_label()).wrap_mode(TextWrapMode::Extend));
                    let format = if table::is_dbf(Path::new(&side.path)) {
                        "table.format.dbf"
                    } else if table::is_html(Path::new(&side.path)) {
                        "table.html_tables"
                    } else if Path::new(&side.path)
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("ods"))
                    {
                        "table.opendocument_spreadsheets"
                    } else if table::is_workbook(Path::new(&side.path)) {
                        "table.ms_excel_workbooks"
                    } else if side.format.is_fixed_width() {
                        "table.format.fixed_width"
                    } else {
                        match side.delimiter {
                            b'\t' => "table.tab_separated",
                            b';' => "table.semicolon_separated",
                            b'|' => "table.pipe_separated",
                            b' ' => "table.space_separated",
                            b'^' => "table.caret_separated",
                            b'~' => "table.tilde_separated",
                            b'=' => "table.equals_separated",
                            _ => "table.comma_separated_values",
                        }
                    };
                    ui.push_id(("table_metadata", left), |ui| {
                        selector(ui, tr(format), |ui| {
                            if ui.button(tr("text.file_format_ellipsis")).clicked() {
                                format_action = true;
                                ui.close_menu();
                            }
                            if !side.sheets.is_empty() {
                                ui.separator();
                                for (index, name) in side.sheets.iter().enumerate() {
                                    if ui
                                        .selectable_label(
                                            !side.missing_sheet && side.sheet == index,
                                            name,
                                        )
                                        .clicked()
                                    {
                                        sheet_action = Some((left, index));
                                        ui.close_menu();
                                    }
                                }
                            }
                        });
                        if let Some(doc) = &side.document {
                            selector(ui, tr(doc.encoding.label()), |ui| {
                                for reopen in [true, false] {
                                    selector(
                                        ui,
                                        tr(if reopen {
                                            "table.reopen_using"
                                        } else {
                                            "table.save_using"
                                        }),
                                        |ui| {
                                            for encoding in Encoding::ALL {
                                                if ui
                                                    .selectable_label(
                                                        doc.encoding == encoding,
                                                        tr(encoding.label()),
                                                    )
                                                    .clicked()
                                                {
                                                    encoding_action =
                                                        Some((left, encoding, reopen));
                                                    ui.close_menu();
                                                }
                                            }
                                        },
                                    );
                                }
                            });
                            ui.label(tr(doc.record_line_ending(side.delimiter).label()))
                                .on_hover_text(tr(
                                    "table.record_separators_line_breaks_inside_cells",
                                ));
                            if !side.format.is_fixed_width() {
                                selector(
                                    ui,
                                    tr(match side.delimiter {
                                        b'\t' => "table.tab",
                                        b';' => "table.delim.semicolon",
                                        b'|' => "table.delim.pipe",
                                        b' ' => "table.delim.space",
                                        b'^' => "table.delim.caret",
                                        b'~' => "table.delim.tilde",
                                        b'=' => "table.delim.equals",
                                        _ => "table.delim.comma",
                                    }),
                                    |ui| {
                                        for (label, delimiter) in [
                                            ("table.delim.comma", b','),
                                            ("table.delim.semicolon", b';'),
                                            ("table.tab", b'\t'),
                                            ("table.delim.pipe", b'|'),
                                            ("table.delim.space", b' '),
                                            ("table.delim.caret", b'^'),
                                            ("table.delim.tilde", b'~'),
                                        ] {
                                            if ui
                                                .selectable_label(
                                                    side.delimiter == delimiter,
                                                    tr(label),
                                                )
                                                .clicked()
                                            {
                                                delimiter_action = Some((left, delimiter));
                                                ui.close_menu();
                                            }
                                        }
                                    },
                                );
                            }
                        }
                        selector(
                            ui,
                            tr(if formulas {
                                "table.formulas"
                            } else {
                                "table.cell_data"
                            }),
                            |ui| {
                                ui.selectable_value(&mut formulas, false, tr("table.cell_data"));
                                ui.selectable_value(&mut formulas, true, tr("table.formulas"));
                            },
                        );
                        selector(ui, tr(date_order.label()), |ui| {
                            use crate::table_columns::DateOrder;
                            for (label, order) in [
                                ("app.language.auto", DateOrder::System),
                                ("table.dmy", DateOrder::Dmy),
                                ("table.mdy", DateOrder::Mdy),
                                ("table.ymd", DateOrder::Ymd),
                            ] {
                                ui.selectable_value(&mut date_order, order, tr(label));
                            }
                        });
                        selector(
                            ui,
                            self.options.default_regional().number_locale.short_label(),
                            |ui| {
                                for locale in [
                                    NumberLocale::System,
                                    NumberLocale::Dot,
                                    NumberLocale::Comma,
                                ] {
                                    ui.selectable_value(
                                        &mut number_locale,
                                        locale,
                                        tr(locale.label()),
                                    );
                                }
                            },
                        );
                    });
                },
            );
        }
        if format_action {
            self.format(dialogs);
        }
        if let Some((left, sheet)) = sheet_action {
            self.change_sheet(left, sheet, dialogs);
        }
        if let Some((left, encoding, reopen)) = encoding_action {
            self.change_encoding(left, encoding, reopen, dialogs);
        }
        if let Some((left, delimiter)) = delimiter_action {
            if self.commit_editor(dialogs) {
                let mut side = self.side(left).clone();
                side.delimiter = delimiter;
                match side.reparse() {
                    Ok(()) => {
                        *self.side_mut(left) = side;
                        self.options.delimiters[usize::from(!left)] = Some(delimiter);
                        self.rebuild();
                    }
                    Err(e) => dialogs.error(&e),
                }
            }
        }
        if formulas != self.options.formulas && self.commit_editor(dialogs) {
            self.options.formulas = formulas;
            self.rebuild();
        }
        if date_order != self.options.date_order && self.commit_editor(dialogs) {
            self.options.date_order = date_order;
            self.rebuild();
        }
        if number_locale != self.options.number_locale && self.commit_editor(dialogs) {
            self.options.number_locale = number_locale;
            self.options.decimal_comma = number_locale == NumberLocale::Comma;
            self.rebuild();
        }
    }
    fn change_encoding(
        &mut self,
        left: bool,
        encoding: Encoding,
        reopen: bool,
        dialogs: &mut dyn DialogHost,
    ) {
        if !self.commit_editor(dialogs) {
            return;
        }
        if reopen {
            if !self.confirm_side(left, dialogs) {
                return;
            }
            let path = self.side(left).path.clone();
            match document::Document::load_with_encoding(Path::new(&path), Some(encoding)) {
                Ok(doc) => {
                    let mut side = self.side(left).clone();
                    side.document = Some(doc);
                    match side.reparse() {
                        Ok(()) => {
                            *self.side_mut(left) = side;
                            self.undo.clear();
                            self.redo.clear();
                            self.rebuild();
                        }
                        Err(e) => dialogs.error(&e),
                    }
                }
                Err(e) => dialogs.error(&e),
            }
        } else if !self.read_only {
            if let Some(doc) = &mut self.side_mut(left).document {
                doc.encoding = encoding;
            }
        }
    }
    pub(super) fn overview(&mut self, ui: &mut Ui, area: Rect, viewport: f32) {
        let track = area.shrink2(vec2(5.0, 2.0));
        ui.painter().rect_filled(track, 0.0, Color32::WHITE);
        ui.painter()
            .rect_stroke(track, 0.0, Stroke::new(1.0_f32, Color32::GRAY));
        if self.visible.is_empty() {
            return;
        }
        for (i, &index) in self.visible.iter().enumerate() {
            if !self.pairs[index].different {
                continue;
            }
            let y = track.top() + i as f32 / self.visible.len() as f32 * track.height();
            let h = (track.height() / self.visible.len() as f32).max(1.0);
            ui.painter().rect_filled(
                Rect::from_min_size(pos2(track.left() + 2.0, y), vec2(track.width() - 4.0, h)),
                0.0,
                row_color(&self.pairs[index]),
            );
        }
        let content = self.visible.len() as f32 * ROW_HEIGHT;
        let y = track.top() + (self.scroll_y / content) * track.height();
        let h = (viewport / content * track.height()).clamp(3.0, track.height());
        ui.painter().rect_stroke(
            Rect::from_min_size(pos2(track.left(), y), vec2(track.width(), h)),
            0.0,
            Stroke::new(1.0_f32, Color32::from_gray(110)),
        );
        let response = ui.interact(
            area,
            ui.id().with("table_overview"),
            Sense::click_and_drag(),
        );
        if response.is_pointer_button_down_on() {
            if let Some(pos) = response.interact_pointer_pos() {
                self.scroll_y = (((pos.y - track.top()) / track.height()) * content
                    - viewport / 2.0)
                    .clamp(0.0, (content - viewport).max(0.0));
                ui.ctx().request_repaint();
            }
        }
    }
    pub(super) fn drag_selection(&mut self, ctx: &Context, area: Rect) -> Option<CellHit> {
        if !self.selection.dragging || self.visible.is_empty() {
            return None;
        }
        let (pointer, held, dt) = ctx.input(|input| {
            let held = input.pointer.primary_down();
            let released = input.pointer.button_released(PointerButton::Primary);
            let pointer = (input.focused && (held || released))
                .then(|| input.pointer.latest_pos())
                .flatten();
            (pointer, held, input.stable_dt.min(1.0 / 30.0))
        });
        let pointer = pointer?;
        let side = usize::from(!self.active_left);
        let data = data_rect(pane_rects(area)[side]);
        if data.width() < 1.0 || data.height() < 1.0 {
            return None;
        }
        let layout = self.column_layout();
        let &(_, last_x, last_width) = layout.last()?;
        let content_width = last_x + last_width - NUMBER_WIDTH;
        let content_height = self.visible.len() as f32 * ROW_HEIGHT;
        let old_x = self.scroll_x[side];
        let old_y = self.scroll_y;
        let mut x = old_x;
        let mut y = old_y;
        if held {
            x += drag_speed(pointer.x, data.left(), data.right()) * dt;
            y += drag_speed(pointer.y, data.top(), data.bottom()) * dt;
        }
        self.scroll_x[side] = x.clamp(0.0, (content_width - data.width()).max(0.0));
        self.scroll_y = y.clamp(0.0, (content_height - data.height()).max(0.0));
        if self.scroll_x[side] != old_x || self.scroll_y != old_y {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        // Resolve the endpoint against the new viewport. Pointer capture keeps
        // the drag on its original side even when it leaves that pane.
        let x = (pointer.x - data.left()).clamp(0.0, data.width() - 0.01)
            + self.scroll_x[side]
            + NUMBER_WIDTH;
        let y = (pointer.y - data.top()).clamp(0.0, data.height() - 0.01) + self.scroll_y;
        let position = (y / ROW_HEIGHT) as usize;
        let pair = self.visible[position.min(self.visible.len() - 1)];
        let column = layout
            .iter()
            .find(|(_, start, width)| x < start + width)
            .map_or(layout.last()?.0, |(column, _, _)| *column);
        (self.selected != pair || self.column != column).then_some((
            self.active_left,
            pair,
            column,
            CellGesture::DragExtend,
        ))
    }

    pub(super) fn pane(
        &mut self,
        ui: &mut Ui,
        left: bool,
        height: f32,
        icons: &mut IconManager,
    ) -> (Option<CellHit>, f32, bool) {
        let area = ui.max_rect();
        let body = body_rect(area);
        let data = data_rect(area);
        let layout = self.column_layout();
        let content_width = layout.last().map_or(0.0, |(_, x, w)| x + w - NUMBER_WIDTH);
        let content_height = self.visible.len() as f32 * ROW_HEIGHT;
        let i = usize::from(!left);
        let mut x_offset = self.scroll_x[i];
        let mut y_offset = self.scroll_y;
        let hover = ui.rect_contains_pointer(area);
        if hover {
            let delta = ui.input_mut(|input| {
                let d = input.smooth_scroll_delta;
                input.smooth_scroll_delta = Vec2::ZERO;
                if input.modifiers.shift {
                    vec2(d.x + d.y, 0.0)
                } else {
                    d
                }
            });
            x_offset -= delta.x;
            y_offset -= delta.y;
        }
        if self.jump {
            if let Some((_, x, w)) = layout.iter().find(|(c, _, _)| *c == self.column) {
                let x = x - NUMBER_WIDTH;
                if x < x_offset {
                    x_offset = x;
                } else if x >= x_offset + data.width() {
                    x_offset = (x + w - data.width()).min(x);
                }
            }
        }
        x_offset = x_offset.clamp(0.0, (content_width - data.width()).max(0.0));
        y_offset = y_offset.clamp(0.0, (content_height - body.height()).max(0.0));
        let status = Rect::from_min_max(pos2(area.left(), area.bottom() - STATUS_HEIGHT), area.max);
        let row = self.selected_row(left);
        let row_number = row.map(|r| self.display_row_number(left, r));
        let position = row_number
            .map(|r| format!("{}{r}", self.heading(left, self.column)))
            .unwrap_or_default();
        text_chrome::label(
            ui,
            Rect::from_min_size(status.min, vec2(86.0, STATUS_HEIGHT)),
            &position,
        );
        let state = if self.side(left).dirty() {
            "status.modified"
        } else if self.read_only || !self.side(left).editable() {
            "common.editing_disabled"
        } else {
            ""
        };
        text_chrome::separator(ui, status.left() + 86.0, status.top(), status.height());
        text_chrome::label(
            ui,
            Rect::from_min_size(status.min + vec2(88.0, 0.0), vec2(130.0, STATUS_HEIGHT)),
            &tr(state),
        );
        text_chrome::scroll_bar(
            ui,
            ui.id().with(("table_hbar", left)),
            Rect::from_min_max(status.min + vec2(220.0, 0.0), status.max - vec2(17.0, 0.0)),
            Axis::Horizontal,
            content_width,
            data.width(),
            &mut x_offset,
        );
        let vbar = Rect::from_min_max(
            pos2(body.right(), body.top()),
            pos2(area.right(), body.bottom()),
        );
        text_chrome::scroll_bar(
            ui,
            ui.id().with(("table_vbar", left)),
            vbar,
            Axis::Vertical,
            content_height,
            body.height(),
            &mut y_offset,
        );
        self.scroll_x[i] = x_offset;
        let mut hit = None;
        let side = self.side(left);
        let header = Rect::from_min_max(area.min, pos2(body.right(), body.top()));
        ui.painter().rect_filled(header, 0.0, BcTheme::WINDOW_BG);
        overlay(
            ui,
            UiBuilder::new().max_rect(Rect::from_min_max(pos2(data.left(), area.top()), data.max)),
            |ui| {
                ui.set_clip_rect(Rect::from_min_max(pos2(data.left(), area.top()), data.max));
                for &(column, x, width) in &layout {
                    let rect = Rect::from_min_size(
                        header.min + vec2(x - x_offset, 0.0),
                        vec2(width, HEADER_HEIGHT),
                    );
                    if !rect.intersects(ui.clip_rect()) {
                        continue;
                    }
                    cell(ui, rect, "", Color32::GRAY, BcTheme::WINDOW_BG);
                    if column == self.column {
                        selected_column(ui, rect);
                    }
                    let action = ui.interact(
                        rect.shrink2(vec2(4.0, 0.0)),
                        ui.id().with(("table_header", left, column)),
                        Sense::click(),
                    );
                    if action.clicked() {
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(header_action_id(), HeaderAction::Sort(column))
                        });
                    }
                    action.context_menu(|ui| {
                        for (label, action) in [
                            ("table.sort_view", HeaderAction::Sort(column)),
                            ("table.toggle_key_column", HeaderAction::Key(column)),
                            (
                                "table.toggle_unimportant_column",
                                HeaderAction::Unimportant(column),
                            ),
                            ("table.configure_column_ellipsis", HeaderAction::Configure(column)),
                        ] {
                            if ui.button(tr(label)).clicked() {
                                ui.ctx()
                                    .data_mut(|d| d.insert_temp(header_action_id(), action));
                                ui.close_menu();
                            }
                        }
                    });
                    let edge = Rect::from_min_max(
                        rect.right_top() - vec2(3.0, 0.0),
                        rect.right_bottom() + vec2(2.0, 0.0),
                    );
                    let response = ui
                        .interact(
                            edge,
                            ui.id().with(("table_resize", left, column)),
                            Sense::drag(),
                        )
                        .on_hover_cursor(CursorIcon::ResizeHorizontal);
                    if response.dragged() {
                        let w = (width + ui.input(|i| i.pointer.delta().x)).clamp(40.0, 1200.0);
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(header_action_id(), HeaderAction::Resize(column, w))
                        });
                    }
                    let mut caption = rect;
                    if self.options.key == Some(column) || self.options.extra_keys.contains(&column)
                    {
                        if let Some(texture) = icons.get_icon(ui.ctx(), "imgHeaderKey") {
                            ui.painter().image(
                                texture.id(),
                                Rect::from_center_size(
                                    rect.left_center() + vec2(10.0, 0.0),
                                    vec2(16.0, 16.0),
                                ),
                                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        caption.min.x += 18.0;
                    } else if self
                        .column_changes
                        .get(column)
                        .is_some_and(|c| *c != Change::Same)
                    {
                        marker(
                            ui,
                            Rect::from_center_size(
                                rect.left_center() + vec2(7.0, 0.0),
                                vec2(6.0, 6.0),
                            ),
                            if self.column_changes[column] == Change::Important {
                                BcTheme::DIFF_TEXT
                            } else {
                                BcTheme::MINOR_TEXT
                            },
                        );
                        caption.min.x += 11.0;
                    }
                    text_chrome::label(ui, caption, &self.heading(left, column));
                }
            },
        );
        let first = (y_offset / ROW_HEIGHT).floor() as usize;
        let last = ((y_offset + body.height()) / ROW_HEIGHT).ceil() as usize;
        for visible in first..last.min(self.visible.len()) {
            let index = self.visible[visible];
            let pair = &self.pairs[index];
            let row_index = if left { pair.left } else { pair.right };
            let row = row_index.map(|r| &side.table.rows[r]);
            let other = (if left { pair.right } else { pair.left })
                .map(|r| &self.side(!left).table.rows[r]);
            let y = body.top() + visible as f32 * ROW_HEIGHT - y_offset;
            let number = Rect::from_min_size(pos2(body.left(), y), vec2(NUMBER_WIDTH, ROW_HEIGHT));
            overlay(ui, UiBuilder::new().max_rect(body), |ui| {
                ui.set_clip_rect(body);
                cell(ui, number, "", Color32::GRAY, BcTheme::WINDOW_BG);
                if let Some(r) = row_index {
                    ui.painter().text(
                        pos2(number.right() - 4.0, number.center().y),
                        Align2::RIGHT_CENTER,
                        self.display_row_number(left, r).to_string(),
                        FontId::proportional(12.0),
                        Color32::GRAY,
                    );
                    marker(
                        ui,
                        Rect::from_center_size(
                            number.left_center() + vec2(15.0, 0.0),
                            vec2(7.0, 7.0),
                        ),
                        row_color(pair),
                    );
                }
                if self.selected == index {
                    let c = pos2(number.left() + 4.0, number.center().y);
                    ui.painter().add(Shape::convex_polygon(
                        vec![
                            c + vec2(-2.0, -5.0),
                            c + vec2(3.0, 0.0),
                            c + vec2(-2.0, 5.0),
                        ],
                        Color32::from_rgb(0, 120, 215),
                        Stroke::NONE,
                    ));
                }
            });
            overlay(ui, UiBuilder::new().max_rect(data), |ui| {
                ui.set_clip_rect(data);
                for &(column, x, width) in &layout {
                    let rect = Rect::from_min_size(
                        pos2(body.left() + x - x_offset, y),
                        vec2(width, ROW_HEIGHT),
                    );
                    if !rect.intersects(data) {
                        continue;
                    }
                    let (value, hint) = row.map_or((None, None), |r| self.cell(left, r, column));
                    let (other_value, other_hint) =
                        other.map_or((None, None), |r| self.cell(!left, r, column));
                    let handling = self.options.col_handling(column);
                    let mut change = if left {
                        table_columns::change_with_hints(
                            &self.options,
                            column,
                            value,
                            hint,
                            other_value,
                            other_hint,
                        )
                    } else {
                        table_columns::change_with_hints(
                            &self.options,
                            column,
                            other_value,
                            other_hint,
                            value,
                            hint,
                        )
                    };
                    if self.options.ignore_minor && change == Change::Minor {
                        change = Change::Same;
                    }
                    let (bg, fg) = match change {
                        Change::Important => (BcTheme::DIFF_BG, BcTheme::DIFF_TEXT),
                        Change::Minor => (BcTheme::MINOR_BG, BcTheme::MINOR_TEXT),
                        Change::Same => (Color32::WHITE, Color32::BLACK),
                    };
                    let inline = value.zip(other_value).filter(|(a, b)| {
                        change == Change::Important && a.len() + b.len() <= 16_384
                    });
                    let selected = self.selected_in_view(left, visible, column);
                    let displayed = table_val::display_cell(
                        value.unwrap_or(""),
                        hint,
                        self.options.date_order,
                        handling.as_text,
                    );
                    if selected && row.is_some() && self.physical_column(left, column).is_some() {
                        cell(
                            ui,
                            rect,
                            &displayed,
                            Color32::WHITE,
                            Color32::from_rgb(0, 105, 190),
                        );
                    } else if let Some((a, b)) = inline {
                        cell(ui, rect, "", fg, bg);
                        self.inline_text(
                            ui,
                            rect,
                            (index, column),
                            left,
                            (a, hint, b, other_hint, false),
                        );
                    } else {
                        cell(ui, rect, &displayed, fg, bg);
                    }
                    if row.is_none() {
                        hatch(ui, rect);
                    }
                    if self.selected == index && self.column == column {
                        if self.active_left == left {
                            for label in [
                                "table.cell_menu",
                                "table.edit_cell",
                                "Menu da célula",
                                "Editar célula",
                            ] {
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(
                                        Id::new("capture_menu_rect").with(label),
                                        rect.intersect(data),
                                    )
                                });
                            }
                        }
                        ui.painter().rect_stroke(
                            rect.shrink(0.5),
                            0.0,
                            Stroke::new(
                                1.0_f32,
                                if self.active_left == left {
                                    Color32::from_rgb(0, 120, 215)
                                } else {
                                    Color32::from_gray(90)
                                },
                            ),
                        );
                    }
                    if self.editor.as_ref().is_some_and(|e| {
                        e.left == left
                            && Some(e.row) == row_index
                            && Some(e.column) == self.physical_column(left, column)
                    }) {
                        cell(ui, rect, "", Color32::BLACK, Color32::WHITE);
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(
                                super::table_actions::editor_rect_id(),
                                rect.intersect(data),
                            )
                        });
                    }
                    let response = ui.interact(
                        rect.intersect(data),
                        ui.id().with((left, index, column)),
                        Sense::click_and_drag(),
                    );
                    #[cfg(test)]
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            Id::new("table_test_cell").with((left, index, column)),
                            rect.intersect(data),
                        )
                    });
                    let gesture = if response.secondary_clicked() {
                        Some(CellGesture::Context)
                    } else if response.drag_started_by(PointerButton::Primary) {
                        Some(CellGesture::DragStart)
                    } else if response.clicked() {
                        let modifiers = ui.input(|input| input.modifiers);
                        Some(if modifiers.shift {
                            CellGesture::Extend
                        } else if modifiers.ctrl {
                            CellGesture::Toggle
                        } else if response.double_clicked() {
                            CellGesture::Edit
                        } else {
                            CellGesture::Select
                        })
                    } else {
                        None
                    };
                    if let Some(gesture) = gesture {
                        hit = Some((left, index, column, gesture));
                    }
                    if response.secondary_clicked() {
                        super::table_actions::prepare_context(ui.ctx());
                    }
                    response.context_menu(|ui| self.cell_context(ui, icons, left, index, column));
                    response.on_hover_text(value.unwrap_or(""));
                }
            });
        }
        text_chrome::editor_edges(ui, area.shrink2(vec2(0.0, 0.5)), body);
        let _ = height;
        (hit, y_offset, hover)
    }
    fn display_row_number(&self, left: bool, row: usize) -> usize {
        if self.side(left).document.is_some() {
            row + 1
        } else {
            self.side(left).table.rows[row].line
        }
    }
    pub(super) fn details(&mut self, ui: &mut Ui) {
        let (splitter, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 5.0), Sense::drag());
        if response.dragged() {
            self.detail_height =
                (self.detail_height - ui.input(|i| i.pointer.delta().y)).clamp(70.0, 260.0);
        }
        response.on_hover_cursor(CursorIcon::ResizeVertical);
        ui.painter().line_segment(
            [
                splitter.left_center() + vec2(OVERVIEW_WIDTH, 0.0),
                splitter.right_center(),
            ],
            Stroke::new(1.0_f32, Color32::GRAY),
        );
        let (rect, _) = ui.allocate_exact_size(
            vec2(ui.available_width(), self.detail_height - 5.0),
            Sense::hover(),
        );
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(Id::new("table_test_details"), rect));
        let area = Rect::from_min_max(rect.min + vec2(OVERVIEW_WIDTH, 0.0), rect.max);
        let layout = self.column_layout();
        let width = layout.last().map_or(0.0, |(_, x, w)| x + w - NUMBER_WIDTH);
        let needs_hscroll = width > area.width();
        let view = if needs_hscroll {
            Rect::from_min_max(area.min, area.max - vec2(0.0, 14.0))
        } else {
            self.detail_x = 0.0;
            area
        };
        if needs_hscroll {
            text_chrome::scroll_bar(
                ui,
                ui.id().with("table_detail_scroll"),
                Rect::from_min_max(pos2(area.left(), view.bottom()), area.max),
                Axis::HorizontalWithArrows,
                width,
                view.width(),
                &mut self.detail_x,
            );
        }
        overlay(ui, UiBuilder::new().max_rect(view), |ui| {
            ui.set_clip_rect(view);
            let h = ((view.height() - 22.0) / 2.0).max(ROW_HEIGHT);
            for &(column, x, w) in &layout {
                let x = view.left() + x - NUMBER_WIDTH - self.detail_x;
                let header = Rect::from_min_size(pos2(x, view.top()), vec2(w, 22.0));
                if !header.intersects(view) {
                    continue;
                }
                cell(
                    ui,
                    header,
                    &self.heading(self.active_left, column),
                    Color32::GRAY,
                    BcTheme::WINDOW_BG,
                );
                if column == self.column {
                    selected_column(ui, header);
                }
                for (index, left) in [true, false].into_iter().enumerate() {
                    let row = self
                        .selected_row(left)
                        .map(|r| &self.side(left).table.rows[r]);
                    let other = self
                        .selected_row(!left)
                        .map(|r| &self.side(!left).table.rows[r]);
                    let (value, hint) = row.map_or((None, None), |r| self.cell(left, r, column));
                    let (other_value, other_hint) =
                        other.map_or((None, None), |r| self.cell(!left, r, column));
                    let handling = self.options.col_handling(column);
                    let change = if left {
                        table_columns::change_with_hints(
                            &self.options,
                            column,
                            value,
                            hint,
                            other_value,
                            other_hint,
                        )
                    } else {
                        table_columns::change_with_hints(
                            &self.options,
                            column,
                            other_value,
                            other_hint,
                            value,
                            hint,
                        )
                    };
                    let (bg, fg) = match change {
                        Change::Important => (BcTheme::DIFF_BG, BcTheme::DIFF_TEXT),
                        Change::Minor => (BcTheme::MINOR_BG, BcTheme::MINOR_TEXT),
                        Change::Same => (Color32::WHITE, Color32::BLACK),
                    };
                    let displayed = table_val::display_cell(
                        value.unwrap_or(""),
                        hint,
                        self.options.date_order,
                        handling.as_text,
                    );
                    let text = if column == self.column {
                        displayed.replace(' ', "·").replace('\n', "↵")
                    } else {
                        displayed.into_owned()
                    };
                    let cell_rect = Rect::from_min_size(
                        pos2(x, view.top() + 22.0 + index as f32 * h),
                        vec2(w, h),
                    );
                    let inline = value.zip(other_value).filter(|(a, b)| {
                        change == Change::Important && a.len() + b.len() <= 16_384
                    });
                    if let Some((a, b)) = inline {
                        cell(ui, cell_rect, "", fg, bg);
                        self.inline_text(
                            ui,
                            cell_rect,
                            (self.selected, column),
                            left,
                            (a, hint, b, other_hint, column == self.column),
                        );
                    } else {
                        cell(ui, cell_rect, &text, fg, bg);
                    }
                    ui.interact(
                        cell_rect.intersect(view),
                        ui.id().with(("table_detail", left, column)),
                        Sense::hover(),
                    )
                    .on_hover_text(value.unwrap_or(""));
                }
            }
        });
    }
    fn inline_text(
        &self,
        ui: &Ui,
        rect: Rect,
        key: (usize, usize),
        left: bool,
        values: (&str, Option<&CellHint>, &str, Option<&CellHint>, bool),
    ) {
        let mut cache = self.inline_cache.borrow_mut();
        if cache.len() >= 4096 && !cache.contains_key(&key) {
            cache.clear();
        }
        let handling = self.options.col_handling(key.1);
        let chunks = cache.entry(key).or_insert_with(|| {
            let a = table_val::display_cell(
                values.0,
                values.1,
                self.options.date_order,
                handling.as_text,
            );
            let b = table_val::display_cell(
                values.2,
                values.3,
                self.options.date_order,
                handling.as_text,
            );
            if left {
                inline_chunks(&a, &b, handling.ignore_case)
            } else {
                inline_chunks(&b, &a, handling.ignore_case)
            }
        });
        let chunks = if left { &chunks.0 } else { &chunks.1 };
        let mut job = egui::text::LayoutJob::default();
        let mut remaining = ((rect.width() / 6.0).ceil() as usize + 4).min(512);
        for chunk in chunks {
            if remaining == 0 {
                break;
            }
            let text: String = chunk
                .text
                .chars()
                .take(remaining)
                .map(|c| match c {
                    '\n' | '\r' => '↵',
                    ' ' if values.4 => '·',
                    _ => c,
                })
                .collect();
            remaining = remaining.saturating_sub(text.chars().count());
            let color = match chunk.kind {
                crate::diff::InlineChunkKind::Same => Color32::BLACK,
                crate::diff::InlineChunkKind::Diff => BcTheme::DIFF_TEXT,
                crate::diff::InlineChunkKind::Unimportant => BcTheme::MINOR_TEXT,
            };
            job.append(&text, 0.0, TextFormat::simple(font(), color));
        }
        job.wrap.max_width = f32::INFINITY;
        let galley = ui.fonts(|f| f.layout_job(job));
        let pos = rect.left_center() + vec2(3.0, -galley.size().y / 2.0);
        ui.painter()
            .with_clip_rect(rect.intersect(ui.clip_rect()))
            .galley(pos, galley, Color32::BLACK);
    }
    pub(super) fn sheet_tabs(&mut self, ui: &mut Ui, dialogs: &mut dyn DialogHost) {
        if self.left.sheets.is_empty() && self.right.sheets.is_empty() {
            return;
        }
        let sheets = self.sheet_pairs();
        let mut pick = None;
        let mut compare_all = false;
        let mut alignment = self.options.sheet_alignment;
        ui.horizontal(|ui| {
            ui.add_space(OVERVIEW_WIDTH);
            ScrollArea::horizontal()
                .id_salt("table_sheet_tabs")
                .max_height(26.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (i, pair) in sheets.iter().enumerate() {
                            let name = |left: bool, index: Option<usize>| {
                                index
                                    .and_then(|s| self.side(left).sheets.get(s))
                                    .map_or("—", String::as_str)
                            };
                            let label = if name(true, pair.left) == name(false, pair.right) {
                                name(true, pair.left).to_owned()
                            } else {
                                format!("{} / {}", name(true, pair.left), name(false, pair.right))
                            };
                            let active = pair.left
                                == (!self.left.missing_sheet).then_some(self.left.sheet)
                                && pair.right
                                    == (!self.right.missing_sheet).then_some(self.right.sheet);
                            let summary = self.sheet_summaries.get(i);
                            let color =
                                if summary.is_some_and(|s| s.important > 0 || s.error.is_some()) {
                                    BcTheme::DIFF_TEXT
                                } else if pair.left.is_none() || pair.right.is_none() {
                                    Color32::from_rgb(128, 0, 255)
                                } else if summary.is_some_and(|s| s.minor > 0) {
                                    BcTheme::MINOR_TEXT
                                } else {
                                    Color32::from_gray(90)
                                };
                            let response = ui.add(
                                Button::new(RichText::new(format!("▪ {label}")).color(color))
                                    .selected(active),
                            );
                            #[cfg(test)]
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(Id::new("table_test_sheet").with(i), response.rect)
                            });
                            if response.clicked() {
                                pick = Some(*pair);
                            }
                            response.context_menu(|ui| {
                                if ui.button(tr("table.compare_all_sheets")).clicked() {
                                    compare_all = true;
                                    ui.close_menu();
                                }
                                ui.separator();
                                for (label, mode) in [
                                    ("table.file_sheet_order", Alignment::Position),
                                    ("table.align_by_left_name", Alignment::LeftNames),
                                    ("table.align_by_right_name", Alignment::RightNames),
                                ] {
                                    if ui
                                        .selectable_value(&mut alignment, mode, tr(label))
                                        .clicked()
                                    {
                                        ui.close_menu();
                                    }
                                }
                            });
                            if let Some(summary) = summary {
                                response.on_hover_text(summary.error.clone().unwrap_or_else(
                                    || {
                                        format!(
                                            "{} {} · {} {} · {} {}",
                                            summary.rows,
                                            tr("table.rows"),
                                            summary.important,
                                            tr("table.different"),
                                            summary.minor,
                                            tr("table.unimportant_differences")
                                        )
                                    },
                                ));
                            }
                        }
                    });
                });
        });
        if compare_all && self.commit_editor(dialogs) {
            self.sheet_summaries =
                crate::table_sheets::compare(&self.left, &self.right, &self.options, &sheets);
        }
        if let Some(pair) = pick {
            self.select_sheet_pair(pair, dialogs);
        }
        if alignment != self.options.sheet_alignment && self.confirm_saved(dialogs) {
            self.options.sheet_alignment = alignment;
            if let Some(&pair) = self.sheet_pairs().first() {
                self.select_sheet_pair(pair, dialogs);
            }
        }
    }
    pub(super) fn window_status(&self, ui: &mut Ui, icons: &mut IconManager) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 19.0), Sense::hover());
        ui.painter().line_segment(
            [rect.left_top(), rect.right_top()],
            Stroke::new(1.0_f32, Color32::from_gray(215)),
        );
        let count = self
            .pairs
            .iter()
            .filter(|p| {
                if self.options.ignore_minor {
                    p.important
                } else {
                    p.different
                }
            })
            .count();
        let same_bytes = self.left.same_saved_bytes(&self.right);
        let result = if same_bytes {
            tr("common.binary_same")
        } else if count == 0 {
            tr("common.same")
        } else {
            format!("{count} {}", tr("table.difference_row_s"))
        };
        let icon = if same_bytes {
            "imgContentSameBinary"
        } else if count > 0 {
            "status_diff"
        } else {
            "status_same"
        };
        if let Some(texture) = icons.get_icon(ui.ctx(), icon) {
            ui.painter().image(
                texture.id(),
                Rect::from_center_size(rect.left_center() + vec2(9.0, 0.0), vec2(16.0, 16.0)),
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        text_chrome::label(
            ui,
            Rect::from_min_max(
                rect.min + vec2(24.0, 0.0),
                pos2(rect.left() + 258.0, rect.bottom()),
            ),
            &result,
        );
        let row_state = self.pairs.get(self.selected).map_or("", |p| {
            if p.important {
                "table.important_difference"
            } else if p.different {
                "table.unimportant_difference"
            } else {
                "common.same"
            }
        });
        let mut x = rect.left() + 258.0;
        for (w, label) in [
            (
                340.0,
                tr(if self.align_pending.is_some() {
                    "table.select_the_row_on_the_other_side_esc_cancels"
                } else {
                    row_state
                }),
            ),
            (
                rect.width(),
                format!(
                    "{} {:.2} {}",
                    tr("table.load_time"),
                    self.load_time,
                    tr("common.seconds")
                ),
            ),
        ] {
            text_chrome::separator(ui, x, rect.top() + 2.0, rect.height() - 3.0);
            text_chrome::label(
                ui,
                Rect::from_min_max(
                    pos2(x + 2.0, rect.top()),
                    pos2((x + w).min(rect.right()), rect.bottom()),
                ),
                &label,
            );
            x += w;
        }
        super::super::chrome::status_bar_grip(ui.painter(), rect);
    }
}

#[cfg(test)]
mod native_oracles {
    use super::*;
    #[test]
    fn table_case_differences_are_red_unless_the_rule_marks_them_unimportant() {
        use crate::diff::InlineChunkKind;
        // BC5 regional probe: Test/test is important in Table Compare, unlike
        // the default case-only coloring of the separate Text Compare engine.
        let (a, b) = inline_chunks("Test", "test", false);
        assert_eq!(a[0].text, "T");
        assert_eq!(b[0].text, "t");
        assert_eq!(a[0].kind, InlineChunkKind::Diff);
        assert_eq!(a[1].kind, InlineChunkKind::Same);
        assert_eq!(
            inline_chunks("Test", "test", true).0[0].kind,
            InlineChunkKind::Unimportant
        );
    }
}
