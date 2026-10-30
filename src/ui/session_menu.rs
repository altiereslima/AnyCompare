use super::{chrome::menu_item, icons::IconManager};
use crate::history::SessionKind;
use egui::{Context, Id, Ui};

#[derive(Clone)]
pub enum Request {
    New(SessionKind),
    Recent(crate::history::RecentSession),
    NewWindow,
    Open,
    LoadWorkspace,
    SaveWorkspace,
    Save(bool),
    Close,
    CompareUsing(SessionKind),
    ParentFolders,
}
fn id() -> Id {
    Id::new("anycompare_session_menu_request")
}
pub fn request(ctx: &Context, request: Request) {
    ctx.data_mut(|data| data.insert_temp(id(), request));
}
pub fn take(ctx: &Context) -> Option<Request> {
    ctx.data_mut(|data| {
        let value = data.get_temp(id());
        data.remove::<Request>(id());
        value
    })
}

pub fn prefix(ui: &mut Ui, icons: &mut IconManager, has_session: bool) {
    ui.menu_button(crate::i18n::tr("Nova Sessão"), |ui| {
        for (label, kind) in [
            ("Comparar Pastas", SessionKind::Folder),
            ("Comparar Textos", SessionKind::Text),
            ("Comparar Hexadecimal", SessionKind::Hex),
        ] {
            if menu_item(ui, icons, label, true).clicked() {
                request(ui.ctx(), Request::New(kind));
                ui.close_menu();
            }
        }
    });
    for (label, enabled, command) in [
        ("Nova Aba  Ctrl+T", true, Request::New(SessionKind::Text)),
        ("Nova Janela", true, Request::NewWindow),
        ("Abrir Sessão...  Ctrl+Shift+O", true, Request::Open),
    ] {
        if menu_item(ui, icons, label, enabled).clicked() {
            request(ui.ctx(), command);
            ui.close_menu();
        }
    }
    ui.separator();
    ui.menu_button(crate::i18n::tr("Carregar Espaço de Trabalho"), |ui| {
        if menu_item(ui, icons, "Abrir espaço de trabalho...", true).clicked() {
            request(ui.ctx(), Request::LoadWorkspace);
            ui.close_menu();
        }
    });
    if menu_item(ui, icons, "Salvar Espaço de Trabalho Como...", has_session).clicked() {
        request(ui.ctx(), Request::SaveWorkspace);
        ui.close_menu();
    }
    ui.separator();
    for (label, save_as) in [
        ("Salvar Sessão  Ctrl+Shift+S", false),
        ("Salvar Sessão Como...", true),
    ] {
        if menu_item(ui, icons, label, has_session).clicked() {
            request(ui.ctx(), Request::Save(save_as));
            ui.close_menu();
        }
    }
}
pub fn suffix(ui: &mut Ui, icons: &mut IconManager, has_pair: bool) {
    ui.separator();
    ui.menu_button(crate::i18n::tr("Comparar Arquivos Usando"), |ui| {
        for (label, kind) in [
            ("Comparar Textos", SessionKind::Text),
            ("Comparar Hexadecimal", SessionKind::Hex),
        ] {
            if menu_item(ui, icons, label, has_pair).clicked() {
                request(ui.ctx(), Request::CompareUsing(kind));
                ui.close_menu();
            }
        }
    });
    menu_item(ui, icons, "Mesclar Arquivos", false)
        .on_disabled_hover_text(crate::i18n::tr("Esta função ainda não está disponível"));
    if menu_item(ui, icons, "Comparar Pastas Pai", has_pair).clicked() {
        request(ui.ctx(), Request::ParentFolders);
        ui.close_menu();
    }
    ui.separator();
    if menu_item(ui, icons, "Sair  Ctrl+Q", true).clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        ui.close_menu();
    }
}
pub fn close(ui: &mut Ui, icons: &mut IconManager) {
    if menu_item(ui, icons, "Fechar Aba  Ctrl+W", true).clicked() {
        request(ui.ctx(), Request::Close);
        ui.close_menu();
    }
}

pub fn settings_tools(
    ui: &mut Ui,
    icons: &mut IconManager,
    dialogs: &mut dyn super::dialogs::DialogHost,
) {
    ui.separator();
    if menu_item(ui, icons, "Exportar Configurações...", true).clicked() {
        crate::i18n::settings_file(true, dialogs);
        ui.close_menu();
    }
    if menu_item(ui, icons, "Importar Configurações...", true).clicked() {
        crate::i18n::settings_file(false, dialogs);
        ui.close_menu();
    }
    if menu_item(ui, icons, "Restaurar Padrões de Fábrica...", true).clicked() {
        crate::i18n::restore_defaults(dialogs);
        ui.close_menu();
    }
}
