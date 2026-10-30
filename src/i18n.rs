use crate::ui::dialogs::{DialogField, DialogHost};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{LazyLock, Mutex, RwLock},
};

#[derive(Clone, Deserialize)]
pub struct Catalog {
    pub locale: String,
    pub name: String,
    #[serde(default)]
    pub translations: BTreeMap<String, String>,
}
static CATALOGS: LazyLock<Vec<Catalog>> = LazyLock::new(|| {
    let mut catalogs = vec![
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/pt-BR.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/en.json")).unwrap(),
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if let Ok(entries) = std::fs::read_dir(parent.join("languages")) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_none_or(|ext| ext != "json") {
                        continue;
                    }
                    if entry.metadata().is_ok_and(|m| m.len() < 4 * 1024 * 1024) {
                        if let Ok(text) = std::fs::read_to_string(path) {
                            if let Ok(catalog) = serde_json::from_str::<Catalog>(&text) {
                                if !catalog.locale.is_empty()
                                    && catalog
                                        .locale
                                        .chars()
                                        .all(|c| c.is_ascii_alphanumeric() || c == '-')
                                    && !catalogs
                                        .iter()
                                        .any(|c| c.locale.eq_ignore_ascii_case(&catalog.locale))
                                {
                                    catalogs.push(catalog);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    catalogs
});
static ACTIVE: LazyLock<RwLock<String>> = LazyLock::new(|| RwLock::new("pt-BR".into()));
#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    #[serde(default = "auto")]
    language: String,
    #[serde(flatten)]
    other: BTreeMap<String, serde_json::Value>,
}
fn auto() -> String {
    "auto".into()
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            language: auto(),
            other: BTreeMap::new(),
        }
    }
}
struct Config {
    settings: Settings,
    error: Option<String>,
}
fn settings_path() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_else(|| ".".into()))
        .join("AnyCompare/settings.json")
}
static CONFIG: LazyLock<Mutex<Config>> = LazyLock::new(|| {
    let path = settings_path();
    let (settings, error) = if path.exists() {
        match std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()))
        {
            Ok(settings) => (settings, None),
            Err(error) => (Settings::default(), Some(error)),
        }
    } else {
        (Settings::default(), None)
    };
    Mutex::new(Config { settings, error })
});
pub fn system_locale() -> String {
    use windows_sys::Win32::Globalization::{GetUserDefaultUILanguage, LCIDToLocaleName};
    let mut name = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH, including the NUL.
    let count = unsafe {
        LCIDToLocaleName(
            GetUserDefaultUILanguage() as u32,
            name.as_mut_ptr(),
            name.len() as i32,
            0,
        )
    };
    if count > 0 {
        String::from_utf16_lossy(&name[..count as usize - 1])
    } else {
        "en".into()
    }
}
fn resolve(preference: &str, system: &str, locales: &[Catalog]) -> String {
    let requested = if preference == "auto" {
        system
    } else {
        preference
    };
    locales
        .iter()
        .find(|c| c.locale.eq_ignore_ascii_case(requested))
        .or_else(|| {
            let primary = requested.split('-').next().unwrap_or(requested);
            locales.iter().find(|c| {
                c.locale
                    .split('-')
                    .next()
                    .is_some_and(|tag| tag.eq_ignore_ascii_case(primary))
            })
        })
        .map(|c| c.locale.clone())
        .unwrap_or("en".into())
}
fn activate(preference: &str) {
    let selected = resolve(preference, &system_locale(), &CATALOGS);
    *ACTIVE.write().unwrap() = selected.clone();
    let windows_locale = if selected == "en" { "en-US" } else { &selected };
    let wide: Vec<_> = windows_locale.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let id = windows_sys::Win32::Globalization::LocaleNameToLCID(wide.as_ptr(), 0);
        if id != 0 {
            windows_sys::Win32::Globalization::SetThreadUILanguage(id as u16);
        }
    }
}
pub fn initialize(override_language: Option<&str>) {
    let preference = override_language
        .map(str::to_owned)
        .unwrap_or_else(|| CONFIG.lock().unwrap().settings.language.clone());
    activate(&preference);
}
pub fn language() -> String {
    ACTIVE.read().unwrap().clone()
}
pub fn language_field() -> DialogField {
    let preference = CONFIG.lock().unwrap().settings.language.clone();
    let selected = if preference == "auto" {
        0
    } else {
        CATALOGS
            .iter()
            .position(|c| c.locale.eq_ignore_ascii_case(&preference))
            .map(|i| i + 1)
            .unwrap_or(0)
    };
    let choices = std::iter::once(format!("Automático ({})", system_locale()))
        .chain(CATALOGS.iter().map(|c| c.name.clone()))
        .collect();
    DialogField::Choice("Idioma:".into(), selected, choices)
}
pub fn apply_language_field(field: &DialogField, dialogs: &mut dyn DialogHost) {
    if !dialogs.persist_settings() {
        return;
    }
    let DialogField::Choice(_, selected, _) = field else {
        return;
    };
    let preference = if *selected == 0 {
        "auto".into()
    } else {
        CATALOGS
            .get(selected - 1)
            .map(|c| c.locale.clone())
            .unwrap_or_else(auto)
    };
    let mut config = CONFIG.lock().unwrap();
    if let Some(error) = &config.error {
        dialogs.error(&format!(
            "Não foi possível carregar as configurações: {error}"
        ));
        return;
    }
    let mut next = config.settings.clone();
    next.language = preference.clone();
    let path = settings_path();
    let result = (|| -> Result<(), String> {
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        crate::document::atomic_write(
            &path,
            &serde_json::to_vec_pretty(&next).map_err(|e| e.to_string())?,
        )
    })();
    match result {
        Ok(()) => {
            config.settings = next;
            drop(config);
            activate(&preference);
        }
        Err(error) => dialogs.error(&error),
    }
}
pub fn configure(dialogs: &mut dyn DialogHost) {
    let mut fields = [language_field()];
    if dialogs.fields("Opções", &mut fields) {
        apply_language_field(&fields[0], dialogs);
    }
}

pub fn settings_file(export: bool, dialogs: &mut dyn DialogHost) {
    if !dialogs.persist_settings() {
        return;
    }
    let Some(path) = dialogs.named_file(
        if export {
            "Exportar Configurações..."
        } else {
            "Importar Configurações..."
        },
        export,
        None,
        "AnyCompare-settings.json",
    ) else {
        return;
    };
    let result = (|| -> Result<(), String> {
        let mut config = CONFIG.lock().unwrap();
        if let Some(error) = &config.error {
            return Err(format!(
                "Não foi possível carregar as configurações: {error}"
            ));
        }
        if export {
            crate::document::atomic_write(
                &path,
                &serde_json::to_vec_pretty(&config.settings).map_err(|e| e.to_string())?,
            )?;
        } else {
            let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            if file.metadata().map_err(|e| e.to_string())?.len() > 4 * 1024 * 1024 {
                return Err("Arquivo de configurações grande demais.".into());
            }
            let settings: Settings = serde_json::from_reader(file).map_err(|e| e.to_string())?;
            if settings.language != "auto"
                && !CATALOGS
                    .iter()
                    .any(|c| c.locale.eq_ignore_ascii_case(&settings.language))
            {
                return Err("O idioma das configurações não está disponível.".into());
            }
            if !dialogs.confirm("Importar e substituir as configurações do AnyCompare?") {
                return Ok(());
            }
            let target = settings_path();
            std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            crate::document::atomic_write(
                &target,
                &serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?,
            )?;
            config.settings = settings;
            let language = config.settings.language.clone();
            drop(config);
            activate(&language);
        }
        Ok(())
    })();
    if let Err(error) = result {
        dialogs.error(&error);
    }
}

pub fn restore_defaults(dialogs: &mut dyn DialogHost) {
    if !dialogs.persist_settings() {
        return;
    }
    if !dialogs.confirm("Restaurar as configurações padrão do AnyCompare?") {
        return;
    }
    let result = (|| -> Result<(), String> {
        let mut config = CONFIG.lock().unwrap();
        let target = settings_path();
        std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
        // Keep a recoverable copy, including a previously invalid settings file.
        if target.exists() {
            let backup = target.with_extension(format!(
                "{}.bak",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            ));
            std::fs::copy(&target, backup).map_err(|e| e.to_string())?;
        }
        let settings = Settings::default();
        crate::document::atomic_write(
            &target,
            &serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?,
        )?;
        config.settings = settings;
        config.error = None;
        drop(config);
        activate("auto");
        Ok(())
    })();
    if let Err(error) = result {
        dialogs.error(&error);
    }
}

// Translation keys are source UI strings. Values and file contents remain data.
// Formatted messages are matched against catalog templates with named/positional
// placeholders; captured paths and numbers are preserved verbatim.
struct Template {
    pattern: regex::Regex,
    english: String,
    slots: Vec<String>,
    specificity: usize,
}
fn template(source: &str, target: &str) -> Option<Template> {
    if !source.contains('{') {
        return None;
    }
    let mut pattern = String::from("(?s)^");
    let mut slots = Vec::new();
    let mut at = 0;
    let mut position = 0;
    let mut specificity = 0;
    while let Some(start) = source[at..].find('{') {
        let start = at + start;
        if source[start..].starts_with("{{") {
            return None;
        }
        pattern.push_str(&regex::escape(&source[at..start]));
        specificity += start - at;
        let end = source[start + 1..].find('}')? + start + 1;
        let raw = &source[start + 1..end];
        let key = raw.split(':').next().unwrap();
        slots.push(if key.is_empty() {
            let value = position.to_string();
            position += 1;
            value
        } else {
            key.into()
        });
        pattern.push_str("(.*?)");
        at = end + 1;
    }
    pattern.push_str(&regex::escape(&source[at..]));
    specificity += source.len() - at;
    pattern.push('$');
    Some(Template {
        pattern: regex::Regex::new(&pattern).ok()?,
        english: target.into(),
        slots,
        specificity,
    })
}
static TEMPLATES: LazyLock<BTreeMap<String, Vec<Template>>> = LazyLock::new(|| {
    CATALOGS
        .iter()
        .map(|c| {
            (c.locale.clone(), {
                let mut templates: Vec<_> = c
                    .translations
                    .iter()
                    .filter(|(source, target)| source != target)
                    .filter_map(|(source, target)| template(source, target))
                    .collect();
                templates.sort_by_key(|t| std::cmp::Reverse(t.specificity));
                templates
            })
        })
        .collect()
});
fn translate(locale: &str, text: &str) -> String {
    let Some(catalog) = CATALOGS.iter().find(|c| c.locale == locale) else {
        return text.into();
    };
    if let Some(value) = catalog.translations.get(text) {
        return value.clone();
    }
    if let Some(templates) = TEMPLATES.get(locale) {
        for template in templates {
            if let Some(captures) = template.pattern.captures(text) {
                let slots: BTreeMap<_, _> = template
                    .slots
                    .iter()
                    .enumerate()
                    .map(|(i, key)| (key.clone(), captures[i + 1].to_owned()))
                    .collect();
                let mut result = String::new();
                let mut at = 0;
                let mut position = 0;
                while let Some(start) = template.english[at..].find('{') {
                    let start = at + start;
                    result.push_str(&template.english[at..start]);
                    let Some(end) = template.english[start + 1..]
                        .find('}')
                        .map(|i| i + start + 1)
                    else {
                        break;
                    };
                    let raw = &template.english[start + 1..end];
                    let key = raw.split(':').next().unwrap();
                    let key = if key.is_empty() {
                        let key = position.to_string();
                        position += 1;
                        key
                    } else {
                        key.into()
                    };
                    result.push_str(slots.get(&key).map(String::as_str).unwrap_or(""));
                    at = end + 1;
                }
                result.push_str(&template.english[at..]);
                return result;
            }
        }
    }
    text.into()
}
pub fn tr(text: &str) -> String {
    translate(&ACTIVE.read().unwrap(), text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_locale_override_fallback_and_parameter_preservation() {
        assert_eq!(resolve("auto", "pt-BR", &CATALOGS), "pt-BR");
        assert_eq!(resolve("auto", "pt-PT", &CATALOGS), "pt-BR");
        assert_eq!(resolve("auto", "en-GB", &CATALOGS), "en");
        assert_eq!(resolve("auto", "ja-JP", &CATALOGS), "en");
        assert_eq!(resolve("en", "pt-BR", &CATALOGS), "en");
        assert_eq!(translate("en", "Página Inicial"), "Home");
        assert_eq!(
            translate("en", "Relatório salvo em:\nC:\\dados\\ação.html"),
            "Report saved to:\nC:\\dados\\ação.html"
        );
        assert_eq!(
            translate("en", "3 seções diferentes"),
            "3 difference sections"
        );
        assert_eq!(
            translate(
                "en",
                "1406 / 34494 bytes · 34237 diferentes · Posição 0xA · Read-only · Valor: 4321"
            ),
            "1406 / 34494 bytes · 34237 different · Position 0xA · Read-only · Value: 4321"
        );
    }
}
