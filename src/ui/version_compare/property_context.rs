//! Context commands for read-only property comparisons. Keep the picked pane
//! explicit: opening a menu on the right must never act on the left source.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    Copy,
    Important,
    Open,
    OpenDefault,
    Explore(bool),
    EditText,
}

pub(super) struct Target {
    pub kind: crate::history::SessionKind,
    pub side: usize,
    pub important: bool,
    pub loaded: bool,
}

pub(super) fn menu(
    ui: &mut Ui,
    icons: &mut IconManager,
    row: &Row,
    target: Target,
) -> Option<Command> {
    use crate::history::SessionKind;
    let Target {
        kind,
        side,
        important,
        loaded,
    } = target;
    let mut command = None;
    ui.set_min_width(if kind == SessionKind::Registry {
        230.0
    } else {
        200.0
    });
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.spacing_mut().button_padding = vec2(4.0, 1.0);
    ui.spacing_mut().interact_size.y = 20.0;
    let present = row.names[side].is_some();
    if kind == SessionKind::Registry {
        item(
            ui,
            icons,
            if side == 0 {
                "Copiar para Direita  Ctrl+R"
            } else {
                "Copiar para Esquerda  Ctrl+L"
            },
            false,
            "copy-to-other",
        );
        ui.separator();
    }
    if item(ui, icons, "Copiar  Ctrl+C", present, "copy").clicked() {
        command = Some(Command::Copy);
    }
    if kind == SessionKind::Registry {
        for label in [
            "Excluir",
            "Renomear  F2",
            "Nova Chave",
            "Novo Valor  Ins",
            "Modificar...",
        ] {
            item(ui, icons, label, false, label);
        }
    } else if !row.group && present {
        ui.separator();
        let label = format!(
            "\"{}\" {}",
            property_name(kind, &row.key, row.names[side].as_deref().unwrap_or("")),
            tr("é importante")
        );
        let response = ui.selectable_label(important, label);
        ui.data_mut(|d| {
            d.insert_temp(
                Id::new("property_context_action").with("important"),
                response.rect,
            )
        });
        if response.clicked() {
            command = Some(Command::Important);
        }
    }
    ui.separator();
    if item(ui, icons, "Abrir Arquivo...  Ctrl+O", true, "open").clicked() {
        command = Some(Command::Open);
    }
    if kind == SessionKind::Media {
        item(ui, icons, "Reproduzir", false, "play");
    }
    ui.add_enabled_ui(loaded, |ui| {
        chrome::menu_button(ui, "Abrir Com", |ui| {
            if kind == SessionKind::Registry
                && item(ui, icons, "Editor de Texto", true, "text-edit").clicked()
            {
                command = Some(Command::EditText);
            }
            if item(ui, icons, "Aplicativo padrão", true, "default").clicked() {
                command = Some(Command::OpenDefault);
            }
        });
        chrome::menu_button(ui, "Explorer", |ui| {
            for (label, select) in [("Selecionar no Explorer", true), ("Abrir pasta", false)] {
                if item(
                    ui,
                    icons,
                    label,
                    true,
                    if select { "explore" } else { "folder" },
                )
                .clicked()
                {
                    command = Some(Command::Explore(select));
                }
            }
        });
    });
    if command.is_some() {
        ui.close_menu();
    }
    command
}

fn item(ui: &mut Ui, icons: &mut IconManager, label: &str, enabled: bool, key: &str) -> Response {
    let response = chrome::menu_item(ui, icons, label, enabled);
    ui.data_mut(|d| d.insert_temp(Id::new("property_context_action").with(key), response.rect));
    #[cfg(test)]
    ui.data_mut(|d| {
        d.insert_temp(
            Id::new("property_context_enabled").with(key),
            enabled && ui.is_enabled(),
        )
    });
    response
}

impl VersionCompareScreen {
    pub(super) fn context_command(
        &mut self,
        command: Command,
        ctx: &Context,
        dialogs: &mut dyn DialogHost,
    ) {
        match command {
            Command::Copy => self.copy(ctx),
            Command::Open => self.pick(self.active_side == 0, dialogs),
            Command::Important => {
                let Some(row) = self
                    .rows
                    .get(self.selected)
                    .filter(|r| !r.group && r.names[self.active_side].is_some())
                else {
                    return;
                };
                let key = row.key.clone();
                let ancestors: Vec<_> = self
                    .options
                    .ignored
                    .iter()
                    .filter(|ignored| key == **ignored || key.starts_with(&format!("{ignored}/")))
                    .cloned()
                    .collect();
                if ancestors.is_empty() {
                    self.options.ignored.push(key);
                } else {
                    self.options
                        .ignored
                        .retain(|ignored| !ancestors.contains(ignored));
                    // Enabling one leaf under an ignored group must not enable
                    // every sibling as a side effect.
                    for row in &self.rows {
                        if !row.group
                            && row.key != key
                            && ancestors
                                .iter()
                                .any(|a| row.key.starts_with(&format!("{a}/")))
                            && !self.options.ignored.contains(&row.key)
                        {
                            self.options.ignored.push(row.key.clone());
                        }
                    }
                }
                self.compare();
            }
            Command::OpenDefault | Command::Explore(_) | Command::EditText => {
                let source = if self.active_side == 0 {
                    &self.left
                } else {
                    &self.right
                };
                let Some(path) = source.as_ref().and_then(|image| image.path.as_deref()) else {
                    return;
                };
                if command == Command::EditText {
                    session_menu::request(ctx, session_menu::Request::EditText(path.to_owned()));
                    return;
                }
                let result = match command {
                    Command::Explore(false) => {
                        dialogs.open_path(path.parent().unwrap_or(path), false)
                    }
                    Command::Explore(true) => dialogs.open_path(path, true),
                    _ => dialogs.open_path(path, false),
                };
                if let Err(error) = result {
                    dialogs.error(&error);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{history::SessionKind, ui::dialogs::TestDialogs};

    struct Harness {
        screen: VersionCompareScreen,
        ctx: Context,
        icons: IconManager,
        dialogs: TestDialogs,
        time: f64,
    }
    impl Harness {
        fn new(kind: SessionKind) -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"));
            let (a, b) = match kind {
                SessionKind::Media => {
                    let media =
                        root.join("vendor/bc5-prototypes/RUST_APP/crates/bc5-media/tests/fixtures");
                    (media.join("A.mp3"), media.join("B.mp3"))
                }
                SessionKind::Registry => {
                    let reg = root.join("vendor/bc5-prototypes/TESTES_REGISTRY");
                    (
                        reg.join("02_diferencas_esquerda.reg"),
                        reg.join("02_diferencas_direita.reg"),
                    )
                }
                _ => (
                    root.join("tests/fixtures/bc5/pe32.bin"),
                    root.join("tests/fixtures/bc5/pe32-changed.bin"),
                ),
            };
            let mut screen = VersionCompareScreen::for_kind(kind);
            screen.load_files(&a, &b).unwrap();
            let ctx = Context::default();
            crate::ui::theme::apply(&ctx);
            let mut h = Self {
                screen,
                ctx,
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
            self.ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1150.0, 720.0))),
                    time: Some(self.time),
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
        fn click(&mut self, pos: Pos2, button: PointerButton) -> FullOutput {
            self.frame(vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ]);
            self.frame(vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ])
        }
        fn row(&self, index: usize, side: usize) -> Rect {
            self.ctx
                .data(|d| d.get_temp(Id::new("property_test_row").with((index, side))))
                .unwrap()
        }
        fn open(&mut self, index: usize, side: usize) {
            self.click(self.row(index, side).center(), PointerButton::Secondary);
            self.frame(vec![]);
            assert_eq!(
                (self.screen.selected, self.screen.active_side),
                (index, side)
            );
            assert!(self.ctx.is_context_menu_open());
        }
        fn action(&mut self, key: &str) -> FullOutput {
            let rect: Rect = self
                .ctx
                .data(|d| d.get_temp(Id::new("property_context_action").with(key)))
                .unwrap();
            self.click(rect.center(), PointerButton::Primary)
        }
        fn escape(&mut self) {
            self.frame(vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]);
            self.frame(vec![]);
            assert!(!self.ctx.is_context_menu_open());
        }
        fn submenu(&mut self, label: &str) {
            let rect: Rect = self
                .ctx
                .data(|d| d.get_temp(Id::new("capture_menu_rect").with(label)))
                .unwrap();
            self.click(rect.center(), PointerButton::Primary);
            self.frame(vec![]);
            self.frame(vec![]);
        }
        fn enabled(&self, key: &str) -> bool {
            self.ctx
                .data(|d| d.get_temp(Id::new("property_context_enabled").with(key)))
                .unwrap()
        }
    }

    #[test]
    fn property_context_copy_uses_the_clicked_row_and_side_in_all_three_comparisons() {
        for kind in [
            SessionKind::Media,
            SessionKind::Registry,
            SessionKind::Version,
        ] {
            for side in [0, 1] {
                let mut h = Harness::new(kind);
                let index = h
                    .screen
                    .rows
                    .iter()
                    .position(|r| {
                        !r.group && r.change == Change::Important && r.names[side].is_some()
                    })
                    .unwrap();
                let row = &h.screen.rows[index];
                let expected = property_value(kind, &row.key, row.values[side].as_deref().unwrap());
                let source_paths = h.screen.record_paths();
                h.open(index, side);
                let output = h.action("copy");
                assert_eq!(
                    output.platform_output.copied_text, expected,
                    "{kind:?} side {side}"
                );
                assert!(!h.ctx.is_context_menu_open());
                assert_eq!(h.screen.record_paths(), source_paths);
                assert!(h.dialogs.errors.is_empty());
            }
        }
    }

    #[test]
    fn property_context_escape_preserves_comparison_and_open_uses_picked_pane() {
        for kind in [
            SessionKind::Media,
            SessionKind::Registry,
            SessionKind::Version,
        ] {
            let mut h = Harness::new(kind);
            let index = h
                .screen
                .rows
                .iter()
                .position(|r| !r.group && r.names[1].is_some())
                .unwrap();
            let paths = h.screen.record_paths();
            let options = h.screen.session_options();
            h.open(index, 1);
            h.escape();
            assert_eq!(h.screen.record_paths(), paths);
            assert_eq!(h.screen.session_options(), options);
            // Use the left fixture as a replacement, then verify only the
            // right source changed through the real context-menu click.
            h.dialogs
                .files
                .push_back(Some(paths.as_ref().unwrap().0.clone()));
            h.open(index, 1);
            h.action("open");
            let updated = h.screen.record_paths().unwrap();
            assert_eq!(updated.0, paths.unwrap().0);
            assert_eq!(updated.1, updated.0);
            assert!(h.dialogs.files.is_empty());
        }
    }

    #[test]
    fn media_context_importance_changes_only_the_clicked_field_and_roundtrips() {
        let mut h = Harness::new(SessionKind::Media);
        let index = h
            .screen
            .rows
            .iter()
            .position(|r| !r.group && r.change == Change::Important)
            .unwrap();
        let key = h.screen.rows[index].key.clone();
        let paths = h.screen.record_paths();
        h.open(index, 1);
        h.action("important");
        assert_eq!(h.screen.rows[index].change, Change::Minor);
        assert!(h.screen.options.ignored.contains(&key));
        h.frame(vec![]);
        h.open(index, 0);
        h.action("important");
        assert_eq!(h.screen.rows[index].change, Change::Important);
        assert!(!h.screen.options.ignored.contains(&key));
        assert_eq!(h.screen.record_paths(), paths);
    }

    #[test]
    fn property_context_submenus_dispatch_the_picked_file_without_launching_external_apps() {
        for kind in [
            SessionKind::Media,
            SessionKind::Registry,
            SessionKind::Version,
        ] {
            for side in [0, 1] {
                for (label, action, folder, select) in [
                    ("Abrir Com", "default", false, false),
                    ("Explorer", "explore", false, true),
                    ("Explorer", "folder", true, false),
                ] {
                    let mut h = Harness::new(kind);
                    let index = h
                        .screen
                        .rows
                        .iter()
                        .position(|r| !r.group && r.names[side].is_some())
                        .unwrap();
                    let paths = h.screen.record_paths().unwrap();
                    let path = if side == 0 { &paths.0 } else { &paths.1 };
                    let before = bc5_media::SourceFingerprint::capture(path).unwrap();
                    let expected = if folder {
                        path.parent().unwrap().to_owned()
                    } else {
                        path.clone()
                    };
                    h.open(index, side);
                    h.submenu(label);
                    assert!(h.enabled(action));
                    h.action(action);
                    assert_eq!(
                        h.dialogs.opened,
                        vec![expected],
                        "{kind:?} side {side} action {action}"
                    );
                    assert_eq!(h.dialogs.opened_selection, vec![select]);
                    assert_eq!(h.screen.record_paths().as_ref(), Some(&paths));
                    assert!(!h.ctx.is_context_menu_open());
                    before.verify_current(path).unwrap();
                }
            }
        }
    }

    #[test]
    fn property_context_missing_side_and_unimplemented_actions_cannot_execute() {
        let mut h = Harness::new(SessionKind::Registry);
        let index = h
            .screen
            .rows
            .iter()
            .position(|r| !r.group && r.names[1].is_none())
            .unwrap();
        let before = h.screen.record_paths();
        h.open(index, 1);
        for key in [
            "copy",
            "copy-to-other",
            "Excluir",
            "Renomear  F2",
            "Nova Chave",
            "Novo Valor  Ins",
            "Modificar...",
        ] {
            assert!(!h.enabled(key), "unexpectedly enabled: {key}");
        }
        assert!(h.action("copy").platform_output.copied_text.is_empty());
        h.submenu("Abrir Com");
        assert!(h.enabled("text-edit"));
        h.action("text-edit");
        assert!(!h.ctx.is_context_menu_open());
        assert!(h.dialogs.opened.is_empty());
        assert!(matches!(
            session_menu::take(&h.ctx),
            Some(session_menu::Request::EditText(_))
        ));
        assert_eq!(h.screen.record_paths(), before);

        let mut media = Harness::new(SessionKind::Media);
        let index = media.screen.rows.iter().position(|r| !r.group).unwrap();
        media.open(index, 0);
        assert!(!media.enabled("play"));
        media.action("play");
        assert!(media.dialogs.opened.is_empty());
        media.escape();
    }

    #[test]
    fn property_context_right_click_group_does_not_fold_and_click_outside_closes() {
        for kind in [
            SessionKind::Media,
            SessionKind::Registry,
            SessionKind::Version,
        ] {
            let mut h = Harness::new(kind);
            let index = h.screen.rows.iter().position(|r| r.group).unwrap();
            let before = h.screen.session_options();
            h.open(index, 0);
            assert_eq!(h.screen.session_options(), before);
            h.click(pos2(1100.0, 670.0), PointerButton::Primary);
            h.frame(vec![]);
            assert!(!h.ctx.is_context_menu_open());
            assert_eq!(h.screen.session_options(), before);
        }
    }

    #[test]
    fn property_context_english_captions_are_inside_the_translation_catalog() {
        let catalog: crate::i18n::Catalog =
            serde_json::from_str(include_str!("../../../assets/locales/en.json")).unwrap();
        for (label, expected) in [
            ("Editor de Texto", "Text Editor"),
            ("é importante", "is important"),
            ("Reproduzir", "Play"),
            ("Metadados", "Metadata"),
        ] {
            assert_eq!(
                catalog.translations.get(label).map(String::as_str),
                Some(expected)
            );
        }
    }

    #[test]
    fn registry_context_text_editor_requests_the_clicked_side_and_never_imports_the_registry() {
        for side in [0, 1] {
            let mut h = Harness::new(SessionKind::Registry);
            let index = h
                .screen
                .rows
                .iter()
                .position(|r| !r.group && r.names[side].is_some())
                .unwrap();
            let paths = h.screen.record_paths().unwrap();
            let expected = if side == 0 {
                paths.0.clone()
            } else {
                paths.1.clone()
            };
            let guard = bc5_media::SourceFingerprint::capture(&expected).unwrap();
            h.open(index, side);
            h.submenu("Abrir Com");
            h.action("text-edit");
            assert!(
                matches!(session_menu::take(&h.ctx), Some(session_menu::Request::EditText(path)) if path == expected)
            );
            assert!(h.dialogs.opened.is_empty());
            assert_eq!(h.screen.record_paths(), Some(paths));
            guard.verify_current(&expected).unwrap();
        }
    }

    #[test]
    #[ignore = "Run English UI checks in their own process to avoid changing concurrent tests' locale"]
    fn english_hidden_registry_open_with_text_editor_is_rendered_and_dispatches() {
        crate::i18n::initialize(Some("en"));
        let mut h = Harness::new(SessionKind::Registry);
        let index = h.screen.rows.iter().position(|r| !r.group).unwrap();
        let path = h.screen.record_paths().unwrap().0;
        h.open(index, 0);
        h.submenu("Abrir Com");
        let output = h.frame(vec![]);
        fn text(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(t) => out.push(t.galley.job.text.clone()),
                egui::epaint::Shape::Vec(children) => {
                    for child in children {
                        text(child, out);
                    }
                }
                _ => {}
            }
        }
        let mut labels = Vec::new();
        for shape in output.shapes {
            text(&shape.shape, &mut labels);
        }
        assert!(labels.iter().any(|s| s == "Open With"));
        assert!(labels.iter().any(|s| s == "Text Editor"));
        assert!(!labels.iter().any(|s| s == "Editor de Texto"));
        h.action("text-edit");
        assert!(
            matches!(session_menu::take(&h.ctx), Some(session_menu::Request::EditText(p)) if p == path)
        );
    }

    #[test]
    #[ignore = "Read-only test of the user's authorized M4A/MP4 samples; no native windows"]
    fn property_context_real_user_media_is_read_only() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let cases: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join(".codex/bc5-ui-audit/real-media-cases.json"))
                .unwrap(),
        )
        .unwrap();
        let case = cases
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "user-m4a")
            .unwrap();
        let pairs = [
            (
                PathBuf::from(case["left"].as_str().unwrap()),
                PathBuf::from(case["right"].as_str().unwrap()),
            ),
            (
                PathBuf::from(
                    "C:/Users/Altieres/Music/Amy Winehouse - Hey Little Rich Girl - Live HD.mp4",
                ),
                PathBuf::from("C:/Users/Altieres/Music/Amy Winehouse - Monkey Man - Live HD.mp4"),
            ),
        ];
        for (a, b) in pairs {
            let guards = [
                bc5_media::SourceFingerprint::capture(&a).unwrap(),
                bc5_media::SourceFingerprint::capture(&b).unwrap(),
            ];
            let mut h = Harness::new(SessionKind::Media);
            h.screen.load_files(&a, &b).unwrap();
            h.frame(vec![]);
            h.frame(vec![]);
            for side in [0, 1] {
                let index = h
                    .screen
                    .rows
                    .iter()
                    .position(|r| !r.group && r.names[side].is_some())
                    .unwrap();
                let row = &h.screen.rows[index];
                let value = property_value(
                    h.screen.kind,
                    &row.key,
                    row.values[side].as_deref().unwrap(),
                );
                h.open(index, side);
                assert_eq!(h.action("copy").platform_output.copied_text, value);
                h.frame(vec![]);
            }
            guards[0].verify_current(&a).unwrap();
            guards[1].verify_current(&b).unwrap();
        }
    }
}
