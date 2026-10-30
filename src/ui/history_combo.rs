use egui::*;

pub struct Output {
    pub response: Response,
    pub selected: Option<String>,
    pub submitted: bool,
}

/// Editable MRU field with one border, an attached dropdown and an opaque list.
/// Choices are returned separately so opening a recent path still passes through
/// the document's unsaved-change confirmation before replacing the field value.
pub fn show(
    ui: &mut Ui,
    salt: impl std::hash::Hash,
    text: &mut String,
    history: &[String],
    width: f32,
    hint: &str,
) -> Output {
    show_sized(ui, salt, text, history, vec2(width, 21.0), hint)
}

/// Same field with an explicit size (the folder filter combo of the toolbar is 23 px tall).
pub fn show_sized(
    ui: &mut Ui,
    salt: impl std::hash::Hash,
    text: &mut String,
    history: &[String],
    size: Vec2,
    hint: &str,
) -> Output {
    let id = ui.make_persistent_id(salt);
    let popup_id = id.with("history");
    let open = ui.memory(|memory| memory.is_popup_open(popup_id));
    let mut selected = None;
    let mut choices = Vec::new();
    for value in std::iter::once(&*text).chain(history) {
        if !value.is_empty() && !choices.contains(value) {
            choices.push(value.clone());
        }
    }
    let mut highlighted = ui
        .ctx()
        .data(|data| data.get_temp::<usize>(popup_id))
        .unwrap_or(0)
        .min(choices.len().saturating_sub(1));
    if open {
        if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowDown)) {
            highlighted = (highlighted + 1).min(choices.len().saturating_sub(1));
        }
        if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowUp)) {
            highlighted = highlighted.saturating_sub(1);
        }
        if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter)) {
            selected = choices.get(highlighted).cloned();
            ui.memory_mut(|memory| memory.close_popup());
        }
        if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
            ui.memory_mut(|memory| memory.close_popup());
        }
    }
    let (rect, anchor) = ui.allocate_exact_size(vec2(size.x.max(45.0), size.y), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
    let mut edit_rect = rect.shrink2(vec2(2.0, 1.0));
    edit_rect.max.x -= 16.0;
    let response = {
        let mut edit_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(edit_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        edit_ui.spacing_mut().interact_size.y = edit_rect.height();
        edit_ui.visuals_mut().selection.bg_fill = Color32::from_rgb(0, 120, 215);
        edit_ui.visuals_mut().selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
        let output = TextEdit::singleline(text)
            .id(id)
            .frame(false)
            .desired_width(edit_rect.width())
            .margin(vec2(1.0, 0.0))
            .hint_text(crate::i18n::tr(if hint.starts_with("Arquivo ") {
                "Entre o nome do arquivo aqui"
            } else {
                hint
            }))
            .show(&mut edit_ui);
        // egui 0.29 keeps the original foreground on selections. Paint the
        // selected region from the CURRENT edit output, avoiding stale cursor
        // ranges while typing or dragging a selection.
        if output.response.has_focus() {
            if let Some(range) = output.cursor_range.filter(|range| !range.is_empty()) {
                let [start, end] = range.sorted_cursors();
                let start = output.galley.pos_from_cursor(&start);
                let end = output.galley.pos_from_cursor(&end);
                let selection = Rect::from_min_max(start.min, pos2(end.min.x, end.max.y))
                    .translate(
                        edit_ui
                            .painter()
                            .round_pos_to_pixels(output.galley_pos)
                            .to_vec2(),
                    )
                    .intersect(output.text_clip_rect);
                let painter = edit_ui.painter().with_clip_rect(selection);
                painter.rect_filled(selection, 0.0, Color32::from_rgb(0, 120, 215));
                painter.galley_with_override_text_color(
                    output.galley_pos,
                    output.galley,
                    Color32::WHITE,
                );
            }
        }
        output.response
    };
    let drop_rect = Rect::from_min_max(
        pos2(rect.right() - 17.0, rect.top() + 1.0),
        rect.right_bottom() - vec2(1.0, 1.0),
    );
    let drop = ui.interact(drop_rect, id.with("drop"), Sense::click());
    let keyboard_open = response.has_focus()
        && ui.input_mut(|input| {
            input.consume_key(Modifiers::ALT, Key::ArrowDown)
                || input.consume_key(Modifiers::NONE, Key::F4)
        });
    if drop.clicked() || keyboard_open {
        ui.memory_mut(|memory| memory.toggle_popup(popup_id));
        response.request_focus();
        if let Some(mut state) = TextEdit::load_state(ui.ctx(), id) {
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(text.chars().count()),
                )));
            state.store(ui.ctx(), id);
        }
        highlighted = choices.iter().position(|value| value == text).unwrap_or(0);
    }
    let focused = response.has_focus() || ui.memory(|memory| memory.is_popup_open(popup_id));
    if drop.hovered() || focused {
        ui.painter()
            .rect_filled(drop_rect, 0.0, Color32::from_rgb(204, 228, 249));
    }
    let center = ui.painter().round_pos_to_pixels(drop_rect.center());
    ui.painter().line_segment(
        [center + vec2(-3.0, -1.0), center + vec2(0.0, 2.0)],
        Stroke::new(1.0_f32, Color32::from_gray(70)),
    );
    ui.painter().line_segment(
        [center + vec2(0.0, 2.0), center + vec2(3.0, -1.0)],
        Stroke::new(1.0_f32, Color32::from_gray(70)),
    );
    ui.painter().rect_stroke(
        rect,
        0.0,
        Stroke::new(
            1.0_f32,
            if focused {
                Color32::from_rgb(0, 120, 215)
            } else {
                Color32::from_gray(140)
            },
        ),
    );
    ui.ctx()
        .data_mut(|data| data.insert_temp(Id::new("capture_menu_rect").with(hint), drop_rect));
    let popup_style = (ui.visuals().window_fill, ui.visuals().window_rounding);
    ui.visuals_mut().window_fill = Color32::WHITE;
    ui.visuals_mut().window_rounding = Rounding::ZERO;
    popup::popup_below_widget(
        ui,
        popup_id,
        &anchor,
        PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(rect.width() - 8.0);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ScrollArea::both()
                .max_height(200.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (index, value) in choices.iter().enumerate() {
                        let text_width = ui.fonts(|fonts| {
                            fonts
                                .layout_no_wrap(
                                    value.clone(),
                                    FontId::proportional(12.0),
                                    Color32::BLACK,
                                )
                                .size()
                                .x
                        });
                        let (row, response) = ui.allocate_exact_size(
                            vec2(text_width.max(rect.width() - 10.0), 15.0),
                            Sense::click(),
                        );
                        if response.hovered() {
                            highlighted = index;
                        }
                        let active = index == highlighted;
                        if active {
                            ui.painter()
                                .rect_filled(row, 0.0, Color32::from_rgb(0, 120, 215));
                        }
                        ui.painter().text(
                            row.left_center() + vec2(2.0, 0.0),
                            Align2::LEFT_CENTER,
                            value,
                            FontId::proportional(12.0),
                            if active {
                                Color32::WHITE
                            } else {
                                Color32::BLACK
                            },
                        );
                        if response.clicked() {
                            selected = Some(value.clone());
                            ui.memory_mut(|memory| memory.close_popup());
                        }
                    }
                });
        },
    );
    ui.visuals_mut().window_fill = popup_style.0;
    ui.visuals_mut().window_rounding = popup_style.1;
    ui.ctx()
        .data_mut(|data| data.insert_temp(popup_id, highlighted));
    let submitted =
        !open && response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
    Output {
        response,
        selected,
        submitted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_input_draws_current_text_white_on_blue_and_accepts_typing() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut value = "ação".to_owned();
        let mut id = Id::NULL;
        let mut frame = |events: Vec<Event>, select: bool| {
            ctx.run(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default().show(ctx, |ui| {
                        let output = show(ui, "selection", &mut value, &[], 300.0, "Localizar");
                        id = output.response.id;
                        output.response.request_focus();
                        if select {
                            let mut state = TextEdit::load_state(ctx, id).unwrap();
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(4),
                                )));
                            state.store(ctx, id);
                        }
                    });
                },
            )
        };
        let _ = frame(vec![], true);
        let selected = frame(vec![], false);
        assert!(
            selected.shapes.iter().any(|shape| matches!(
                &shape.shape, egui::epaint::Shape::Text(text)
                    if text.override_text_color == Some(Color32::WHITE)
                        && text.galley.text() == "ação"
            )),
            "selected foreground must be white"
        );
        let changed = frame(vec![Event::Text("Novo".into())], false);
        assert!(
            changed.shapes.iter().any(|shape| matches!(
                &shape.shape, egui::epaint::Shape::Text(text) if text.galley.text() == "Novo"
            )),
            "typing must render the current text in the same frame"
        );
        drop(frame);
        assert_eq!(value, "Novo");
    }

    #[test]
    fn dropdown_keyboard_selection_returns_candidate_without_opening_or_overwriting_field() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut value = "C:/original.txt".to_owned();
        let choices = vec!["D:/ação.txt".to_owned(), "E:/outro.txt".to_owned()];
        let mut field_id = None;
        let mut selected = None;
        let mut frame = |events| {
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 400.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default().show(ctx, |ui| {
                        let output = show(
                            ui,
                            "test_mru",
                            &mut value,
                            &choices,
                            350.0,
                            "Arquivo esquerdo",
                        );
                        output.response.request_focus();
                        field_id = Some(output.response.id);
                        selected = output.selected;
                        assert!(!output.submitted);
                    });
                },
            );
        };
        let key = |key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame(vec![]);
        frame(vec![key(Key::F4)]);
        frame(vec![key(Key::ArrowDown)]);
        frame(vec![key(Key::Enter)]);
        assert_eq!(selected.as_deref(), Some("D:/ação.txt"));
        assert_eq!(value, "C:/original.txt");
        let id = field_id.unwrap();
        assert!(!ctx.memory(|memory| memory.is_popup_open(id.with("history"))));
    }
}
