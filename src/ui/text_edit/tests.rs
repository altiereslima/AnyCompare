use super::*;
use crate::ui::dialogs::TestDialogs;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self(std::env::temp_dir().join(format!(
            "anycompare-single-editor-{}-{}.reg",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct Harness {
    ctx: Context,
    screen: TextEditScreen,
    icons: IconManager,
    dialogs: TestDialogs,
    time: f64,
    size: Vec2,
}
impl Harness {
    fn new(text: &str) -> Self {
        let ctx = Context::default();
        super::super::theme::apply(&ctx);
        let mut screen = TextEditScreen::new();
        screen.document.text = text.into();
        let mut h = Self {
            ctx,
            screen,
            icons: IconManager::new(),
            dialogs: TestDialogs::default(),
            time: 0.0,
            size: vec2(900.0, 650.0),
        };
        h.frame(vec![], None);
        h.frame(vec![], None);
        h
    }
    fn frame(&mut self, events: Vec<Event>, action: Option<Action>) -> FullOutput {
        self.time += 0.5;
        self.ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                events,
                time: Some(self.time),
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    if let Some(action) = action {
                        self.screen.run(action, ctx, &mut self.dialogs);
                    }
                    self.screen.ui(ctx, ui, &mut self.icons, &mut self.dialogs);
                });
            },
        )
    }
    fn key(&mut self, key: Key, mut modifiers: Modifiers) -> FullOutput {
        // winit sets the logical Command flag for Ctrl on Windows. TextEdit's
        // native shortcuts use that flag, unlike our explicit Ctrl bindings.
        modifiers.command = modifiers.ctrl;
        self.frame(
            vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            None,
        )
    }
    fn replace(&mut self, value: &str) {
        self.frame(vec![], Some(Action::All));
        self.frame(vec![Event::Text(value.into())], None);
        self.frame(vec![], None);
    }
}

#[test]
fn single_editor_typing_undo_redo_and_word_wrap_preserve_real_lines() {
    let original = "ação ç漢字 ".repeat(80);
    let mut h = Harness::new(&original);
    h.frame(vec![], None);
    let rows_before: usize = h
        .ctx
        .data(|d| d.get_temp(Id::new("text_edit_rows")))
        .unwrap();
    h.screen.options.wrap = true;
    h.frame(vec![], None);
    let wrapped: usize = h
        .ctx
        .data(|d| d.get_temp(Id::new("text_edit_rows")))
        .unwrap();
    assert!(wrapped > rows_before);
    assert_eq!(h.screen.document.text, original);
    h.replace("tradução nova 漢字\nsegunda linha");
    assert_eq!(h.screen.document.text, "tradução nova 漢字\nsegunda linha");
    h.frame(vec![], Some(Action::Undo));
    assert_eq!(h.screen.document.text, original);
    h.frame(vec![], Some(Action::Redo));
    assert_eq!(h.screen.document.text, "tradução nova 漢字\nsegunda linha");
    let saved = h.screen.session_options();
    let mut restored = TextEditScreen::new();
    restored.apply_session_options(&saved);
    assert!(restored.options.wrap);
}

#[test]
fn edited_line_navigation_tracks_real_changes_undo_and_save() {
    let temp = Temp::new();
    std::fs::write(&temp.0, "zero\r\num\r\ndois\r\ntrês\r\nfim\r\n").unwrap();
    let mut h = Harness::new("");
    h.screen.load(&temp.0).unwrap();
    h.frame(vec![], None);
    let go = |h: &mut Harness, line: usize| {
        h.dialogs.inputs.push_back(Some(line.to_string()));
        h.frame(vec![], Some(Action::GoTo));
    };
    assert_eq!(h.screen.edited_targets(&h.ctx), [None, None]);
    for line in [2, 4] {
        go(&mut h, line);
        h.frame(vec![Event::Text("x".into())], None);
    }
    go(&mut h, 1);
    h.frame(vec![], Some(Action::NextEdit(true)));
    assert_eq!(h.screen.cursor_line(&h.ctx), 2);
    h.frame(vec![], Some(Action::NextEdit(true)));
    assert_eq!(h.screen.cursor_line(&h.ctx), 4);
    h.frame(vec![], Some(Action::NextEdit(false)));
    assert_eq!(h.screen.cursor_line(&h.ctx), 2);
    h.frame(vec![], Some(Action::Undo));
    go(&mut h, 1);
    assert_eq!(h.screen.edited_targets(&h.ctx), [Some(2), None]);
    h.frame(vec![], Some(Action::Redo));
    go(&mut h, 3);
    assert_eq!(h.screen.edited_targets(&h.ctx), [Some(4), Some(2)]);
    assert!(h.screen.save(false, &mut h.dialogs));
    assert_eq!(h.screen.edited_targets(&h.ctx), [None, None]);
    assert!(!h.screen.document.dirty());
}

#[test]
fn editor_bookmarks_follow_physical_lines_through_insert_undo_and_wrap() {
    let mut h = Harness::new(&format!("{}\nmarcado\nfim\n", "ação longa ".repeat(80)));
    h.screen.options.wrap = true;
    h.dialogs.inputs.push_back(Some("2:3".into()));
    h.frame(vec![], Some(Action::GoTo));
    h.frame(vec![], Some(Action::ToggleBookmark(7)));
    assert_eq!(h.screen.navigation.bookmarks[7], Some(2));
    h.dialogs.inputs.push_back(Some("1:1".into()));
    h.frame(vec![], Some(Action::GoTo));
    h.frame(vec![Event::Paste("linha inserida\n".into())], None);
    assert_eq!(h.screen.navigation.bookmarks[7], Some(3));
    h.frame(vec![], Some(Action::GoToBookmark(7)));
    assert_eq!(h.screen.cursor_line(&h.ctx), 3);
    h.frame(vec![], Some(Action::Undo));
    assert_eq!(h.screen.navigation.bookmarks[7], Some(2));
    h.frame(vec![], Some(Action::GoToBookmark(7)));
    assert_eq!(h.screen.cursor_line(&h.ctx), 2);
    let before = h.screen.document.text.clone();
    h.frame(vec![], Some(Action::ToggleBookmark(7)));
    assert_eq!(h.screen.navigation.bookmarks[7], None);
    h.frame(vec![], Some(Action::ToggleBookmark(0)));
    h.frame(vec![], Some(Action::ToggleBookmark(9)));
    h.frame(vec![], Some(Action::ClearBookmarks));
    assert_eq!(h.screen.navigation.bookmarks, [None; 10]);
    assert_eq!(h.screen.document.text, before);
}

#[test]
fn single_editor_utf16_registry_save_dirty_icon_and_external_conflict() {
    let file = Temp::new();
    let mut document = Document::default();
    document.text = "Windows Registry Editor Version 5.00\n\n[HKEY_CURRENT_USER\\Software\\EditorTest]\n\"Olá\"=\"ação\"\n".into();
    document.encoding = Encoding::Utf16Le;
    document.line_ending = LineEnding::CrLf;
    document.save(&file.0).unwrap();
    let mut h = Harness::new("");
    h.screen.load(&file.0).unwrap();
    h.frame(vec![], None);
    assert!(!h.screen.document.dirty());
    assert_eq!(
        h.ctx
            .data(|d| d.get_temp::<bool>(Id::new("text_edit_save_enabled").with("imgSave"))),
        Some(false)
    );
    let edited = h.screen.document.text.replace("ação", "tradução 漢字");
    h.replace(&edited);
    assert!(h.screen.document.dirty());
    h.frame(vec![], None);
    assert_eq!(
        h.ctx
            .data(|d| d.get_temp::<bool>(Id::new("text_edit_save_enabled").with("imgSave"))),
        Some(true)
    );
    h.key(Key::S, Modifiers::CTRL);
    let bytes = std::fs::read(&file.0).unwrap();
    assert!(bytes.starts_with(&[255, 254]));
    let decoded = String::from_utf16(
        &bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(decoded.contains("tradução 漢字"));
    assert!(decoded.contains("\r\n"));
    assert!(!h.screen.document.dirty());
    assert!(
        matches!(session_menu::take(&h.ctx), Some(session_menu::Request::SourceSaved(p)) if p == file.0)
    );
    std::fs::write(&file.0, b"external writer").unwrap();
    h.replace("do not overwrite");
    h.key(Key::S, Modifiers::CTRL);
    assert_eq!(std::fs::read(&file.0).unwrap(), b"external writer");
    assert_eq!(h.dialogs.errors.len(), 1);
    assert!(h.screen.document.dirty());
}

#[test]
fn single_editor_read_only_still_selects_and_copies_and_cannot_modify_or_save() {
    let mut h = Harness::new("Seleção ç漢字");
    h.screen.read_only = true;
    h.frame(vec![], None);
    h.frame(vec![], Some(Action::All));
    h.frame(
        vec![
            Event::Text("blocked".into()),
            Event::Paste("blocked".into()),
        ],
        None,
    );
    assert_eq!(h.screen.document.text, "Seleção ç漢字");
    let output = h.frame(vec![], Some(Action::Copy));
    assert_eq!(output.platform_output.copied_text, "Seleção ç漢字");
    h.frame(vec![], Some(Action::Cut));
    h.frame(vec![], Some(Action::Delete));
    assert_eq!(h.screen.document.text, "Seleção ç漢字");
    assert!(!h.screen.save(false, &mut h.dialogs));
    assert!(h.dialogs.files.is_empty());
}

#[test]
fn single_editor_close_cancel_discard_and_save_as_cancel_preserve_buffer() {
    let mut h = Harness::new("unsaved");
    h.dialogs.choices.push_back(SaveChoice::Cancel);
    assert!(!h.screen.confirm_saved(&mut h.dialogs));
    h.dialogs.choices.push_back(SaveChoice::Save);
    h.dialogs.files.push_back(None);
    assert!(!h.screen.confirm_saved(&mut h.dialogs));
    assert_eq!(h.screen.document.text, "unsaved");
    assert!(h.screen.document.dirty());
    h.dialogs.choices.push_back(SaveChoice::Discard);
    assert!(h.screen.confirm_saved(&mut h.dialogs));
    assert_eq!(h.screen.document.text, "unsaved");
}

#[test]
fn single_editor_ansi_unicode_warning_cancel_and_utf8_save_as_are_lossless() {
    let file = Temp::new();
    std::fs::write(&file.0, b"caf\xe9\r\n").unwrap();
    let saved_as = Temp::new();
    let mut h = Harness::new("");
    h.screen.load(&file.0).unwrap();
    h.frame(vec![], None);
    assert_eq!(h.screen.document.encoding, Encoding::Windows1252);
    h.replace("café 漢字\n");
    h.dialogs.field_answers.push_back(None);
    assert!(!h.screen.save(false, &mut h.dialogs));
    assert_eq!(h.screen.document.encoding, Encoding::Windows1252);
    assert_eq!(std::fs::read(&file.0).unwrap(), b"caf\xe9\r\n");
    h.dialogs.files.push_back(Some(saved_as.0.clone()));
    h.dialogs.field_answers.push_back(Some(vec![
        DialogField::Notice(String::new()),
        DialogField::Choice(String::new(), 0, vec![]),
    ]));
    assert!(h.screen.save(true, &mut h.dialogs));
    assert_eq!(Document::load(&saved_as.0).unwrap().text, "café 漢字\n");
    assert_eq!(std::fs::read(&file.0).unwrap(), b"caf\xe9\r\n");
}

#[test]
fn single_editor_saved_registry_refreshes_properties_without_reopening_the_session() {
    let file = Temp::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/registry");
    std::fs::copy(root.join("left.reg"), &file.0).unwrap();
    let mut registry = super::super::version_compare::VersionCompareScreen::for_kind(
        crate::history::SessionKind::Registry,
    );
    registry
        .load_files(&file.0, &root.join("right.reg"))
        .unwrap();
    let before = registry.record_paths();
    let options = registry.session_options();
    let mut h = Harness::new("");
    h.screen.load(&file.0).unwrap();
    h.frame(vec![], None);
    let changed = h.screen.document.text.replace("Padrão A", "Padrão B");
    h.replace(&changed);
    assert!(h.screen.save(false, &mut h.dialogs));
    registry.refresh_source(&h.screen.take_saved_source().unwrap(), &mut h.dialogs);
    assert_eq!(registry.record_paths(), before);
    assert_eq!(registry.session_options(), options);
    assert!(h.dialogs.errors.is_empty());
    let loaded =
        crate::property_sources::load(crate::history::SessionKind::Registry, &file.0).unwrap();
    fn contains(nodes: &[crate::pe_version::Node]) -> bool {
        nodes
            .iter()
            .any(|n| n.value.contains("Padrão B") || contains(&n.children))
    }
    assert!(contains(&loaded.nodes));
}

#[test]
fn single_editor_right_click_opens_real_menu_and_copies_selected_unicode() {
    let mut h = Harness::new("texto ç漢字");
    h.frame(vec![], Some(Action::All));
    let rect: Rect = h
        .ctx
        .data(|d| d.get_temp(Id::new("text_edit_rect")))
        .unwrap();
    let pos = rect.min + vec2(50.0, 6.0);
    for pressed in [true, false] {
        h.frame(
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Secondary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            None,
        );
    }
    h.frame(vec![], None);
    assert!(h.ctx.is_context_menu_open());
    let (copy, enabled): (Rect, bool) = h
        .ctx
        .data(|d| d.get_temp(Id::new("text_edit_context").with("common.copy  Ctrl+C")))
        .unwrap();
    assert!(enabled);
    let pos = copy.center();
    h.frame(
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        None,
    );
    let output = h.frame(
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
        None,
    );
    assert_eq!(output.platform_output.copied_text, "texto ç漢字");
    assert!(!h.ctx.is_context_menu_open());
}

fn shape_text(shape: &egui::epaint::Shape, result: &mut Vec<String>) {
    match shape {
        egui::epaint::Shape::Text(text) => result.push(text.galley.job.text.clone()),
        egui::epaint::Shape::Vec(children) => {
            for child in children {
                shape_text(child, result);
            }
        }
        _ => {}
    }
}

#[test]
#[ignore = "Run English UI checks in their own process to avoid changing concurrent tests' locale"]
fn english_hidden_single_editor_menus_and_wrap_caption_are_translated() {
    crate::i18n::initialize(Some("en"));
    for side in [None, Some(true), Some(false)] {
        let question =
            super::super::dialogs::save_question(side, Some(Path::new("C:/test/ação.reg")));
        assert!(question.starts_with("Save changes"), "{question}");
        assert!(question.contains("ação.reg"));
        assert!(!question.contains("Salvar"));
        assert!(
            !question.contains("esquerdo") && !question.contains("direito"),
            "{question}"
        );
    }
    let mut h = Harness::new("document content");
    let output = h.frame(vec![], None);
    let mut text = Vec::new();
    for shape in output.shapes {
        shape_text(&shape.shape, &mut text);
    }
    for expected in [
        "Session", "Edit", "View", "Tools", "Undo", "Redo", "Cut", "Copy", "Paste", "Delete",
        "Syntax", "Font",
    ] {
        assert!(
            text.iter().any(|s| s == expected),
            "missing English UI caption {expected}: {text:?}"
        );
    }
    let view: Rect = h
        .ctx
        .data(|d| d.get_temp(Id::new("capture_menu_rect").with("menu.view")))
        .unwrap();
    for pressed in [true, false] {
        let pos = view.center();
        h.frame(
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            None,
        );
    }
    let output = h.frame(vec![], None);
    text.clear();
    for shape in output.shapes {
        shape_text(&shape.shape, &mut text);
    }
    assert!(text.iter().any(|s| s == "Word Wrap"));
    assert!(!text.iter().any(|s| s == "Quebra automática de linha"));

    h.key(Key::Escape, Modifiers::NONE);
    let search: Rect = h
        .ctx
        .data(|d| d.get_temp(Id::new("capture_menu_rect").with("menu.search")))
        .unwrap();
    for pressed in [true, false] {
        let pos = search.center();
        h.frame(
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            None,
        );
    }
    let output = h.frame(vec![], None);
    text.clear();
    for shape in output.shapes {
        shape_text(&shape.shape, &mut text);
    }
    for label in [
        "Next Edit",
        "Previous Edit",
        "Toggle Bookmark",
        "Go To Bookmark",
        "Clear Bookmarks",
    ] {
        assert!(text.iter().any(|s| s == label), "missing {label}: {text:?}");
    }
    h.key(Key::Escape, Modifiers::NONE);
    h.key(Key::H, Modifiers::CTRL);
    let output = h.frame(vec![], None);
    text.clear();
    for shape in output.shapes {
        shape_text(&shape.shape, &mut text);
    }
    for expected in [
        "Find",
        "Replace:",
        "Replace",
        "Replace All",
        "Previous",
        "Next",
    ] {
        assert!(
            text.iter().any(|s| s == expected),
            "missing search caption {expected}: {text:?}"
        );
    }
    h.screen.search.options.regular_expression = true;
    h.screen.search.options.query = "(".into();
    h.key(Key::F3, Modifiers::NONE);
    assert!(h
        .screen
        .search
        .message
        .as_ref()
        .unwrap()
        .starts_with("Invalid expression:"));
}

#[test]
fn single_editor_search_shortcuts_keep_query_input_out_of_document_and_navigate_unicode() {
    let original = "zero ação aqui\nAÇÃO depois";
    let mut h = Harness::new(original);
    h.key(Key::F, Modifiers::CTRL);
    assert!(h.screen.search.shown);
    h.frame(vec![Event::Text("ação".into())], None);
    assert_eq!(h.screen.search.options.query, "ação");
    assert_eq!(h.screen.document.text, original);
    h.key(Key::Enter, Modifiers::NONE);
    let first = h.screen.selected_bytes(&h.ctx);
    assert_eq!(&h.screen.document.text[first.clone()], "ação");
    h.key(Key::F3, Modifiers::NONE);
    let second = h.screen.selected_bytes(&h.ctx);
    assert_eq!(&h.screen.document.text[second.clone()], "AÇÃO");
    assert!(second.start > first.start);
    h.key(Key::F3, Modifiers::SHIFT);
    assert_eq!(h.screen.selected_bytes(&h.ctx), first);
    h.key(Key::Escape, Modifiers::NONE);
    assert!(!h.screen.search.shown);
    assert_eq!(h.screen.document.text, original);
    assert_eq!(h.screen.take_recent_search(), Some("ação".into()));
    assert!(h.screen.take_recent_search().is_none());
}

#[test]
fn single_editor_replace_all_literal_and_regex_are_each_one_undo_and_preserve_redo_rules() {
    let original = "ação $1\nAÇÃO $1\ná=1\nβ=2";
    let mut h = Harness::new(original);
    h.screen.search.options.query = "ação".into();
    h.screen.search.replacement = "$2 漢字".into();
    h.frame(vec![], Some(Action::ReplaceAll));
    assert_eq!(h.screen.document.text, "$2 漢字 $1\n$2 漢字 $1\ná=1\nβ=2");
    h.frame(vec![], Some(Action::Undo));
    assert_eq!(h.screen.document.text, original);
    h.frame(vec![], Some(Action::Redo));
    assert!(h.screen.document.text.starts_with("$2 漢字"));
    h.frame(vec![], Some(Action::Undo));
    h.screen.search.options.query = "^([^=\\n]+)=(\\d+)$".into();
    h.screen.search.options.regular_expression = true;
    h.screen.search.replacement = "$2:$1".into();
    h.frame(vec![], Some(Action::ReplaceAll));
    assert_eq!(h.screen.document.text, "ação $1\nAÇÃO $1\n1:á\n2:β");
    assert!(
        !h.screen.undo_flags.1,
        "new replacement must invalidate previous Redo"
    );
    h.frame(vec![], Some(Action::Undo));
    assert_eq!(h.screen.document.text, original);
    h.frame(vec![], Some(Action::Redo));
    assert!(h.screen.document.text.ends_with("1:á\n2:β"));
}

#[test]
fn single_editor_replace_current_advances_and_invalid_or_read_only_replace_never_changes_data() {
    let mut h = Harness::new("ç café café");
    h.screen.search.options.query = "café".into();
    h.screen.search.replacement = "漢".into();
    h.frame(vec![], Some(Action::FindNext(true)));
    h.frame(vec![], Some(Action::Replace));
    assert_eq!(h.screen.document.text, "ç 漢 café");
    let range = h.screen.selected_bytes(&h.ctx);
    assert_eq!(&h.screen.document.text[range], "café");
    h.frame(vec![], Some(Action::Replace));
    assert_eq!(h.screen.document.text, "ç 漢 漢");
    let before = h.screen.document.text.clone();
    h.screen.search.options.regular_expression = true;
    h.screen.search.options.query = "[".into();
    h.frame(vec![], Some(Action::ReplaceAll));
    assert_eq!(h.screen.document.text, before);
    assert!(h
        .screen
        .search
        .message
        .as_ref()
        .unwrap()
        .contains(&crate::i18n::tr("text_edit.invalid_expression")));
    h.screen.read_only = true;
    h.screen.search.options.regular_expression = false;
    h.screen.search.options.query = "漢".into();
    h.frame(vec![], Some(Action::FindNext(true)));
    h.frame(vec![], Some(Action::Replace));
    h.frame(vec![], Some(Action::ReplaceAll));
    assert_eq!(h.screen.document.text, before);
}

#[test]
fn single_editor_zero_length_regex_advances_by_unicode_boundaries_and_respects_no_wrap() {
    let mut h = Harness::new("é\n漢");
    #[cfg(target_os = "macos")]
    {
        h.key(Key::ArrowUp, Modifiers::NONE);
        h.key(Key::ArrowLeft, Modifiers::NONE);
    }
    #[cfg(not(target_os = "macos"))]
    h.key(Key::Home, Modifiers::CTRL);
    assert_eq!(h.screen.selected_bytes(&h.ctx), 0..0);
    h.screen.search.options.query = "^|$".into();
    h.screen.search.options.regular_expression = true;
    h.screen.search.wrap = false;
    for at in [0, 2, 3, 6] {
        h.frame(vec![], Some(Action::FindNext(true)));
        assert_eq!(h.screen.selected_bytes(&h.ctx), at..at);
    }
    h.frame(vec![], Some(Action::FindNext(true)));
    assert!(h.screen.search.message.is_some());
    assert_eq!(h.screen.document.text, "é\n漢");
    h.screen.search.options.query = "^".into();
    h.screen.search.replacement = ">".into();
    h.frame(vec![], Some(Action::ReplaceAll));
    assert_eq!(h.screen.document.text, ">é\n>漢");
    h.frame(vec![], Some(Action::Undo));
    assert_eq!(h.screen.document.text, "é\n漢");
}

#[test]
fn single_editor_find_and_goto_scroll_real_caret_with_wrap_without_adding_newlines() {
    let original = (0..200)
        .map(|i| format!("linha {i} {}\n", "palavra ".repeat(20)))
        .collect::<String>()
        + "última 漢字\n";
    let mut h = Harness::new(&original);
    h.screen.options.wrap = true;
    h.screen.search.options.query = "漢字".into();
    h.frame(vec![], Some(Action::FindNext(true)));
    h.frame(vec![], None);
    h.frame(vec![], None);
    let offset: Vec2 = h
        .ctx
        .data(|d| d.get_temp(Id::new("text_edit_scroll_offset")))
        .unwrap();
    assert!(offset.y > 0.0);
    let caret: Rect = h
        .ctx
        .data(|d| d.get_temp(Id::new("text_edit_caret_rect")))
        .unwrap();
    let viewport: Rect = h
        .ctx
        .data(|d| d.get_temp(Id::new("text_edit_viewport")))
        .unwrap();
    assert!(
        viewport.expand(5.0).intersects(caret),
        "caret {caret:?} viewport {viewport:?}"
    );
    h.dialogs.inputs.push_back(Some("1:3".into()));
    h.key(Key::G, Modifiers::CTRL);
    h.frame(vec![], None);
    assert_eq!(h.screen.selected_bytes(&h.ctx), 2..2);
    h.dialogs.inputs.push_back(Some("0:1".into()));
    h.key(Key::G, Modifiers::CTRL);
    assert_eq!(h.dialogs.errors.len(), 1);
    assert_eq!(h.screen.document.text, original);
}

#[test]
fn single_editor_replace_strip_clicks_edit_only_document_and_disabled_buttons_do_nothing() {
    let mut h = Harness::new("café café");
    h.key(Key::H, Modifiers::CTRL);
    h.frame(vec![Event::Text("café".into())], None);
    fn click(h: &mut Harness, rect: Rect) {
        for pressed in [true, false] {
            let pos = rect.center();
            h.frame(
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                None,
            );
        }
    }
    let field: Rect = h
        .ctx
        .data(|d| d.get_temp(Id::new("text_edit_replace_rect")))
        .unwrap();
    click(&mut h, field);
    h.frame(vec![Event::Text("chá 漢".into())], None);
    assert_eq!(h.screen.search.replacement, "chá 漢");
    assert_eq!(h.screen.document.text, "café café");
    let (button, enabled): (Rect, bool) = h
        .ctx
        .data(|d| d.get_temp(Id::new("text_edit_find_action").with("text.replace_all")))
        .unwrap();
    assert!(enabled);
    click(&mut h, button);
    assert_eq!(h.screen.document.text, "chá 漢 chá 漢");
    h.frame(vec![], Some(Action::Undo));
    assert_eq!(h.screen.document.text, "café café");
    h.screen.read_only = true;
    h.frame(vec![], None);
    let (button, enabled): (Rect, bool) = h
        .ctx
        .data(|d| d.get_temp(Id::new("text_edit_find_action").with("text.replace_all")))
        .unwrap();
    assert!(!enabled);
    click(&mut h, button);
    assert_eq!(h.screen.document.text, "café café");
}

#[test]
fn single_editor_find_and_goto_do_not_consume_the_first_undo_of_an_edit() {
    let mut h = Harness::new("antigo antigo");
    h.screen.search.options.query = "antigo".into();
    h.screen.search.replacement = "novo".into();
    h.frame(vec![], Some(Action::ReplaceAll));
    h.screen.search.options.query = "novo".into();
    h.frame(vec![], Some(Action::FindNext(true)));
    h.frame(vec![], None);
    h.frame(vec![], None);
    h.dialogs.inputs.push_back(Some("1:2".into()));
    h.key(Key::G, Modifiers::CTRL);
    h.frame(vec![], None);
    h.frame(vec![], None);
    h.key(Key::Z, Modifiers::CTRL);
    assert_eq!(h.screen.document.text, "antigo antigo");
}

#[test]
fn single_editor_menu_delete_removes_one_unicode_character_and_undo_survives_many_searches() {
    let mut h = Harness::new("ç漢字 café");
    h.dialogs.inputs.push_back(Some("1:2".into()));
    h.key(Key::G, Modifiers::CTRL);
    h.frame(vec![], Some(Action::Delete));
    assert_eq!(h.screen.document.text, "ç字 café");
    h.screen.search.options.query = "café".into();
    for _ in 0..110 {
        h.frame(vec![], Some(Action::FindNext(true)));
    }
    h.key(Key::Z, Modifiers::CTRL);
    assert_eq!(h.screen.document.text, "ç漢字 café");
    h.key(Key::Y, Modifiers::CTRL);
    assert_eq!(h.screen.document.text, "ç字 café");
}

#[test]
fn single_editor_search_strip_fits_supported_widths_and_query_undo_does_not_edit_document() {
    for width in [700.0, 1153.0, 1919.0] {
        let mut h = Harness::new("documento original");
        h.size = vec2(width, 720.0);
        h.key(Key::H, Modifiers::CTRL);
        h.frame(vec![], None);
        let query: Rect = h
            .ctx
            .data(|d| d.get_temp(Id::new("text_edit_query_rect")))
            .unwrap();
        let (button, _): (Rect, bool) = h
            .ctx
            .data(|d| d.get_temp(Id::new("text_edit_find_action").with("text.replace_all")))
            .unwrap();
        assert!(
            query.left() >= 0.0 && query.right() < width && button.right() <= width,
            "width {width} query {query:?} replace {button:?}"
        );
        h.frame(vec![Event::Text("procura".into())], None);
        h.frame(vec![], None);
        h.key(Key::Z, Modifiers::CTRL);
        assert!(h.screen.search.options.query.is_empty());
        assert_eq!(h.screen.document.text, "documento original");
    }
}

#[test]
fn single_editor_text_starts_cleanly_to_the_right_of_the_gutter() {
    let mut h = Harness::new("# Configuracao do Projeto\nhost = localhost\n");
    h.frame(vec![], None);
    let rect: Rect = h
        .ctx
        .data(|d| d.get_temp(Id::new("text_edit_rect")))
        .unwrap();
    let viewport: Rect = h
        .ctx
        .data(|d| d.get_temp(Id::new("text_edit_viewport")))
        .unwrap();
    assert!(
        rect.left() >= viewport.left() + 34.0,
        "text rect {rect:?} must sit strictly to the right of the 30px gutter in {viewport:?}"
    );
}
