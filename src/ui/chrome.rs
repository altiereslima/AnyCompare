use super::icons::IconManager;
use egui::*;

pub fn menu_button<R>(
    ui: &mut Ui,
    label: &str,
    content: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<Option<R>> {
    let response = ui.menu_button(crate::i18n::tr(label), content);
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            Id::new("capture_menu_rect").with(label),
            response.response.rect,
        )
    });
    response
}

/// Menu bar laid out like the original at 100% (text-loaded.png): first item 2 px from the
/// left, item width = caption advance + 12 with the caption 6 px in, baseline 18 px below
/// the top of the 27 px band.
pub fn menu_bar<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    menu::bar(ui, |ui| {
        ui.spacing_mut().button_padding = vec2(6.0, MENU_TEXT_TOP);
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(2.0);
        add_contents(ui)
    })
}

/// Space above the 16 px text box of a menu bar caption (ascent 13 -> baseline 18).
const MENU_TEXT_TOP: f32 = 5.0;

/// Shared menu gutter, original action icons and right-aligned shortcuts.
pub fn menu_item(ui: &mut Ui, icons: &mut IconManager, label: &str, enabled: bool) -> Response {
    let translated = crate::i18n::tr(label);
    let translated = if translated == label {
        let (caption, shortcut) = label.rsplit_once("  ").unwrap_or((label, ""));
        let bare = caption.trim_end_matches("...");
        let caption = crate::i18n::tr(bare);
        format!(
            "{}{}{}{}",
            caption,
            if bare.len() != label.split("  ").next().unwrap_or(label).len() {
                "..."
            } else {
                ""
            },
            if shortcut.is_empty() { "" } else { "  " },
            shortcut
        )
    } else {
        translated
    };
    let (caption, shortcut) = translated.rsplit_once("  ").unwrap_or((&translated, ""));
    let icon = menu_icon(label);
    let texture = icon
        .and_then(|name| icons.get_icon_with_state(ui.ctx(), name, enabled))
        .unwrap_or_else(|| icons.menu_gutter(ui.ctx()));
    ui.add_enabled_ui(enabled, |ui| {
        let caption = RichText::new(caption).color(if enabled {
            Color32::BLACK
        } else {
            Color32::GRAY
        });
        let mut button = Button::image_and_text((texture.id(), vec2(16.0, 16.0)), caption);
        if !shortcut.is_empty() {
            button = button.shortcut_text(shortcut);
        }
        ui.add(button)
    })
    .inner
}

fn menu_icon(label: &str) -> Option<&'static str> {
    let label = label
        .split("  ")
        .next()
        .unwrap_or(label)
        .trim_end_matches('.');
    Some(match label {
        "Página Inicial" => "imgSessionHome",
        "Comparar Textos" => "imgHomeTextCompareSession",
        "Comparar Pastas" => "imgHomeDirCompareSession",
        "Comparar Hexadecimal" => "imgHomeHexCompareSession",
        "Opções" => "imgOptions",
        "Formato do arquivo" | "Formato do texto" => "imgTextFormatEdit",
        "Formatos de Arquivo" => "imgFileFormatManager",
        "Regras de comparação" => "imgRules",
        "Configurações da Sessão" => "imgSessionEdit",
        "Relatório de texto" | "Relatório de pastas" | "Relatório hexadecimal" => "imgReport",
        "Informações da comparação" => "imgInfo",
        "Salvar Sessão" => "imgSessionSave",
        "Salvar Sessão Como" => "imgSessionSaveAs",
        "Salvar arquivo" | "Salvar bytes" => "imgSave",
        "Salvar arquivo como" | "Salvar bytes como" | "Salvar como" => "imgSaveAs",
        "Salvar Instantâneo" => "imgSaveSnapshot",
        "Visualizar Correção" => "imgTextPatchSession",
        "Bloqueado" => "imgSessionLocked",
        "Inverter lados" | "Inverter Lados" => "imgSwap",
        "Recarregar" => "imgRefresh",
        "Recomparar" | "Recomparar Arquivos" => "imgRecompare",
        "Desfazer" => "imgUndo",
        "Refazer" => "imgRedo",
        "Recortar" | "Recortar bytes" => "imgCut",
        "Copiar" | "Copiar bytes" | "Copiar nomes dos arquivos" => "imgCopy",
        "Colar" | "Colar bytes" => "imgPaste",
        "Excluir bytes" | "Excluir linhas" | "Excluir" => "imgDelete",
        "Localizar" | "Localizar nome" | "Localizar bytes" => "imgFind",
        "Localizar próximo" | "Localizar próxima ocorrência" => "imgFindNext",
        "Localizar anterior" | "Localizar ocorrência anterior" => "imgFindPrev",
        "Substituir" | "Substituir bytes" => "imgReplace",
        "Ir para" => "imgGoTo",
        "Próxima diferença" => "imgDown",
        "Diferença anterior" => "imgUp",
        "Copiar para a Direita"
        | "Copiar diferença para a Direita"
        | "Copiar linha para a Direita" => "imgCopyRight",
        "Copiar para a Esquerda"
        | "Copiar diferença para a Esquerda"
        | "Copiar linha para a Esquerda" => "imgCopyLeft",
        "Expandir tudo" => "imgExpandAll",
        "Recolher tudo" => "imgCollapseAll",
        "Selecionar tudo" => "imgSelectAll",
        "Renomear" => "imgRename",
        "Nova pasta" => "imgNewFolder",
        "Parar" => "imgStopRed",
        "Editar bytes" | "Editar Arquivo de Texto" => "imgFullEdit",
        "Mesclar Arquivos" => "imgTextMergeSession",
        _ => return None,
    })
}

/// Vertical gradient interpolated per physical pixel row in sRGB, as GDI paints it (an
/// egui mesh would interpolate in linear space and darken the middle rows).
pub fn gradient(ui: &Ui, rect: Rect, top: Color32, bottom: Color32) {
    let ppp = ui.ctx().pixels_per_point();
    let first = (rect.top() * ppp).round() as i32;
    let last = (rect.bottom() * ppp).round() as i32;
    let rows = (last - first).max(1);
    let lerp = |a: u8, b: u8, t: f32| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    for row in first..last {
        let t = if rows > 1 {
            (row - first) as f32 / (rows - 1) as f32
        } else {
            0.0
        };
        let color = Color32::from_rgb(
            lerp(top.r(), bottom.r(), t),
            lerp(top.g(), bottom.g(), t),
            lerp(top.b(), bottom.b(), t),
        );
        ui.painter().rect_filled(
            Rect::from_min_max(
                pos2(rect.left(), row as f32 / ppp),
                pos2(rect.right(), (row + 1) as f32 / ppp),
            ),
            0.0,
            color,
        );
    }
}

pub fn menu_background(ui: &Ui) {
    gradient(
        ui,
        ui.max_rect(),
        Color32::from_rgb(237, 241, 245),
        Color32::from_rgb(221, 227, 233),
    );
}

/// Height of the menu band; the toolbar band starts right below it (text-loaded.png).
pub const MENU_BAND_HEIGHT: f32 = 27.0;

/// Gradients fitted to the original at 100% (rows within ±1 of its dithering): menu band
/// rows 0..=26 from (237,241,245) to (221,227,233), toolbar band rows 27..=65 from
/// (221,227,233) to (198,207,216); there is no line between them.
pub fn toolbar_background(ui: &Ui) {
    gradient(
        ui,
        ui.max_rect(),
        if super::session_tabs::visible(ui.ctx()) {
            Color32::from_rgb(235, 239, 243)
        } else {
            Color32::from_rgb(221, 227, 233)
        },
        Color32::from_rgb(198, 207, 216),
    );
}
