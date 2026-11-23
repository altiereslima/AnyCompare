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
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/en.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/pt-BR.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/de.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/es.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/fr.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/ga.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/it.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/ja.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/ko.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/ru.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/sv.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/sl.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/th.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/vi.json")).unwrap(),
        serde_json::from_str::<Catalog>(include_str!("../assets/locales/zh-CN.json")).unwrap(),
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

static ACTIVE: LazyLock<RwLock<String>> = LazyLock::new(|| RwLock::new("en".into()));

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppPreferences {
    pub open_in_new_tab: bool,
    pub confirm_close_tabs: bool,
    pub explorer_context_menu: bool,
    pub editor_font_size: u8,
    pub alternate_row_shading: bool,
    pub folder_compare_content: bool,
    pub folder_show_tree: bool,
    pub folder_show_log: bool,
    pub text_allow_edit: bool,
    pub text_line_numbers: bool,
    pub text_show_context: bool,
    pub text_show_details: bool,
    pub text_show_thumbnail: bool,
    pub text_show_whitespace: bool,
    pub text_word_wrap: bool,
    pub table_has_header: bool,
    pub table_sort_by_key: bool,
    pub table_ignore_minor: bool,
    pub table_hide_same_columns: bool,
    pub picture_default_mode: u8,
    pub picture_tolerance: u8,
    pub picture_ghost: bool,
    pub picture_auto_scale: bool,
    pub picture_fit_to_window: bool,
    pub hex_bytes_per_row: usize,
    pub hex_show_ascii: bool,
    pub hex_addresses: bool,
    pub hex_little_endian: bool,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            open_in_new_tab: true,
            confirm_close_tabs: true,
            explorer_context_menu: true,
            editor_font_size: 13,
            alternate_row_shading: true,
            folder_compare_content: true,
            folder_show_tree: true,
            folder_show_log: true,
            text_allow_edit: true,
            text_line_numbers: false,
            text_show_context: false,
            text_show_details: true,
            text_show_thumbnail: true,
            text_show_whitespace: false,
            text_word_wrap: false,
            table_has_header: false,
            table_sort_by_key: false,
            table_ignore_minor: false,
            table_hide_same_columns: false,
            picture_default_mode: 0,
            picture_tolerance: 0,
            picture_ghost: true,
            picture_auto_scale: false,
            picture_fit_to_window: true,
            hex_bytes_per_row: 16,
            hex_show_ascii: true,
            hex_addresses: true,
            hex_little_endian: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Settings {
    #[serde(default = "auto")]
    language: String,
    #[serde(default)]
    preferences: AppPreferences,
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
            preferences: AppPreferences::default(),
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
    #[cfg(windows)]
    {
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
            return String::from_utf16_lossy(&name[..count as usize - 1]);
        }
    }
    #[cfg(not(windows))]
    {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(val) = std::env::var(var) {
                let tag = val.split(['.', '@']).next().unwrap_or("").replace('_', "-");
                if !tag.is_empty() && !tag.eq_ignore_ascii_case("c") && !tag.eq_ignore_ascii_case("posix") {
                    return tag;
                }
            }
        }
    }
    "en".into()
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
    #[cfg(windows)]
    {
        let windows_locale = match selected.as_str() {
            "en" => "en-US",
            "de" => "de-DE",
            "es" => "es-ES",
            "fr" => "fr-FR",
            "ga" => "ga-IE",
            "it" => "it-IT",
            "ja" => "ja-JP",
            "ko" => "ko-KR",
            "ru" => "ru-RU",
            "sv" => "sv-SE",
            "sl" => "sl-SI",
            "th" => "th-TH",
            "vi" => "vi-VN",
            other => other,
        };
        let wide: Vec<_> = windows_locale.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let id = windows_sys::Win32::Globalization::LocaleNameToLCID(wide.as_ptr(), 0);
            if id != 0 {
                windows_sys::Win32::Globalization::SetThreadUILanguage(id as u16);
            }
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

pub fn language_preference() -> String {
    CONFIG.lock().unwrap().settings.language.clone()
}

pub fn available_languages() -> Vec<(String, String)> {
    std::iter::once((
        "auto".to_owned(),
        format!("{} ({})", tr("app.language.auto"), system_locale()),
    ))
    .chain(CATALOGS.iter().map(|c| (c.locale.clone(), c.name.clone())))
    .collect()
}

pub fn preferences() -> AppPreferences {
    CONFIG.lock().unwrap().settings.preferences.clone()
}

pub fn save_options(
    language_pref: &str,
    prefs: AppPreferences,
    dialogs: &mut dyn DialogHost,
) -> bool {
    if !dialogs.persist_settings() {
        return true;
    }
    let mut config = CONFIG.lock().unwrap();
    if let Some(error) = &config.error {
        dialogs.error(&tr_fmt(
            "error.could_not_load_settings",
            &[("error", error)],
        ));
        return false;
    }
    let prev_shellex = config.settings.preferences.explorer_context_menu;
    let next_shellex = prefs.explorer_context_menu;
    let mut next = config.settings.clone();
    next.language = language_pref.to_owned();
    next.preferences = prefs;
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
            activate(language_pref);
            if prev_shellex != next_shellex {
                apply_explorer_context_menu(next_shellex);
            }
            true
        }
        Err(error) => {
            dialogs.error(&error);
            false
        }
    }
}

fn apply_explorer_context_menu(enable: bool) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let Some(dir) = exe.parent() else {
            return;
        };
        let script = dir.join("register_shellex.ps1");
        if !script.is_file() {
            return;
        }
        let action = if enable { "Install" } else { "Uninstall" };
        let _ = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-File",
            ])
            .arg(&script)
            .args(["-Action", action, "-InstallDir"])
            .arg(dir)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = enable;
    }
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
    let choices = std::iter::once(tr_fmt("status.automatic", &[("", &system_locale())]))
        .chain(CATALOGS.iter().map(|c| c.name.clone()))
        .collect();
    DialogField::Choice("app.language.label".into(), selected, choices)
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
        dialogs.error(&tr_fmt(
            "error.could_not_load_settings",
            &[("error", error)],
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
    if dialogs.fields("common.options", &mut fields) {
        apply_language_field(&fields[0], dialogs);
    }
}

pub fn settings_file(export: bool, dialogs: &mut dyn DialogHost) {
    if !dialogs.persist_settings() {
        return;
    }
    let Some(path) = dialogs.named_file(
        if export {
            "menu.tools.export_settings"
        } else {
            "menu.tools.import_settings"
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
            return Err(tr_fmt(
                "error.could_not_load_settings",
                &[("error", error)],
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
                return Err(tr("app.settings_file_is_too_large"));
            }
            let settings: Settings = serde_json::from_reader(file).map_err(|e| e.to_string())?;
            if settings.language != "auto"
                && !CATALOGS
                    .iter()
                    .any(|c| c.locale.eq_ignore_ascii_case(&settings.language))
            {
                return Err(tr("app.the_language_in_the_settings_file_is_not"));
            }
            if !dialogs.confirm("app.import_and_replace_anycompare_settings") {
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
    if !dialogs.confirm("app.restore_the_default_anycompare_settings") {
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

/// Semantic-key translation lookup with automatic English fallback.
///
/// Unknown strings (such as file paths or already-formatted messages) are returned verbatim.
pub fn translate(locale: &str, key: &str) -> String {
    if let Some(catalog) = CATALOGS.iter().find(|c| c.locale == locale) {
        if let Some(value) = catalog.translations.get(key) {
            return value.clone();
        }
    }
    if locale != "en" {
        if let Some(en) = CATALOGS.iter().find(|c| c.locale == "en") {
            if let Some(value) = en.translations.get(key) {
                return value.clone();
            }
        }
    }
    key.into()
}

pub fn tr(key: &str) -> String {
    translate(&ACTIVE.read().unwrap(), key)
}

/// Translate a semantic key and substitute `{name}` or positional `{}` / `{0}` placeholders.
pub fn tr_fmt(key: &str, args: &[(&str, &str)]) -> String {
    let template = tr(key);
    format_template(&template, args)
}

fn format_template(template: &str, args: &[(&str, &str)]) -> String {
    if args.is_empty() || !template.contains('{') {
        return template.to_owned();
    }
    let mut out = String::with_capacity(template.len() + 32);
    let mut at = 0usize;
    let mut pos_idx = 0usize;
    while let Some(rel) = template[at..].find('{') {
        let start = at + rel;
        out.push_str(&template[at..start]);
        let Some(end_rel) = template[start + 1..].find('}') else {
            out.push_str(&template[start..]);
            return out;
        };
        let end = start + 1 + end_rel;
        let raw = &template[start + 1..end];
        let name = raw.split(':').next().unwrap_or("");
        let replacement = if name.is_empty() {
            let v = args.get(pos_idx).map(|(_, val)| *val);
            pos_idx += 1;
            v
        } else if let Ok(idx) = name.parse::<usize>() {
            args.get(idx).map(|(_, val)| *val)
        } else {
            args.iter().find(|(k, _)| *k == name).map(|(_, val)| *val)
        };
        if let Some(val) = replacement {
            out.push_str(val);
        } else {
            out.push_str(&template[start..=end]);
        }
        at = end + 1;
    }
    out.push_str(&template[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_locale_override_fallback_and_formatting() {
        assert_eq!(resolve("auto", "pt-BR", &CATALOGS), "pt-BR");
        assert_eq!(resolve("auto", "pt-PT", &CATALOGS), "pt-BR");
        assert_eq!(resolve("auto", "en-GB", &CATALOGS), "en");
        assert_eq!(resolve("auto", "de-DE", &CATALOGS), "de");
        assert_eq!(resolve("auto", "es-MX", &CATALOGS), "es");
        assert_eq!(resolve("auto", "fr-FR", &CATALOGS), "fr");
        assert_eq!(resolve("auto", "ga-IE", &CATALOGS), "ga");
        assert_eq!(resolve("auto", "it-IT", &CATALOGS), "it");
        assert_eq!(resolve("auto", "ja-JP", &CATALOGS), "ja");
        assert_eq!(resolve("auto", "ko-KR", &CATALOGS), "ko");
        assert_eq!(resolve("auto", "ru-RU", &CATALOGS), "ru");
        assert_eq!(resolve("auto", "sv-SE", &CATALOGS), "sv");
        assert_eq!(resolve("auto", "sl-SI", &CATALOGS), "sl");
        assert_eq!(resolve("auto", "th-TH", &CATALOGS), "th");
        assert_eq!(resolve("auto", "vi-VN", &CATALOGS), "vi");
        assert_eq!(resolve("auto", "zh-CN", &CATALOGS), "zh-CN");
        assert_eq!(resolve("auto", "zh-Hans-CN", &CATALOGS), "zh-CN");
        assert_eq!(resolve("auto", "ar-SA", &CATALOGS), "en");
        assert_eq!(resolve("en", "pt-BR", &CATALOGS), "en");

        assert_eq!(translate("en", "session.home"), "Home");
        assert_eq!(translate("pt-BR", "session.home"), "Página Inicial");
        assert_eq!(translate("ja", "session.home"), "ホーム");
        // Unknown locale falls back cleanly to English.
        assert_eq!(translate("xx-ZZ", "session.home"), "Home");

        assert_eq!(
            format_template(
                &translate("en", "status.report_saved_to"),
                &[("", r"C:\dados\ação.html")]
            ),
            "Report saved to:\nC:\\dados\\ação.html"
        );
        assert_eq!(
            format_template(&translate("pt-BR", "status.difference_sections"), &[("", "3")]),
            "Seções com diferenças: 3"
        );
        assert_eq!(
            format_template(
                &translate("en", "status.bytes_different_position_0x_value"),
                &[
                    ("", "1406"),
                    ("", "34494"),
                    ("", "34237"),
                    ("", "A"),
                    ("", "Read-only"),
                    ("value", "4321"),
                ]
            ),
            "1406 / 34494 bytes · 34237 different · Position 0xA · Read-only · Value: 4321"
        );
    }

    #[test]
    fn all_fifteen_builtin_locales_have_complete_semantic_key_and_placeholder_parity() {
        let expected = [
            "en", "pt-BR", "de", "es", "fr", "ga", "it", "ja", "ko", "ru", "sv", "sl", "th", "vi",
            "zh-CN",
        ];
        let key_re = regex::Regex::new(r"^[a-z][a-z0-9_]*(\.[a-z0-9_]+)+$").unwrap();
        let ph_re = regex::Regex::new(r"\{[^}]*\}").unwrap();

        for loc in expected {
            assert!(
                CATALOGS.iter().any(|c| c.locale == loc),
                "missing builtin catalog {loc}"
            );
        }
        let en = CATALOGS.iter().find(|c| c.locale == "en").unwrap();
        assert!(en.translations.len() >= 800, "unexpectedly small en catalog");

        for key in en.translations.keys() {
            assert!(key_re.is_match(key), "invalid semantic key format: {key:?}");
        }

        for loc in expected {
            let cat = CATALOGS.iter().find(|c| c.locale == loc).unwrap();
            assert_eq!(
                cat.translations.len(),
                en.translations.len(),
                "locale {loc} translation count mismatch"
            );
            for (key, en_val) in &en.translations {
                let val = cat
                    .translations
                    .get(key)
                    .unwrap_or_else(|| panic!("locale {loc} missing key {key:?}"));
                assert!(!val.trim().is_empty(), "locale {loc} empty value for {key:?}");
                let mut en_phs: Vec<&str> = ph_re.find_iter(en_val).map(|m| m.as_str()).collect();
                let mut loc_phs: Vec<&str> = ph_re.find_iter(val).map(|m| m.as_str()).collect();
                en_phs.sort_unstable();
                loc_phs.sort_unstable();
                assert_eq!(
                    loc_phs, en_phs,
                    "placeholder mismatch in locale {loc} for key {key:?}: en={en_val:?} vs loc={val:?}"
                );
            }
            assert_ne!(
                translate(loc, "session.home"),
                "session.home",
                "locale {loc} did not translate session.home"
            );
        }
    }
}
