//! Toolbar rendered from the generated specification (`toolbar_spec.rs`).
//!
//! Geometry measured on the original at 100% (`folder-empty.png`, `folder-sync.png`,
//! `text-loaded.png`; every button and separator of those three toolbars lands on the same
//! pixel columns):
//! - first item 2 px from the left edge; buttons 36 px tall;
//! - button width = max(45, caption advance in Segoe UI 9pt + 12), caption centred;
//! - split part of a drop-down button 14 px (15 px for the sessions tree button), with a
//!   7×4 arrow 2 px (3 px) into it and 17 px below the top;
//! - separator 6 px wide: dark line 2 px into it and light line 3 px into it, rows 2..=34;
//! - a checked button draws its icon 1 px right/down and its caption 1 px down;
//! - icon 16×16 centred, 3 px below the top; caption baseline 31 px below the top;
//! - checked: fill (204,232,255), border (153,209,255); hot: fill (229,243,255),
//!   border (204,232,255);
//! - combo item: label, 3 px, then a 259×23 box 7 px below the top.

use super::icons::IconManager;
use egui::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Separator,
    /// Plain button with an action.
    Button,
    /// "<X>Side" button that shows the "<X>Right" or "<X>Left" face toward the other pane.
    Side,
    /// Button with a 15 px drop-down part (sessions tree).
    Tree,
    /// Button with an action and a 14 px drop-down part.
    Dropdown,
    /// Drop-down group without an action; its face shows the checked (or first) child.
    Group,
    /// Label and combo box.
    Combo,
    /// Entry of a drop-down group.
    Item,
}

#[derive(Clone, Copy, Debug)]
pub struct ToolItem {
    pub name: &'static str,
    pub kind: ToolKind,
    pub image: &'static str,
    /// Menu caption (group entries) or toolbar caption.
    pub caption: &'static str,
    /// Toolbar caption (ShortCaption, or Caption without accelerator).
    pub short: &'static str,
    /// Short hint (text before '|').
    pub hint: &'static str,
    pub children: &'static [ToolItem],
}

impl ToolItem {
    pub const SEPARATOR: ToolItem = ToolItem {
        name: "",
        kind: ToolKind::Separator,
        image: "",
        caption: "",
        short: "",
        hint: "",
        children: &[],
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToolState {
    pub enabled: bool,
    pub checked: bool,
}

impl ToolState {
    pub const DISABLED: ToolState = ToolState {
        enabled: false,
        checked: false,
    };

    pub const fn enabled(enabled: bool) -> Self {
        ToolState {
            enabled,
            checked: false,
        }
    }

    pub const fn checked(checked: bool) -> Self {
        ToolState {
            enabled: true,
            checked,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolEvent {
    /// Face of a button (for a group: the entry shown on the face or picked in its menu).
    Click(&'static str),
    /// Drop-down part of a Tree or Dropdown button.
    Arrow(&'static str),
}

pub const ROW_HEIGHT: f32 = 36.0;
const MIN_WIDTH: f32 = 45.0;
const PADDING: f32 = 12.0;
const SPLIT: f32 = 14.0;
const TREE_SPLIT: f32 = 15.0;
const SEPARATOR: f32 = 6.0;
const COMBO_GAP: f32 = 3.0;
const COMBO_WIDTH: f32 = 259.0;
const COMBO_HEIGHT: f32 = 23.0;
pub const LEFT_MARGIN: f32 = 2.0;
/// Space above the first and below the last row inside the toolbar band (39 rows for one
/// row of buttons in the original).
pub const MARGIN_TOP: f32 = 0.0;
pub const MARGIN_BOTTOM: f32 = 3.0;
/// Vertical space between toolbar rows.
const ROW_GAP: f32 = 2.0;

fn spacing(ui: &Ui) -> (f32, f32, f32) {
    if super::session_tabs::visible(ui.ctx()) {
        (5.0, 5.0, 5.0)
    } else {
        (MARGIN_TOP, MARGIN_BOTTOM, ROW_GAP)
    }
}

const CHECKED_FILL: Color32 = Color32::from_rgb(204, 232, 255);
const CHECKED_BORDER: Color32 = Color32::from_rgb(153, 209, 255);
const HOT_FILL: Color32 = Color32::from_rgb(229, 243, 255);
const HOT_BORDER: Color32 = Color32::from_rgb(204, 232, 255);
// Etched separator, sampled over the toolbar gradient: x0.55 dark line, +30% white line.
const SEPARATOR_DARK: Color32 = Color32::from_black_alpha(115);
const SEPARATOR_LIGHT: Color32 = Color32::from_rgba_premultiplied(77, 77, 77, 77);
const ARROW: Color32 = Color32::from_rgb(40, 40, 40);
/// Caption of a disabled button (folder-empty.png: "Copiar", "Parar").
const DISABLED_TEXT: Color32 = Color32::from_gray(160);

fn font() -> FontId {
    FontId::proportional(12.0)
}

fn advance(ui: &Ui, text: &str) -> f32 {
    ui.fonts(|fonts| fonts.layout_no_wrap(text.to_owned(), font(), Color32::BLACK))
        .size()
        .x
}

/// Width of a button face for a caption (without the drop-down part).
pub fn face_width(ui: &Ui, caption: &str) -> f32 {
    MIN_WIDTH.max(advance(ui, &crate::i18n::tr(caption)).round() + PADDING)
}

/// Group entry shown on the face: the checked child, else the first one.
fn group_face(item: &ToolItem, state: &dyn Fn(&str) -> ToolState) -> Option<&'static ToolItem> {
    item.children
        .iter()
        .find(|child| state(child.name).checked)
        .or_else(|| item.children.first())
}

fn item_width(ui: &Ui, item: &ToolItem, state: &dyn Fn(&str) -> ToolState) -> f32 {
    match item.kind {
        ToolKind::Separator => SEPARATOR,
        ToolKind::Button | ToolKind::Side | ToolKind::Item => face_width(ui, item.short),
        ToolKind::Tree => face_width(ui, item.short) + TREE_SPLIT,
        ToolKind::Dropdown => face_width(ui, item.short) + SPLIT,
        ToolKind::Group => {
            group_face(item, state).map_or(MIN_WIDTH, |face| face_width(ui, face.short)) + SPLIT
        }
        ToolKind::Combo => {
            advance(ui, &crate::i18n::tr(item.short)).round() + COMBO_GAP + COMBO_WIDTH
        }
    }
}

/// Row, left offset and width of a placed item.
type Placed = (usize, f32, f32);

/// Positions of the items, wrapping whole separator groups to new rows.
fn layout(
    ui: &Ui,
    items: &[ToolItem],
    width: f32,
    state: &dyn Fn(&str) -> ToolState,
) -> (Vec<Option<Placed>>, usize) {
    let widths: Vec<f32> = items.iter().map(|i| item_width(ui, i, state)).collect();
    let mut placed = vec![None; items.len()];
    let (mut row, mut x) = (0usize, LEFT_MARGIN);
    let mut start = 0;
    while start < items.len() {
        let end = items[start + 1..]
            .iter()
            .position(|i| i.kind == ToolKind::Separator)
            .map_or(items.len(), |p| start + 1 + p);
        let group: f32 = widths[start..end].iter().sum();
        let leading_separator = items[start].kind == ToolKind::Separator;
        if x > LEFT_MARGIN && x + group > width {
            row += 1;
            x = LEFT_MARGIN;
        }
        for i in start..end {
            if i == start && leading_separator && x == LEFT_MARGIN {
                continue;
            }
            placed[i] = Some((row, x, widths[i]));
            x += widths[i];
        }
        start = end;
    }
    (placed, row + 1)
}

/// Height of the toolbar for an available width.
pub fn height(ui: &Ui, items: &[ToolItem], width: f32, state: &dyn Fn(&str) -> ToolState) -> f32 {
    let rows = layout(ui, items, width, state).1 as f32;
    let (top, bottom, gap) = spacing(ui);
    top + rows * ROW_HEIGHT + (rows - 1.0) * gap + bottom
}

fn paint_frame(painter: &Painter, rect: Rect, checked: bool, hot: bool, split: Option<f32>) {
    let (fill, border) = match (checked, hot) {
        (true, _) => (CHECKED_FILL, CHECKED_BORDER),
        (false, true) => (HOT_FILL, HOT_BORDER),
        _ => return,
    };
    // Filled 1 px rectangles stay crisp; egui strokes spread over two pixel rows.
    let rect = painter.round_rect_to_pixels(rect);
    painter.rect_filled(rect, 0.0, fill);
    let (l, t, r, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    for edge in [
        Rect::from_min_max(pos2(l, t), pos2(r, t + 1.0)),
        Rect::from_min_max(pos2(l, b - 1.0), pos2(r, b)),
        Rect::from_min_max(pos2(l, t), pos2(l + 1.0, b)),
        Rect::from_min_max(pos2(r - 1.0, t), pos2(r, b)),
    ] {
        painter.rect_filled(edge, 0.0, border);
    }
    if let Some(x) = split {
        let x = painter.round_to_pixel(x);
        painter.rect_filled(
            Rect::from_min_max(pos2(x, t), pos2(x + 1.0, b)),
            0.0,
            border,
        );
    }
}

fn paint_arrow(painter: &Painter, left: f32, top: f32) {
    let origin = painter.round_pos_to_pixels(pos2(left, top + 17.0));
    for (row, width) in [7.0, 5.0, 3.0, 1.0].into_iter().enumerate() {
        let row = row as f32;
        painter.rect_filled(
            Rect::from_min_size(origin + vec2(row, row), vec2(width, 1.0)),
            0.0,
            ARROW,
        );
    }
}

fn paint_filter_icon(painter: &Painter, rect: Rect, name: &str, enabled: bool) -> bool {
    #[derive(Clone, Copy)]
    enum Block {
        Red,
        Gray,
        Black,
        Purple,
    }
    let rows: &[(Option<Block>, Option<Block>)] = match name {
        "FavNoOrphan" => &[
            (Some(Block::Red), Some(Block::Gray)),
            (Some(Block::Gray), Some(Block::Red)),
            (Some(Block::Black), Some(Block::Black)),
        ],
        "FavDiffNoOrphan" => &[
            (Some(Block::Red), Some(Block::Gray)),
            (Some(Block::Gray), Some(Block::Red)),
        ],
        "FavOrphan" => &[
            (None, Some(Block::Purple)),
            (Some(Block::Purple), None),
            (None, Some(Block::Purple)),
        ],
        "FavLeftNewer" => &[
            (Some(Block::Red), Some(Block::Gray)),
            (Some(Block::Red), Some(Block::Gray)),
            (Some(Block::Red), Some(Block::Gray)),
        ],
        "FavRightNewer" => &[
            (Some(Block::Gray), Some(Block::Red)),
            (Some(Block::Gray), Some(Block::Red)),
            (Some(Block::Gray), Some(Block::Red)),
        ],
        "FavLeftNewerOrphan" => &[
            (Some(Block::Red), Some(Block::Gray)),
            (Some(Block::Purple), None),
            (Some(Block::Purple), None),
        ],
        "FavRightNewerOrphan" => &[
            (Some(Block::Gray), Some(Block::Red)),
            (None, Some(Block::Purple)),
            (None, Some(Block::Purple)),
        ],
        "FavLeftOrphan" => &[
            (Some(Block::Purple), None),
            (Some(Block::Purple), None),
            (Some(Block::Purple), None),
        ],
        "FavRightOrphan" => &[
            (None, Some(Block::Purple)),
            (None, Some(Block::Purple)),
            (None, Some(Block::Purple)),
        ],
        _ => return false,
    };

    let colors = |b: Block| -> (Color32, Color32) {
        if !enabled {
            return (Color32::from_gray(195), Color32::from_gray(155));
        }
        match b {
            Block::Red => (Color32::from_rgb(228, 42, 42), Color32::from_rgb(162, 18, 18)),
            Block::Gray => (
                Color32::from_rgb(172, 172, 172),
                Color32::from_rgb(112, 112, 112),
            ),
            Block::Black => (
                Color32::from_rgb(80, 80, 80),
                Color32::from_rgb(35, 35, 35),
            ),
            Block::Purple => (
                Color32::from_rgb(154, 42, 238),
                Color32::from_rgb(102, 14, 176),
            ),
        }
    };

    let origin = painter.round_pos_to_pixels(rect.min);
    let block_w = 5.0;
    let block_h = 3.0;
    let row_step = 5.0;
    let total_h = (rows.len() as f32) * block_h + (rows.len().saturating_sub(1) as f32) * 2.0;
    let start_y = origin.y + ((16.0 - total_h) / 2.0).floor();
    let left_x = origin.x + 2.0;
    let right_x = origin.x + 9.0;

    let draw_block = |x: f32, y: f32, b: Block| {
        let (fill, border) = colors(b);
        let r = Rect::from_min_size(pos2(x, y), vec2(block_w, block_h));
        painter.rect_filled(r, 0.0, border);
        painter.rect_filled(r.shrink(0.5), 0.0, fill);
    };

    for (idx, &(left, right)) in rows.iter().enumerate() {
        let y = start_y + (idx as f32) * row_step;
        if let Some(b) = left {
            draw_block(left_x, y, b);
        }
        if let Some(b) = right {
            draw_block(right_x, y, b);
        }
    }
    if name == "FavOrphan" {
        let dash_color = if enabled {
            Color32::from_gray(75)
        } else {
            Color32::from_gray(160)
        };
        painter.rect_filled(
            Rect::from_min_size(pos2(origin.x + 7.0, origin.y + 7.0), vec2(2.0, 2.0)),
            0.0,
            dash_color,
        );
    }
    true
}

fn paint_face(
    ui: &Ui,
    icons: &mut IconManager,
    rect: Rect,
    name: &str,
    image: &str,
    caption: &str,
    state: ToolState,
) {
    let painter = ui.painter();
    // A checked button draws its icon 1 px right and down and its caption 1 px down.
    let pressed = if state.checked { 1.0 } else { 0.0 };
    let icon_rect = painter.round_rect_to_pixels(Rect::from_min_size(
        pos2(
            rect.left() + ((rect.width() - 16.0) / 2.0).floor() + pressed,
            rect.top() + 3.0 + pressed,
        ),
        vec2(16.0, 16.0),
    ));
    if let Some(texture) = icons.get_icon_with_state(ui.ctx(), image, state.enabled) {
        painter.image(
            texture.id(),
            icon_rect,
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        paint_filter_icon(painter, icon_rect, name, state.enabled);
    }
    let caption = crate::i18n::tr(caption);
    let galley = ui.fonts(|fonts| {
        fonts.layout_no_wrap(
            caption,
            font(),
            if state.enabled {
                Color32::BLACK
            } else {
                DISABLED_TEXT
            },
        )
    });
    let baseline = galley
        .rows
        .first()
        .and_then(|row| row.glyphs.first())
        .map_or(galley.size().y * 0.8, |glyph| glyph.pos.y);
    // Half pixels round up ("Editar" in a 45 px button starts 8 px in).
    let left = rect.left() + ((rect.width() - galley.size().x.round()) / 2.0).round();
    let pos = painter.round_pos_to_pixels(pos2(left, rect.top() + 31.0 + pressed - baseline));
    painter.galley(pos, galley, Color32::BLACK);
}

/// Paints the toolbar band over `ui.max_rect()`, draws the items and returns the event of
/// this frame.
///
/// `state` gives enabled/checked per item name (group entries included); `left_active`
/// selects the face of `Side` buttons; `combo` draws the combo box of a `Combo` item.
pub fn show(
    ui: &mut Ui,
    icons: &mut IconManager,
    items: &'static [ToolItem],
    left_active: bool,
    state: &dyn Fn(&str) -> ToolState,
    combo: &mut dyn FnMut(&mut Ui, &ToolItem, Rect),
) -> Option<ToolEvent> {
    if ui.data(|d| {
        d.get_temp::<bool>(Id::new("bc5_flat_toolbar"))
            .unwrap_or(false)
    }) {
        ui.painter()
            .rect_filled(ui.max_rect(), 0.0, Color32::from_gray(240));
    } else {
        super::chrome::toolbar_background(ui);
    }
    let (margin_top, _, row_gap) = spacing(ui);
    let area = ui.max_rect();
    let area = Rect::from_min_max(
        ui.painter()
            .round_pos_to_pixels(area.min + vec2(0.0, margin_top)),
        area.max,
    );
    let (placed, rows) = layout(ui, items, area.width(), state);
    for row in 1..rows {
        let y = area.top() + row as f32 * (ROW_HEIGHT + row_gap) - 2.0;
        for (offset, color) in [(0.0, SEPARATOR_DARK), (1.0, SEPARATOR_LIGHT)] {
            ui.painter().rect_filled(
                Rect::from_min_size(
                    pos2(area.left() + 2.0, y + offset),
                    vec2(area.width() - 4.0, 1.0),
                ),
                0.0,
                color,
            );
        }
    }
    let mut event = None;
    for (index, item) in items.iter().enumerate() {
        let Some((row, x, width)) = placed[index] else {
            continue;
        };
        let top = area.top() + row as f32 * (ROW_HEIGHT + row_gap);
        let rect = Rect::from_min_size(pos2(area.left() + x, top), vec2(width, ROW_HEIGHT));
        let id = ui.id().with(("tool", item.name, index));
        match item.kind {
            ToolKind::Separator => {
                super::audit::record_tool(ui.ctx(), item.name, "|", "Separator", true, false, rect);
                // Etched line: dark at +2, light at +3, rows 2..=34.
                let x = ui.painter().round_to_pixel(rect.left() + 2.0);
                let top = ui.painter().round_to_pixel(top);
                for (dx, color) in [(0.0, SEPARATOR_DARK), (1.0, SEPARATOR_LIGHT)] {
                    ui.painter().rect_filled(
                        Rect::from_min_max(pos2(x + dx, top + 2.0), pos2(x + dx + 1.0, top + 35.0)),
                        0.0,
                        color,
                    );
                }
            }
            ToolKind::Combo => {
                let label = crate::i18n::tr(item.short);
                super::audit::record_tool(
                    ui.ctx(),
                    item.name,
                    &label,
                    "Combo",
                    true,
                    false,
                    rect,
                );
                let galley = ui.fonts(|f| f.layout_no_wrap(label, font(), Color32::BLACK));
                let baseline = galley
                    .rows
                    .first()
                    .and_then(|row| row.glyphs.first())
                    .map_or(galley.size().y * 0.8, |glyph| glyph.pos.y);
                let label_width = galley.size().x.round();
                let pos = ui
                    .painter()
                    .round_pos_to_pixels(pos2(rect.left(), top + 23.0 - baseline));
                ui.painter().galley(pos, galley, Color32::BLACK);
                let field = Rect::from_min_size(
                    pos2(rect.left() + label_width + COMBO_GAP, top + 7.0),
                    vec2(COMBO_WIDTH, COMBO_HEIGHT),
                );
                combo(ui, item, field);
            }
            ToolKind::Button | ToolKind::Side | ToolKind::Item => {
                let st = state(item.name);
                let cap = crate::i18n::tr(item.short);
                super::audit::record_tool(
                    ui.ctx(),
                    item.name,
                    &cap,
                    "Button",
                    st.enabled,
                    st.checked,
                    rect,
                );
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        Id::new("capture_menu_rect").with(format!("Barra: {}", item.short)),
                        rect,
                    );
                    data.insert_temp(
                        Id::new("capture_menu_rect").with(format!("Barra: {cap}")),
                        rect,
                    );
                });
                let response = ui.interact(rect, id, Sense::click());
                let hot = st.enabled && response.hovered();
                paint_frame(ui.painter(), rect, st.checked, hot, None);
                // "<X>Side" shows the double arrow while disabled (folder-empty.png) and the
                // arrow toward the other pane otherwise (text-loaded.png).
                let image = match (item.kind, st.enabled, left_active) {
                    (ToolKind::Side, false, _) => "imgCopyLeftOrRight",
                    (ToolKind::Side, true, true) => "imgCopyRight",
                    (ToolKind::Side, true, false) => "imgCopyLeft",
                    _ => item.image,
                };
                paint_face(ui, icons, rect, item.name, image, item.short, st);
                let response = response.on_hover_text(crate::i18n::tr(item.hint));
                if st.enabled && response.clicked() {
                    event = Some(ToolEvent::Click(item.name));
                }
            }
            ToolKind::Tree | ToolKind::Dropdown | ToolKind::Group => {
                let split_width = if item.kind == ToolKind::Tree {
                    TREE_SPLIT
                } else {
                    SPLIT
                };
                let face_rect =
                    Rect::from_min_max(rect.min, pos2(rect.right() - split_width, rect.bottom()));
                let arrow_rect = Rect::from_min_max(pos2(face_rect.right(), rect.top()), rect.max);
                let (face, st) = if item.kind == ToolKind::Group {
                    let face = group_face(item, state);
                    let st = face.map_or(ToolState::DISABLED, |f| state(f.name));
                    (face.copied().unwrap_or(*item), st)
                } else {
                    (*item, state(item.name))
                };
                let cap = crate::i18n::tr(face.short);
                super::audit::record_tool(
                    ui.ctx(),
                    face.name,
                    &cap,
                    match item.kind {
                        ToolKind::Tree => "Tree",
                        ToolKind::Dropdown => "Dropdown",
                        _ => "Group",
                    },
                    st.enabled,
                    st.checked,
                    rect,
                );
                ui.ctx().data_mut(|data| {
                    data.insert_temp(
                        Id::new("capture_menu_rect").with(format!("Barra: {}", face.short)),
                        rect,
                    );
                    data.insert_temp(
                        Id::new("capture_menu_rect").with(format!("Barra: {cap}")),
                        rect,
                    );
                    data.insert_temp(
                        Id::new("capture_menu_rect").with(format!("Seta: {}", face.short)),
                        arrow_rect,
                    );
                    data.insert_temp(
                        Id::new("capture_menu_rect").with(format!("Seta: {cap}")),
                        arrow_rect,
                    );
                });
                let face_response = ui.interact(face_rect, id.with("face"), Sense::click());
                let arrow_response = ui.interact(arrow_rect, id.with("arrow"), Sense::click());
                let hot = st.enabled && (face_response.hovered() || arrow_response.hovered());
                let tree_open = item.kind == ToolKind::Tree
                    && item.name == "SessionsCombo"
                    && super::session_picker::is_open(ui.ctx());
                let checked = st.checked || tree_open;
                paint_frame(
                    ui.painter(),
                    rect,
                    checked,
                    hot,
                    (checked || hot).then_some(face_rect.right() - 1.0),
                );
                paint_face(ui, icons, face_rect, face.name, face.image, face.short, st);
                paint_arrow(
                    ui.painter(),
                    arrow_rect.left()
                        + if item.kind == ToolKind::Tree {
                            3.0
                        } else {
                            2.0
                        },
                    top,
                );
                let face_response = if tree_open {
                    face_response
                } else {
                    face_response.on_hover_text(crate::i18n::tr(face.hint))
                };
                if item.kind == ToolKind::Tree && item.name == "SessionsCombo" && st.enabled {
                    super::session_picker::show(ui, icons, &face_response.union(arrow_response));
                    continue;
                }
                if st.enabled && face_response.clicked() {
                    event = Some(ToolEvent::Click(face.name));
                }
                let popup = id.with("menu");
                if st.enabled && arrow_response.clicked() {
                    if item.kind == ToolKind::Group {
                        ui.memory_mut(|m| m.toggle_popup(popup));
                    } else {
                        event = Some(ToolEvent::Arrow(item.name));
                    }
                }
                if item.kind == ToolKind::Group {
                    let anchor = face_response.clone().union(arrow_response.clone());
                    if ui.memory(|m| m.is_popup_open(popup)) {
                        let captions: Vec<String> = item
                            .children
                            .iter()
                            .map(|c| crate::i18n::tr(c.caption))
                            .collect();
                        let max_text_w = captions
                            .iter()
                            .map(|c| advance(ui, c))
                            .fold(0.0_f32, f32::max);
                        let menu_w = (max_text_w + 42.0).max(176.0).ceil();
                        let row_h = 22.0;
                        let area_resp = Area::new(popup)
                            .order(Order::Foreground)
                            .fixed_pos(pos2(anchor.rect.left(), anchor.rect.bottom() + 1.0))
                            .constrain(true)
                            .show(ui.ctx(), |ui| {
                                Frame::none()
                                    .fill(Color32::from_gray(242))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_gray(180)))
                                    .shadow(Shadow {
                                        offset: vec2(2.0, 2.0),
                                        blur: 4.0,
                                        spread: 0.0,
                                        color: Color32::from_black_alpha(38),
                                    })
                                    .inner_margin(Margin::same(2.0))
                                    .show(ui, |ui| {
                                        ui.set_width(menu_w);
                                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                                        let mut picked = None;
                                        for (child, label) in
                                            item.children.iter().zip(captions.iter())
                                        {
                                            let child_state = state(child.name);
                                            let (row_rect, resp) = ui.allocate_exact_size(
                                                vec2(menu_w, row_h),
                                                if child_state.enabled {
                                                    Sense::click()
                                                } else {
                                                    Sense::hover()
                                                },
                                            );
                                            let hot = child_state.enabled && resp.hovered();
                                            let painter = ui.painter();
                                            if hot {
                                                painter.rect_filled(row_rect, 0.0, CHECKED_FILL);
                                                painter.rect_stroke(
                                                    row_rect.shrink(0.5),
                                                    0.0,
                                                    Stroke::new(1.0_f32, CHECKED_BORDER),
                                                );
                                            }
                                            let icon_rect = painter.round_rect_to_pixels(
                                                Rect::from_min_size(
                                                    pos2(
                                                        row_rect.left() + 4.0,
                                                        row_rect.center().y - 8.0,
                                                    ),
                                                    vec2(16.0, 16.0),
                                                ),
                                            );
                                            if child_state.checked {
                                                let badge = icon_rect.expand(1.5);
                                                painter.rect_filled(badge, 1.0, CHECKED_FILL);
                                                painter.rect_stroke(
                                                    badge,
                                                    1.0,
                                                    Stroke::new(1.0_f32, CHECKED_BORDER),
                                                );
                                            }
                                            if let Some(tex) = icons.get_icon_with_state(
                                                ui.ctx(),
                                                child.image,
                                                child_state.enabled,
                                            ) {
                                                painter.image(
                                                    tex.id(),
                                                    icon_rect,
                                                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                                    Color32::WHITE,
                                                );
                                            } else {
                                                paint_filter_icon(
                                                    painter,
                                                    icon_rect,
                                                    child.name,
                                                    child_state.enabled,
                                                );
                                            }
                                            let text_color = if child_state.enabled {
                                                Color32::BLACK
                                            } else {
                                                DISABLED_TEXT
                                            };
                                            painter.text(
                                                pos2(row_rect.left() + 28.0, row_rect.center().y),
                                                Align2::LEFT_CENTER,
                                                label,
                                                font(),
                                                text_color,
                                            );
                                            ui.ctx().data_mut(|data| {
                                                data.insert_temp(
                                                    Id::new("capture_menu_rect").with(label),
                                                    row_rect,
                                                );
                                            });
                                            if child_state.enabled && resp.clicked() {
                                                picked = Some(child.name);
                                            }
                                        }
                                        picked
                                    })
                                    .inner
                            });
                        let esc = ui.input(|i| i.key_pressed(Key::Escape));
                        let clicked_elsewhere = area_resp.response.clicked_elsewhere()
                            && !arrow_response.clicked()
                            && !face_response.clicked();
                        if let Some(name) = area_resp.inner {
                            event = Some(ToolEvent::Click(name));
                            ui.memory_mut(|m| m.close_popup());
                        } else if esc || clicked_elsewhere {
                            ui.memory_mut(|m| m.close_popup());
                        }
                    }
                }
            }
        }
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_widths_follow_the_original_rule() {
        // GDI advances of Segoe UI 9pt measured on Windows: Todos 33, Diferenças 55,
        // Página Inicial 70, Editar 30 -> widths 45, 67, 82 and the 45 px minimum.
        let ctx = Context::default();
        crate::ui::theme::apply(&ctx);
        let _ = ctx.run(RawInput::default(), |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                #[cfg(windows)]
                {
                    assert_eq!(face_width(ui, "Todos"), 45.0);
                    assert_eq!(face_width(ui, "Diferenças"), 67.0);
                    assert_eq!(face_width(ui, "Página Inicial"), 82.0);
                    assert_eq!(face_width(ui, "Editar"), 45.0);
                }
                #[cfg(not(windows))]
                {
                    assert!(face_width(ui, "Todos") >= 45.0);
                    assert!(face_width(ui, "Editar") >= 45.0);
                    assert!(face_width(ui, "Diferenças") > face_width(ui, "Todos"));
                    assert!(face_width(ui, "Página Inicial") > face_width(ui, "Diferenças"));
                }
            });
        });
    }
}
