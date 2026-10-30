//! Shared session-tree dropdown for every generated comparison toolbar.
use super::{
    home::{HomeAction, HomeScreen},
    icons::IconManager,
    session_menu::{self, Request},
};
use crate::history::RecentSession;
use egui::*;

fn history_id() -> Id {
    Id::new("anycompare_session_picker_history")
}

pub fn set_history(ctx: &Context, entries: &[RecentSession]) {
    ctx.data_mut(|data| data.insert_temp(history_id(), entries.to_vec()));
}

/// The tree owns keyboard input while open. Do not let an editor behind the
/// popup receive typing, paste, deletion or editing shortcuts. Native typeahead
/// is not reconstructed yet; unsupported keys must not change a document.
pub fn is_open(ctx: &Context) -> bool {
    ctx.memory(|memory| memory.is_popup_open(Id::new("anycompare_session_picker_popup")))
}

pub fn route_input(ctx: &Context, input: &mut RawInput) {
    if is_open(ctx) {
        input.events.retain(|event| match event {
            Event::Text(_) | Event::Paste(_) | Event::Cut | Event::Ime(_) => false,
            Event::Key { key, modifiers, .. } => {
                modifiers.is_none()
                    && matches!(
                        key,
                        Key::ArrowDown
                            | Key::ArrowUp
                            | Key::ArrowLeft
                            | Key::ArrowRight
                            | Key::Enter
                            | Key::Escape
                    )
            }
            _ => true,
        });
    }
}

pub fn show(ui: &mut Ui, icons: &mut IconManager, anchor: &Response) {
    let popup = Id::new("anycompare_session_picker_popup");
    let state_id = popup.with("tree");
    if anchor.clicked() {
        ui.memory_mut(|memory| memory.toggle_popup(popup));
    }
    ui.ctx().data_mut(|data| {
        data.insert_temp(Id::new("capture_menu_rect").with("Sessões"), anchor.rect)
    });
    if !ui.memory(|memory| memory.is_popup_open(popup)) {
        return;
    }
    let mut tree = ui
        .ctx()
        .data(|data| data.get_temp::<HomeScreen>(state_id))
        .unwrap_or_else(HomeScreen::new);
    tree.recent = ui
        .ctx()
        .data(|data| data.get_temp::<Vec<RecentSession>>(history_id()))
        .unwrap_or_default();
    let size = tree.session_dropdown_size(ui, anchor.rect.width());
    let response = Area::new(popup)
        .kind(UiKind::Popup)
        .order(Order::Foreground)
        .fixed_pos(anchor.rect.left_bottom())
        .show(ui.ctx(), |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            Frame::none()
                .inner_margin(Margin::same(1.0))
                .fill(Color32::from_gray(102))
                .show(ui, |ui| {
                    tree.session_dropdown(ui, icons, size - Vec2::splat(2.0))
                })
                .inner
        });
    ui.ctx().data_mut(|data| data.insert_temp(state_id, tree));
    let command = match response.inner {
        HomeAction::OpenTextCompare => Some(Request::New(crate::history::SessionKind::Text)),
        HomeAction::OpenFolderCompare => Some(Request::New(crate::history::SessionKind::Folder)),
        HomeAction::OpenHexCompare => Some(Request::New(crate::history::SessionKind::Hex)),
        HomeAction::OpenRecentSession(record) => Some(Request::Recent(record)),
        _ => None,
    };
    if let Some(command) = command {
        session_menu::request(ui.ctx(), command);
        ui.memory_mut(|memory| memory.close_popup());
    } else if ui.input(|input| input.key_pressed(Key::Escape))
        || anchor.clicked_elsewhere() && response.response.clicked_elsewhere()
    {
        ui.memory_mut(|memory| memory.close_popup());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::SessionKind;

    fn frame(
        ctx: &Context,
        icons: &mut IconManager,
        time: &mut f64,
        events: Vec<Event>,
    ) -> FullOutput {
        frame_with_document(ctx, icons, time, events, None)
    }
    fn frame_with_document(
        ctx: &Context,
        icons: &mut IconManager,
        time: &mut f64,
        events: Vec<Event>,
        mut document: Option<&mut String>,
    ) -> FullOutput {
        *time += if events
            .iter()
            .any(|e| matches!(e, Event::PointerButton { pressed: true, .. }))
        {
            1.0
        } else {
            0.016
        };
        let mut input = RawInput {
            time: Some(*time),
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(700.0, 450.0))),
            events,
            ..Default::default()
        };
        route_input(ctx, &mut input);
        ctx.run(input, |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                super::super::toolbar::show(
                    ui,
                    icons,
                    super::super::toolbar_spec::TEXT_COMPARE,
                    true,
                    &|name| super::super::toolbar::ToolState::enabled(name == "SessionsCombo"),
                    &mut |_, _, _| {},
                );
                if let Some(document) = document.as_deref_mut() {
                    ui.put(
                        Rect::from_min_size(pos2(8.0, 100.0), vec2(600.0, 200.0)),
                        TextEdit::multiline(document).id(Id::new("session_test_editor")),
                    );
                }
            });
        })
    }
    fn click(ctx: &Context, icons: &mut IconManager, time: &mut f64, pos: Pos2) {
        for pressed in [true, false] {
            frame(
                ctx,
                icons,
                time,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        frame(ctx, icons, time, vec![]);
    }
    fn caption(frame: &FullOutput, label: &str) -> Rect {
        frame
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                Shape::Text(t) if t.galley.text() == label => {
                    Some(Rect::from_min_size(t.pos, t.galley.size()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing caption {label}"))
    }
    #[test]
    fn popup_typing_and_paste_cannot_change_the_document_behind_it() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut time = 0.0;
        let mut document = String::from("original");
        for _ in 0..2 {
            frame_with_document(&ctx, &mut icons, &mut time, vec![], Some(&mut document));
        }
        let anchor = ctx
            .data(|data| data.get_temp::<Rect>(Id::new("capture_menu_rect").with("Sessões")))
            .unwrap();
        click(&ctx, &mut icons, &mut time, anchor.center());
        assert!(is_open(&ctx));
        // Exercise an editor that still owns focus: the raw input boundary must
        // protect its buffer even before the popup's own widgets run.
        ctx.memory_mut(|memory| memory.request_focus(Id::new("session_test_editor")));
        frame_with_document(
            &ctx,
            &mut icons,
            &mut time,
            vec![
                Event::Text("ERROR".into()),
                Event::Paste("PASTE".into()),
                Event::Key {
                    key: Key::Backspace,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
            ],
            Some(&mut document),
        );
        assert_eq!(document, "original");
        frame_with_document(
            &ctx,
            &mut icons,
            &mut time,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            Some(&mut document),
        );
        assert!(
            !ctx.memory(|memory| memory.is_popup_open(Id::new("anycompare_session_picker_popup")))
        );
        ctx.memory_mut(|memory| memory.request_focus(Id::new("session_test_editor")));
        frame_with_document(
            &ctx,
            &mut icons,
            &mut time,
            vec![Event::Text("OK".into())],
            Some(&mut document),
        );
        assert!(document.contains("OK"));
    }

    #[test]
    fn native_popup_fits_visible_rows_and_measures_collapsed_children() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut time = 0.0;
        for _ in 0..2 {
            frame(&ctx, &mut icons, &mut time, vec![]);
        }
        let anchor = ctx
            .data(|d| d.get_temp::<Rect>(Id::new("capture_menu_rect").with("Sessões")))
            .unwrap();
        click(&ctx, &mut icons, &mut time, anchor.center());
        let popup = Id::new("anycompare_session_picker_popup");
        let compact = ctx.memory(|m| m.area_rect(popup)).unwrap();
        // Measured on BC4's Portuguese popup: border included, one visible row.
        assert_eq!(compact.size(), vec2(196.0, 20.0));
        let output = frame(&ctx, &mut icons, &mut time, vec![]);
        assert!(!output.shapes.iter().any(|s| {
            matches!(&s.shape, Shape::Text(t) if t.galley.text() == crate::i18n::tr("Salvo automaticamente"))
        }));
        let root = caption(&output, &crate::i18n::tr("Novo"));
        click(&ctx, &mut icons, &mut time, root.center());
        assert_eq!(
            ctx.memory(|m| m.area_rect(popup)).unwrap().size(),
            compact.size()
        );
        let output = frame(&ctx, &mut icons, &mut time, vec![]);
        let root = caption(&output, &crate::i18n::tr("Novo"));
        click(
            &ctx,
            &mut icons,
            &mut time,
            pos2(root.left() - 32.0, root.center().y),
        );
        let expanded = ctx.memory(|m| m.area_rect(popup)).unwrap();
        assert_eq!(expanded.size(), vec2(196.0, 218.0));
        let output = frame(&ctx, &mut icons, &mut time, vec![]);
        let last = caption(&output, &crate::i18n::tr("Comparar Versões"));
        assert!(expanded.contains(last.center()));
    }

    #[test]
    fn native_thirty_row_limit_and_viewport_clipping_keep_scrolling_available() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut home = HomeScreen::new();
        for i in 0..60 {
            home.recent.push(RecentSession::new(
                crate::history::SessionKind::Text,
                format!("left/file-{i:02}.txt").into(),
                format!("right/file-{i:02}.txt").into(),
            ));
        }
        for (height, expected) in [(900.0, 542.0), (450.0, 434.0)] {
            let mut actual = Vec2::ZERO;
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(700.0, height))),
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default().show(ctx, |ui| {
                        actual = home.session_dropdown_size(ui, 66.0);
                    });
                },
            );
            assert_eq!(actual.y, expected);
        }
    }

    #[test]
    fn tree_button_opens_recent_session_with_full_paths_and_closes() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut time = 0.0;
        let record = RecentSession::new(
            SessionKind::Hex,
            "one/source.bin".into(),
            "two/target.bin".into(),
        );
        set_history(&ctx, std::slice::from_ref(&record));
        for _ in 0..2 {
            frame(&ctx, &mut icons, &mut time, vec![]);
        }
        let anchor = ctx
            .data(|d| d.get_temp::<Rect>(Id::new("capture_menu_rect").with("Sessões")))
            .unwrap();
        click(&ctx, &mut icons, &mut time, anchor.center());
        assert!(ctx.memory(|m| m.is_popup_open(Id::new("anycompare_session_picker_popup"))));
        let output = frame(&ctx, &mut icons, &mut time, vec![]);
        let row = caption(&output, &record.label());
        assert!(row.top() >= anchor.bottom());
        click(&ctx, &mut icons, &mut time, row.center());
        match session_menu::take(&ctx) {
            Some(Request::Recent(chosen)) => assert_eq!(chosen, record),
            _ => panic!("expected recent session request"),
        }
        assert!(!ctx.memory(|m| m.is_popup_open(Id::new("anycompare_session_picker_popup"))));
        assert!(session_menu::take(&ctx).is_none());
    }
    #[test]
    fn keyboard_opens_new_session_and_escape_cancels_without_request() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut time = 0.0;
        for _ in 0..2 {
            frame(&ctx, &mut icons, &mut time, vec![]);
        }
        let anchor = ctx
            .data(|d| d.get_temp::<Rect>(Id::new("capture_menu_rect").with("Sessões")))
            .unwrap();
        click(&ctx, &mut icons, &mut time, anchor.center());
        for key in [Key::ArrowDown, Key::ArrowRight, Key::ArrowDown, Key::Enter] {
            frame(
                &ctx,
                &mut icons,
                &mut time,
                vec![Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
            );
            frame(
                &ctx,
                &mut icons,
                &mut time,
                vec![Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
        assert!(matches!(
            session_menu::take(&ctx),
            Some(Request::New(SessionKind::Folder))
        ));
        click(&ctx, &mut icons, &mut time, anchor.center());
        frame(
            &ctx,
            &mut icons,
            &mut time,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(!ctx.memory(|m| m.is_popup_open(Id::new("anycompare_session_picker_popup"))));
        assert!(session_menu::take(&ctx).is_none());
        click(&ctx, &mut icons, &mut time, anchor.center());
        click(&ctx, &mut icons, &mut time, pos2(680.0, 430.0));
        assert!(!ctx.memory(|m| m.is_popup_open(Id::new("anycompare_session_picker_popup"))));
        assert!(session_menu::take(&ctx).is_none());
    }

    #[test]
    fn tree_groups_expand_and_unimplemented_sessions_do_not_open() {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut icons = IconManager::new();
        let mut time = 0.0;
        for _ in 0..2 {
            frame(&ctx, &mut icons, &mut time, vec![]);
        }
        let anchor = ctx
            .data(|d| d.get_temp::<Rect>(Id::new("capture_menu_rect").with("Sessões")))
            .unwrap();
        click(
            &ctx,
            &mut icons,
            &mut time,
            anchor.right_center() - vec2(4.0, 0.0),
        );
        let output = frame(&ctx, &mut icons, &mut time, vec![]);
        let row = caption(&output, &crate::i18n::tr("Novo"));
        click(
            &ctx,
            &mut icons,
            &mut time,
            pos2(row.left() - 32.0, row.center().y),
        );
        assert!(session_menu::take(&ctx).is_none());
        let output = frame(&ctx, &mut icons, &mut time, vec![]);
        let disabled = caption(&output, &crate::i18n::tr("Sincronizar Pastas"));
        click(&ctx, &mut icons, &mut time, disabled.center());
        assert!(session_menu::take(&ctx).is_none());
        let output = frame(&ctx, &mut icons, &mut time, vec![]);
        let text = caption(&output, &crate::i18n::tr("Comparar Textos"));
        click(&ctx, &mut icons, &mut time, text.center());
        assert!(matches!(
            session_menu::take(&ctx),
            Some(Request::New(SessionKind::Text))
        ));
    }
}
