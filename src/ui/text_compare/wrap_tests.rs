fn wrapped_screen() -> (Context, TextCompareScreen, IconManager) {
    let ctx = Context::default();
    super::super::theme::apply(&ctx);
    let mut screen = TextCompareScreen::new();
    screen.left.text = format!("{}\nâncora\n", "ação 🙂 e palavra comprida ".repeat(24));
    screen.right.text = format!("{}\nlinha só direita\nâncora\n", "outro texto ".repeat(4));
    screen.word_wrap = true;
    screen.compare();
    let mut icons = IconManager::new();
    for _ in 0..3 {
        render_frame_timed(&ctx, &mut screen, &mut icons, vec![]);
    }
    (ctx, screen, icons)
}

#[test]
fn word_wrap_aligns_physical_lines_and_preserves_unicode_character_mapping() {
    let (_ctx, screen, _icons) = wrapped_screen();
    let left = screen.editor_cache[0].test_layout();
    let right = screen.editor_cache[1].test_layout();
    for (layout, text) in [(&left, &screen.left.text), (&right, &screen.right.text)] {
        assert_eq!(layout.galley.job.text, *text);
        assert_eq!(
            layout
                .galley
                .rows
                .iter()
                .map(|r| r.char_count_including_newline())
                .sum::<usize>(),
            text.chars().count()
        );
        assert!(layout.galley.rows.len() >= text.split('\n').count());
        for index in 0..=text.chars().count() {
            let cursor = layout.galley.from_ccursor(egui::text::CCursor::new(index));
            assert_eq!(cursor.ccursor.index, index);
            let prefix: String = text.chars().take(index).collect();
            assert_eq!(
                cursor.pcursor.paragraph,
                prefix.bytes().filter(|&b| b == b'\n').count()
            );
        }
    }
    let anchor = |layout: &aligned_editor::EditorLayout, paragraph| {
        let cursor = layout
            .galley
            .from_pcursor(egui::epaint::text::cursor::PCursor {
                paragraph,
                offset: 0,
                prefer_next_row: true,
            });
        layout.galley.pos_from_cursor(&cursor).top() + layout.leading_y
    };
    assert_eq!(anchor(&left, 1), anchor(&right, 2));
    assert_eq!(screen.scroll_offset_x, [0.0; 2]);
}

#[test]
fn word_wrap_clicks_continuations_edits_and_undoes_at_real_unicode_offset() {
    let (ctx, mut screen, mut icons) = wrapped_screen();
    let before = screen.left.text.clone();
    let layout = screen.editor_cache[0].test_layout();
    let row = &layout.galley.rows[2];
    let local = pos2(row.glyphs[3].pos.x + 2.0, row.rect.top() + ROW_HEIGHT / 2.0);
    let expected = layout.galley.cursor_from_pos(local.to_vec2()).ccursor.index;
    let rect = screen.editor_cache[0].widget_rect.unwrap();
    let pos = rect.min + local.to_vec2();
    for pressed in [true, false] {
        render_frame_timed(
            &ctx,
            &mut screen,
            &mut icons,
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
    render_frame_timed(
        &ctx,
        &mut screen,
        &mut icons,
        vec![Event::Text("漢🙂".into())],
    );
    let byte = before
        .char_indices()
        .nth(expected)
        .map(|(b, _)| b)
        .unwrap_or(before.len());
    let mut edited = before.clone();
    edited.insert_str(byte, "漢🙂");
    assert_eq!(screen.left.text, edited);
    assert_eq!(screen.caret[0], (1, expected + 3));
    screen.undo_merge();
    assert_eq!(screen.left.text, before);
}

#[test]
fn word_wrap_filters_expand_without_losing_text_and_selection_copy_is_physical() {
    let (ctx, mut screen, mut icons) = wrapped_screen();
    let before = (screen.left.text.clone(), screen.right.text.clone());
    screen.filter_mode = FilterMode::Differences;
    render_frame_timed(&ctx, &mut screen, &mut icons, vec![]);
    assert!(!screen.fold_controls.is_empty());
    let layout = screen.editor_cache[0].test_layout();
    assert_eq!(layout.galley.job.text, before.0);
    focus_at(&ctx, &screen, true, 0);
    let ctrl = Modifiers {
        ctrl: true,
        command: true,
        ..Modifiers::NONE
    };
    render_frame_timed(
        &ctx,
        &mut screen,
        &mut icons,
        vec![Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: ctrl,
        }],
    );
    let out = render_frame(&ctx, &mut screen, &mut icons, vec![Event::Copy]);
    assert_eq!(out.platform_output.copied_text, before.0);
    let range = screen.fold_controls[0].1.clone();
    text_view::toggle(&mut screen.revealed, range);
    screen.view_revision += 1;
    render_frame_timed(&ctx, &mut screen, &mut icons, vec![]);
    assert_eq!((screen.left.text, screen.right.text), before);
}

#[test]
fn word_wrap_resize_dpi_session_and_readonly_preserve_buffers() {
    let (ctx, mut screen, mut icons) = wrapped_screen();
    let before = (screen.left.text.clone(), screen.right.text.clone());
    let options = screen.session_options();
    assert_eq!(options["word_wrap"], true);
    let mut restored = TextCompareScreen::new();
    restored.apply_session_options(&options);
    assert!(restored.word_wrap);
    let mut old_rows = 0;
    for scale in [1.0, 1.25, 1.5, 2.0] {
        ctx.set_pixels_per_point(scale);
        for width in [700.0, 1153.0, 1919.0] {
            for _ in 0..2 {
                let _ = ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 720.0))),
                        ..Default::default()
                    },
                    |ctx| {
                        CentralPanel::default().show(ctx, |ui| {
                            screen.ui(ctx, ui, &mut icons, &mut TestDialogs::default());
                        });
                    },
                );
            }
            let layout = screen.editor_cache[0].test_layout();
            assert!(layout.galley.size().x <= width / 2.0 - TEXT_OFFSET + 1.0);
            if width == 700.0 {
                old_rows = layout.galley.rows.len();
            }
            if width == 1919.0 {
                assert!(layout.galley.rows.len() < old_rows);
            }
            assert_eq!(screen.scroll_offset_x, [0.0; 2]);
        }
    }
    screen.editing = false;
    focus_at(&ctx, &screen, true, 9);
    render_frame_timed(
        &ctx,
        &mut screen,
        &mut icons,
        vec![
            Event::Text("NÃO".into()),
            Event::Paste("NÃO".into()),
            Event::Cut,
        ],
    );
    screen.word_wrap = false;
    render_frame_timed(&ctx, &mut screen, &mut icons, vec![]);
    assert_eq!((screen.left.text, screen.right.text), before);
}

#[test]
fn word_wrap_keyboard_search_and_goto_keep_logical_line_and_scroll_to_hit() {
    let (ctx, mut screen, mut icons) = wrapped_screen();
    focus_at(&ctx, &screen, true, 2);
    render_frame_timed(
        &ctx,
        &mut screen,
        &mut icons,
        vec![Event::Key {
            key: Key::ArrowDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert_eq!(
        screen.caret[0].0, 1,
        "visual movement must stay on physical line 1"
    );
    assert!(screen.caret[0].1 > 2);
    screen.left.text = format!(
        "{}alvo final\nâncora\n",
        "linha comprida com espaços ".repeat(150)
    );
    screen.compare();
    render_frame_timed(&ctx, &mut screen, &mut icons, vec![]);
    focus_at(&ctx, &screen, true, 0);
    screen.active_left = true;
    screen.search.options.query = "alvo final".into();
    screen.search.sides = [true, false];
    screen.find(&ctx, true);
    for _ in 0..3 {
        render_frame_timed(&ctx, &mut screen, &mut icons, vec![]);
    }
    assert_eq!(screen.caret[0].0, 1);
    assert!(
        screen.scroll_offset_y > 100.0,
        "search near the end of a wrapped paragraph must scroll"
    );
    let layout = screen.editor_cache[0].test_layout();
    let id = screen.editor_cache[0].widget_id.unwrap();
    let state = TextEdit::load_state(&ctx, id).unwrap();
    let range = state.cursor.char_range().unwrap();
    let index = range.primary.index;
    let cursor = layout.galley.from_ccursor(egui::text::CCursor::new(index));
    assert_eq!(cursor.pcursor.paragraph, 0);
    assert!(cursor.rcursor.row > 10);
    let rect = layout
        .galley
        .pos_from_cursor(&cursor)
        .translate(screen.editor_cache[0].widget_rect.unwrap().min.to_vec2());
    assert!(
        rect.top() >= 0.0 && rect.bottom() <= 700.0,
        "hit must be visible: {rect:?}"
    );
    let mut dialogs = TestDialogs::default();
    dialogs.inputs.push_back(Some("2:3".into()));
    screen.go_to(&mut dialogs);
    for _ in 0..3 {
        render_frame_timed(&ctx, &mut screen, &mut icons, vec![]);
    }
    assert_eq!(screen.caret[0], (2, 3));
    assert!(dialogs.errors.is_empty());
}

#[test]
fn word_wrap_saves_utf16_bom_crlf_without_inserting_display_breaks() {
    let dir = std::env::temp_dir().join(format!("anycompare-wrap-save-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("own-left.txt");
    let other = dir.join("own-right.txt");
    let original = format!("{}\r\nfim\r\n", "ação e Unicode 漢🙂 ".repeat(35));
    let encode = |text: &str| {
        let mut bytes = vec![0xff, 0xfe];
        for word in text.encode_utf16() {
            bytes.extend(word.to_le_bytes());
        }
        bytes
    };
    let bytes = encode(&original);
    std::fs::write(&path, &bytes).unwrap();
    std::fs::write(&other, &bytes).unwrap();
    let ctx = Context::default();
    super::super::theme::apply(&ctx);
    let mut icons = IconManager::new();
    let mut screen = TextCompareScreen::new();
    screen.load_files(path.to_str().unwrap(), other.to_str().unwrap());
    screen.word_wrap = true;
    for _ in 0..2 {
        render_frame_timed(&ctx, &mut screen, &mut icons, vec![]);
    }
    assert!(!screen.left.dirty());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    focus_at(&ctx, &screen, true, 40);
    render_frame_timed(
        &ctx,
        &mut screen,
        &mut icons,
        vec![Event::Text("ç🙂".into())],
    );
    let mut expected: String = original.chars().take(40).collect();
    expected.push_str("ç🙂");
    expected.extend(original.chars().skip(40));
    screen.save_side(true, false, &mut TestDialogs::default());
    assert_eq!(std::fs::read(&path).unwrap(), encode(&expected));
    assert_eq!(std::fs::read(&other).unwrap(), bytes);
    assert!(!screen.left.dirty());
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(other).unwrap();
    std::fs::remove_dir(dir).unwrap();
}

#[test]
#[ignore = "Run English UI checks in their own test process"]
fn word_wrap_english_hidden_view_menu_preserves_selection() {
    crate::i18n::initialize(Some("en"));
    let (ctx, mut screen, mut icons) = wrapped_screen();
    let before = screen.left.text.clone();
    focus_at(&ctx, &screen, true, 38);
    let id = screen.editor_cache[0].widget_id.unwrap();
    let mut state = TextEdit::load_state(&ctx, id).unwrap();
    let selection =
        egui::text::CCursorRange::two(egui::text::CCursor::new(38), egui::text::CCursor::new(9));
    state.cursor.set_char_range(Some(selection));
    state.store(&ctx, id);
    render_frame_timed(&ctx, &mut screen, &mut icons, vec![]);
    let click = |screen: &mut TextCompareScreen, icons: &mut IconManager, pos| {
        for pressed in [true, false] {
            render_frame_timed(
                &ctx,
                screen,
                icons,
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
    };
    let menu: Rect = ctx
        .data(|d| d.get_temp(Id::new("capture_menu_rect").with("menu.view")))
        .unwrap();
    click(&mut screen, &mut icons, menu.center());
    let output = render_frame(&ctx, &mut screen, &mut icons, vec![]);
    fn contains_label(shape: &egui::Shape) -> bool {
        match shape {
            egui::Shape::Text(t) => t.galley.job.text == "Word Wrap",
            egui::Shape::Vec(v) => v.iter().any(contains_label),
            _ => false,
        }
    }
    assert!(output.shapes.iter().any(|s| contains_label(&s.shape)));
    let toggle: Rect = ctx
        .data(|d| d.get_temp(Id::new("text_compare_wrap_toggle")))
        .unwrap();
    click(&mut screen, &mut icons, toggle.center());
    assert!(!screen.word_wrap);
    assert_eq!(screen.left.text, before);
    assert_eq!(
        TextEdit::load_state(&ctx, id)
            .unwrap()
            .cursor
            .char_range()
            .unwrap(),
        selection
    );
}

#[test]
#[ignore = "Manual CPU frame benchmark; no desktop windows or GPU"]
fn profile_word_wrap_edit_latency_hidden() {
    let (ctx, mut screen, mut icons) = wrapped_screen();
    screen.left.text = (0..1200)
        .map(|i| format!("{i}: {}\n", "ação palavra ".repeat(20)))
        .collect();
    screen.right.text = screen.left.text.replace("500:", "NOVO:");
    screen.compare();
    render_frame_timed(&ctx, &mut screen, &mut icons, vec![]);
    focus_at(&ctx, &screen, true, 120);
    render_frame_timed(&ctx, &mut screen, &mut icons, vec![]);
    let mut samples = Vec::new();
    for _ in 0..12 {
        let start = Instant::now();
        render_frame_timed(&ctx, &mut screen, &mut icons, vec![Event::Text("x".into())]);
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "{}",
        serde_json::json!({"metric":"wrapped CPU input/layout; no GPU", "bytes":screen.left.text.len(), "frames_ms":samples, "mean_ms":samples.iter().sum::<f64>() / samples.len() as f64})
    );
}
