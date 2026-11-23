use super::{
    chrome::{menu_item, submenu_item},
    icons::IconManager,
};
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
    ManageWorkspaces,
    Save(bool),
    Close,
    CompareUsing(SessionKind),
    MergeFiles,
    ParentFolders,
    SyncBaseFolders(std::path::PathBuf, std::path::PathBuf),
    MergeBaseFolders(std::path::PathBuf, std::path::PathBuf),
    CompareBaseFolders(std::path::PathBuf, std::path::PathBuf),
    EditText(std::path::PathBuf),
    SourceSaved(std::path::PathBuf),
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
    submenu_item(ui, icons, "menu.session.new", |ui, icons| {
        for (label, kind) in [
            ("session.folder_compare", SessionKind::Folder),
            ("session.folder_merge", SessionKind::FolderMerge),
            ("session.folder_sync", SessionKind::FolderSync),
            ("session.text_compare", SessionKind::Text),
            ("session.text_merge", SessionKind::TextMerge),
            ("session.text_edit", SessionKind::TextEdit),
            ("session.hex_compare", SessionKind::Hex),
            ("session.table_compare", SessionKind::Table),
            ("session.version_compare", SessionKind::Version),
            ("session.registry_compare", SessionKind::Registry),
            ("session.media_compare", SessionKind::Media),
            ("session.picture_compare", SessionKind::Picture),
        ] {
            if menu_item(ui, icons, label, true).clicked() {
                request(ui.ctx(), Request::New(kind));
                ui.close_menu();
            }
        }
    });
    for (label, enabled, command) in [
        ("menu.session.new_tab  Ctrl+T", true, Request::New(SessionKind::Text)),
        ("menu.session.new_window", true, Request::NewWindow),
        ("menu.session.open  Ctrl+Shift+O", true, Request::Open),
    ] {
        if menu_item(ui, icons, label, enabled).clicked() {
            request(ui.ctx(), command);
            ui.close_menu();
        }
    }
    ui.separator();
    submenu_item(ui, icons, "menu.session.load_workspace", |ui, icons| {
        if menu_item(ui, icons, "menu.session.open_workspace", true).clicked() {
            request(ui.ctx(), Request::LoadWorkspace);
            ui.close_menu();
        }
    });
    if menu_item(ui, icons, "menu.session.save_workspace_as", has_session).clicked() {
        request(ui.ctx(), Request::SaveWorkspace);
        ui.close_menu();
    }
    if menu_item(ui, icons, "menu.session.manage_workspaces", true).clicked() {
        request(ui.ctx(), Request::ManageWorkspaces);
        ui.close_menu();
    }
    ui.separator();
    for (label, save_as) in [
        ("menu.session.save  Ctrl+Shift+S", false),
        ("menu.session.save_as", true),
    ] {
        if menu_item(ui, icons, label, has_session).clicked() {
            request(ui.ctx(), Request::Save(save_as));
            ui.close_menu();
        }
    }
}
pub fn suffix(ui: &mut Ui, icons: &mut IconManager, has_pair: bool) {
    ui.separator();
    submenu_item(ui, icons, "menu.session.compare_using", |ui, icons| {
        for (label, kind) in [
            ("session.text_compare", SessionKind::Text),
            ("session.hex_compare", SessionKind::Hex),
            ("session.table_compare", SessionKind::Table),
            ("session.version_compare", SessionKind::Version),
            ("session.registry_compare", SessionKind::Registry),
            ("session.media_compare", SessionKind::Media),
            ("session.picture_compare", SessionKind::Picture),
        ] {
            if menu_item(ui, icons, label, has_pair).clicked() {
                request(ui.ctx(), Request::CompareUsing(kind));
                ui.close_menu();
            }
        }
    });
    if menu_item(ui, icons, "menu.session.merge_files", has_pair).clicked() {
        request(ui.ctx(), Request::MergeFiles);
        ui.close_menu();
    }
    if menu_item(ui, icons, "menu.session.parent_folders", has_pair).clicked() {
        request(ui.ctx(), Request::ParentFolders);
        ui.close_menu();
    }
    ui.separator();
    if menu_item(ui, icons, "menu.session.exit  Ctrl+Q", true).clicked() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        ui.close_menu();
    }
}
pub fn close(ui: &mut Ui, icons: &mut IconManager) {
    if menu_item(ui, icons, "menu.session.close_tab  Ctrl+W", true).clicked() {
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
    if menu_item(ui, icons, "menu.tools.export_settings", true).clicked() {
        crate::i18n::settings_file(true, dialogs);
        ui.close_menu();
    }
    if menu_item(ui, icons, "menu.tools.import_settings", true).clicked() {
        crate::i18n::settings_file(false, dialogs);
        ui.close_menu();
    }
    if menu_item(ui, icons, "menu.tools.restore_defaults", true).clicked() {
        crate::i18n::restore_defaults(dialogs);
        ui.close_menu();
    }
}

pub fn utility_tools(
    ui: &mut Ui,
    icons: &mut IconManager,
    dialogs: &mut dyn super::dialogs::DialogHost,
    snapshot_root: Option<&std::path::Path>,
    edit_file: Option<&std::path::Path>,
) {
    if menu_item(ui, icons, "menu.tools.save_snapshot", true).clicked() {
        let source = snapshot_root
            .filter(|p| crate::folder_source::is_folder_like(p))
            .map(|p| p.to_path_buf())
            .or_else(|| dialogs.folder(true, std::path::Path::new(".")));
        if let Some(root) = source {
            let default_name = root
                .file_name()
                .map(|n| format!("{}.acsnap.json", n.to_string_lossy()))
                .unwrap_or_else(|| "instantaneo.acsnap.json".into());
            if let Some(output) = dialogs.named_file(
                "dialog.save_snapshot_title",
                true,
                root.parent().or(Some(root.as_path())),
                &default_name,
            ) {
                let cancel = std::sync::atomic::AtomicBool::new(false);
                match crate::folder_source::save_snapshot(&root, &output, &cancel) {
                    Ok(count) => dialogs.info(&crate::i18n::tr_fmt(
                        "status.snapshot_saved",
                        &[
                            ("count", &count.to_string()),
                            ("path", &output.display().to_string()),
                        ],
                    )),
                    Err(error) => dialogs.error(&error),
                }
            }
        }
        ui.close_menu();
    }
    if menu_item(ui, icons, "menu.tools.edit_text_file", true).clicked() {
        if let Some(file) = edit_file.filter(|p| p.is_file()) {
            request(ui.ctx(), Request::EditText(file.to_path_buf()));
        } else {
            request(ui.ctx(), Request::New(SessionKind::TextEdit));
        }
        ui.close_menu();
    }
    menu_item(ui, icons, "menu.tools.view_patch", false)
        .on_disabled_hover_text(crate::i18n::tr("common.not_available"));
}
