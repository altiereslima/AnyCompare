use std::path::{Path, PathBuf};

pub const APP_TITLE: &str = "AnyCompare";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveChoice {
    Save,
    Discard,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogField {
    Notice(String),
    Text(String, String),
    Check(String, bool),
    Choice(String, usize, Vec<String>),
}

/// Every application dialog goes through this boundary. The native host owns
/// modality and system chrome; tests supply answers without opening real windows.
pub trait DialogHost {
    /// Mock dialogs must not write the user's global preferences during tests.
    fn persist_settings(&self) -> bool {
        true
    }
    fn named_file(
        &mut self,
        title: &str,
        save: bool,
        current: Option<&Path>,
        default_name: &str,
    ) -> Option<PathBuf> {
        let _ = (title, default_name);
        self.file(save, true, current)
    }
    fn confirm_save(&mut self, left: bool, path: Option<&Path>) -> SaveChoice;
    fn file(&mut self, save: bool, left: bool, current: Option<&Path>) -> Option<PathBuf>;
    fn folder(&mut self, left: bool, current: &Path) -> Option<PathBuf>;
    fn error(&mut self, message: &str);
    fn input(&mut self, title: &str, label: &str, initial: &str) -> Option<String>;
    fn confirm(&mut self, message: &str) -> bool;
    fn fields(&mut self, title: &str, fields: &mut [DialogField]) -> bool;
    fn fields_with_accept(
        &mut self,
        title: &str,
        fields: &mut [DialogField],
        _accept: &str,
    ) -> bool {
        self.fields(title, fields)
    }
    fn recycle(&mut self, paths: &[PathBuf]) -> Result<(), String>;
    fn open_path(&mut self, path: &Path, select: bool) -> Result<(), String>;
    fn report_path(&mut self, extension: &str) -> Option<PathBuf>;
    fn info(&mut self, message: &str);
    fn about(&mut self);
}

#[derive(Default)]
pub struct NativeDialogs<'a> {
    parent: Option<&'a eframe::Frame>,
}

impl<'a> NativeDialogs<'a> {
    pub fn parented(frame: &'a eframe::Frame) -> Self {
        Self {
            parent: Some(frame),
        }
    }
    fn message(
        &self,
        description: &str,
        level: rfd::MessageLevel,
        buttons: rfd::MessageButtons,
    ) -> rfd::MessageDialogResult {
        let mut dialog = rfd::MessageDialog::new()
            .set_title(APP_TITLE)
            .set_description(crate::i18n::tr(description))
            .set_level(level)
            .set_buttons(buttons);
        if let Some(parent) = self.parent {
            dialog = dialog.set_parent(parent);
        }
        dialog.show()
    }
    fn picker(&self, title: &str) -> rfd::FileDialog {
        let mut dialog = rfd::FileDialog::new().set_title(crate::i18n::tr(title));
        if let Some(parent) = self.parent {
            dialog = dialog.set_parent(parent);
        }
        dialog
    }
}

impl DialogHost for NativeDialogs<'_> {
    fn named_file(
        &mut self,
        title: &str,
        save: bool,
        current: Option<&Path>,
        default_name: &str,
    ) -> Option<PathBuf> {
        let mut dialog = self.picker(title).set_file_name(default_name);
        if let Some(parent) = current.and_then(Path::parent).filter(|p| p.is_dir()) {
            dialog = dialog.set_directory(parent);
        }
        if save {
            dialog.save_file()
        } else {
            dialog.pick_file()
        }
    }
    fn confirm_save(&mut self, left: bool, path: Option<&Path>) -> SaveChoice {
        let side = crate::i18n::tr(if left { "esquerdo" } else { "direito" });
        let mut question = crate::i18n::tr(&format!("Salvar alterações ao arquivo {side}?"));
        if let Some(path) = path {
            question.push_str(&format!("\n\n{}", path.display()));
        }
        match self.message(
            &question,
            rfd::MessageLevel::Warning,
            rfd::MessageButtons::YesNoCancel,
        ) {
            rfd::MessageDialogResult::Yes => SaveChoice::Save,
            rfd::MessageDialogResult::No => SaveChoice::Discard,
            _ => SaveChoice::Cancel,
        }
    }
    fn file(&mut self, save: bool, left: bool, current: Option<&Path>) -> Option<PathBuf> {
        let side = crate::i18n::tr(if left { "esquerdo" } else { "direito" });
        let mut dialog = self.picker(&format!(
            "{} arquivo {side}",
            crate::i18n::tr(if save { "Salvar" } else { "Abrir" })
        ));
        if let Some(path) = current {
            if let Some(parent) = path.parent().filter(|path| path.is_dir()) {
                dialog = dialog.set_directory(parent);
            }
            if save {
                if let Some(name) = path.file_name() {
                    dialog = dialog.set_file_name(name.to_string_lossy());
                }
            }
        }
        if save {
            dialog.save_file()
        } else {
            dialog.pick_file()
        }
    }
    fn folder(&mut self, left: bool, current: &Path) -> Option<PathBuf> {
        let mut dialog = self.picker(if left {
            "Escolher pasta esquerda"
        } else {
            "Escolher pasta direita"
        });
        if current.is_dir() {
            dialog = dialog.set_directory(current);
        }
        dialog.pick_folder()
    }
    fn error(&mut self, message: &str) {
        self.message(message, rfd::MessageLevel::Error, rfd::MessageButtons::Ok);
    }
    fn input(&mut self, title: &str, label: &str, initial: &str) -> Option<String> {
        super::native_prompt::input(
            self.parent,
            &format!("{} - {APP_TITLE}", crate::i18n::tr(title)),
            &crate::i18n::tr(label),
            initial,
        )
    }
    fn confirm(&mut self, message: &str) -> bool {
        self.message(
            message,
            rfd::MessageLevel::Warning,
            rfd::MessageButtons::YesNo,
        ) == rfd::MessageDialogResult::Yes
    }
    fn fields(&mut self, title: &str, fields: &mut [DialogField]) -> bool {
        self.fields_with_accept(title, fields, "OK")
    }
    fn fields_with_accept(
        &mut self,
        title: &str,
        fields: &mut [DialogField],
        accept: &str,
    ) -> bool {
        let mut translated = fields
            .iter()
            .cloned()
            .map(|field| match field {
                DialogField::Notice(text) => DialogField::Notice(crate::i18n::tr(&text)),
                DialogField::Text(label, value) => {
                    DialogField::Text(crate::i18n::tr(&label), value)
                }
                DialogField::Check(label, value) => {
                    DialogField::Check(crate::i18n::tr(&label), value)
                }
                DialogField::Choice(label, value, choices) => DialogField::Choice(
                    crate::i18n::tr(&label),
                    value,
                    choices.iter().map(|c| crate::i18n::tr(c)).collect(),
                ),
            })
            .collect::<Vec<_>>();
        let title = crate::i18n::tr(title);
        let accepted = super::native_prompt::form_with_accept(
            self.parent,
            &format!("{title} - {APP_TITLE}"),
            &mut translated,
            &crate::i18n::tr(accept),
        );
        if accepted {
            for (field, translated) in fields.iter_mut().zip(translated) {
                match (field, translated) {
                    (DialogField::Text(_, value), DialogField::Text(_, next)) => *value = next,
                    (DialogField::Check(_, value), DialogField::Check(_, next)) => *value = next,
                    (DialogField::Choice(_, value, _), DialogField::Choice(_, next, _)) => {
                        *value = next
                    }
                    _ => {}
                }
            }
        }
        accepted
    }
    fn recycle(&mut self, paths: &[PathBuf]) -> Result<(), String> {
        super::native_files::recycle(self.parent, paths)
    }
    fn open_path(&mut self, path: &Path, select: bool) -> Result<(), String> {
        super::native_files::open(path, select)
    }
    fn report_path(&mut self, extension: &str) -> Option<PathBuf> {
        self.picker("Salvar relatório")
            .set_file_name(crate::i18n::tr(&format!(
                "AnyCompare-relatorio.{extension}"
            )))
            .add_filter(
                crate::i18n::tr(if extension == "html" { "HTML" } else { "Texto" }),
                &[extension],
            )
            .save_file()
    }
    fn info(&mut self, message: &str) {
        self.message(message, rfd::MessageLevel::Info, rfd::MessageButtons::Ok);
    }
    fn about(&mut self) {
        self.message(&format!("AnyCompare {}\nComparação de arquivos e pastas.\n\nReconstrução em desenvolvimento.",env!("CARGO_PKG_VERSION")),rfd::MessageLevel::Info,rfd::MessageButtons::Ok);
    }
}

#[cfg(test)]
#[derive(Default)]
pub struct TestDialogs {
    pub choices: std::collections::VecDeque<SaveChoice>,
    pub files: std::collections::VecDeque<Option<PathBuf>>,
    pub prompts: Vec<bool>,
    pub errors: Vec<String>,
    pub inputs: std::collections::VecDeque<Option<String>>,
    pub confirmations: std::collections::VecDeque<bool>,
    pub field_answers: std::collections::VecDeque<Option<Vec<DialogField>>>,
    pub recycled: Vec<PathBuf>,
    pub opened: Vec<PathBuf>,
    pub information: Vec<String>,
}

#[cfg(test)]
impl DialogHost for TestDialogs {
    fn persist_settings(&self) -> bool {
        false
    }
    fn confirm_save(&mut self, left: bool, _: Option<&Path>) -> SaveChoice {
        self.prompts.push(left);
        self.choices.pop_front().expect("Unexpected save prompt")
    }
    fn file(&mut self, _: bool, _: bool, _: Option<&Path>) -> Option<PathBuf> {
        self.files.pop_front().expect("Unexpected file dialog")
    }
    fn folder(&mut self, _: bool, _: &Path) -> Option<PathBuf> {
        panic!("Unexpected folder dialog")
    }
    fn error(&mut self, message: &str) {
        self.errors.push(message.into());
    }
    fn input(&mut self, _: &str, _: &str, _: &str) -> Option<String> {
        self.inputs.pop_front().expect("Unexpected input dialog")
    }
    fn confirm(&mut self, _: &str) -> bool {
        self.confirmations
            .pop_front()
            .expect("Unexpected confirmation")
    }
    fn fields(&mut self, _: &str, fields: &mut [DialogField]) -> bool {
        if let Some(answer) = self
            .field_answers
            .pop_front()
            .expect("Unexpected settings dialog")
        {
            fields.clone_from_slice(&answer);
            true
        } else {
            false
        }
    }
    fn recycle(&mut self, paths: &[PathBuf]) -> Result<(), String> {
        self.recycled.extend_from_slice(paths);
        Ok(())
    }
    fn open_path(&mut self, path: &Path, _: bool) -> Result<(), String> {
        self.opened.push(path.into());
        Ok(())
    }
    fn report_path(&mut self, _: &str) -> Option<PathBuf> {
        self.files.pop_front().expect("Unexpected report picker")
    }
    fn info(&mut self, message: &str) {
        self.information.push(message.into());
    }
    fn about(&mut self) {}
}
