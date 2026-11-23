//! Native 64-bit Windows Explorer Context Menu COM extension (`IExplorerCommand`)
//! for AnyCompare.
//!
//! Works simultaneously as:
//! 1. **Windows 11 Modern Top-Level Context Menu** via `AnyCompareShellEx.msix`
//!    (`windows.fileExplorerContextMenus` + `windows.comServer` SurrogateServer).
//! 2. **Windows 10 & Windows 11 Classic Context Menu ("Mostrar mais opções")** via
//!    `ExplorerCommandHandler` registry verbs registered under `HKCU\Software\Classes`.

#![cfg_attr(not(windows), allow(dead_code))]

use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    sync::atomic::{AtomicIsize, AtomicUsize, Ordering},
};

use windows::{
    core::{implement, w, Error, IUnknown, Interface, Result, GUID, HRESULT, PCWSTR, PWSTR},
    Win32::{
        Foundation::{
            BOOL, CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_INVALIDARG,
            E_NOTIMPL, E_POINTER, HINSTANCE, HMODULE, HWND, MAX_PATH, S_FALSE, S_OK,
        },
        Globalization::GetUserDefaultUILanguage,
        System::{
            Com::{CoTaskMemFree, IBindCtx, IClassFactory, IClassFactory_Impl},
            LibraryLoader::GetModuleFileNameW,
            Registry::{
                RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegGetValueW, RegSetValueExW, HKEY,
                HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
                RRF_RT_REG_SZ,
            },
        },
        UI::{
            Shell::{
                IEnumExplorerCommand, IExplorerCommand, IExplorerCommand_Impl, IShellItemArray,
                SHStrDupW, ShellExecuteW, ECF_DEFAULT, ECS_ENABLED, ECS_HIDDEN, SIGDN_FILESYSPATH,
            },
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
};

/// COM CLSID registered in both `AppxManifest.xml` and `HKCU\Software\Classes\CLSID`.
pub const CLSID_ANYCOMPARE_COMMAND: GUID = GUID::from_u128(0xA4E8C7D1_93B2_4F5A_8E11_5B9C3D7A2F01);
pub const CLSID_STR: &str = "{A4E8C7D1-93B2-4F5A-8E11-5B9C3D7A2F01}";
const CANONICAL_COMMAND_ID: GUID = GUID::from_u128(0xA4E8C7D1_93B2_4F5A_8E11_5B9C3D7A2F02);

const DLL_PROCESS_ATTACH: u32 = 1;

static MODULE_HANDLE: AtomicIsize = AtomicIsize::new(0);
static LIVE_OBJECTS: AtomicUsize = AtomicUsize::new(0);
static SERVER_LOCKS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Debug, PartialEq, Eq)]
enum MenuAction {
    /// Two compatible items selected (both files or both folders) — instant compare.
    ComparePair(PathBuf, PathBuf),
    /// One item selected, and a compatible left item is already remembered.
    CompareWithSaved { left: PathBuf, right: PathBuf },
    /// One item selected, and it matches the remembered left item — clear it.
    ClearSavedLeft,
    /// One item selected, no compatible left item yet — remember it as left.
    SelectLeft { path: PathBuf, is_dir: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UiLang {
    PtBr,
    En,
    De,
    Es,
    Fr,
    Ga,
    It,
    Ja,
    Ko,
    Ru,
    Sv,
    Sl,
    Th,
    Vi,
    ZhCn,
}

fn lang_from_primary_id(primary_id: u16) -> UiLang {
    match primary_id {
        0x16 => UiLang::PtBr, // LANG_PORTUGUESE
        0x07 => UiLang::De,   // LANG_GERMAN
        0x0A => UiLang::Es,   // LANG_SPANISH
        0x0C => UiLang::Fr,   // LANG_FRENCH
        0x3C => UiLang::Ga,   // LANG_IRISH
        0x10 => UiLang::It,   // LANG_ITALIAN
        0x11 => UiLang::Ja,   // LANG_JAPANESE
        0x12 => UiLang::Ko,   // LANG_KOREAN
        0x19 => UiLang::Ru,   // LANG_RUSSIAN
        0x1D => UiLang::Sv,   // LANG_SWEDISH
        0x24 => UiLang::Sl,   // LANG_SLOVENIAN
        0x1E => UiLang::Th,   // LANG_THAI
        0x2A => UiLang::Vi,   // LANG_VIETNAMESE
        0x04 => UiLang::ZhCn, // LANG_CHINESE
        _ => UiLang::En,
    }
}

fn current_lang() -> UiLang {
    let lang_id = unsafe { GetUserDefaultUILanguage() };
    lang_from_primary_id(lang_id & 0x03FF)
}

fn same_path_icase(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

fn short_display_name(path: &Path) -> String {
    let raw = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() > 42 {
        let mut clipped: String = chars[..39].iter().collect();
        clipped.push_str("...");
        clipped
    } else {
        raw
    }
}

fn resolve_action(
    selected: &[PathBuf],
    saved_left: Option<&Path>,
    is_dir_fn: impl Fn(&Path) -> bool,
) -> Option<MenuAction> {
    match selected {
        [a, b] => {
            if is_dir_fn(a) == is_dir_fn(b) {
                Some(MenuAction::ComparePair(a.clone(), b.clone()))
            } else {
                None
            }
        }
        [one] => {
            let one_dir = is_dir_fn(one);
            if let Some(left) = saved_left {
                if is_dir_fn(left) == one_dir {
                    if same_path_icase(left, one) {
                        return Some(MenuAction::ClearSavedLeft);
                    }
                    return Some(MenuAction::CompareWithSaved {
                        left: left.to_path_buf(),
                        right: one.clone(),
                    });
                }
            }
            Some(MenuAction::SelectLeft {
                path: one.clone(),
                is_dir: one_dir,
            })
        }
        _ => None,
    }
}

impl MenuAction {
    fn title(&self, lang: UiLang) -> String {
        match (self, lang) {
            (Self::ComparePair(_, _), UiLang::PtBr) => "Comparar".into(),
            (Self::ComparePair(_, _), UiLang::En) => "Compare".into(),
            (Self::ComparePair(_, _), UiLang::De) => "Vergleichen".into(),
            (Self::ComparePair(_, _), UiLang::Es) => "Comparar".into(),
            (Self::ComparePair(_, _), UiLang::Fr) => "Comparer".into(),
            (Self::ComparePair(_, _), UiLang::Ga) => "Cuir i gComparáid".into(),
            (Self::ComparePair(_, _), UiLang::It) => "Confronta".into(),
            (Self::ComparePair(_, _), UiLang::Ja) => "比較".into(),
            (Self::ComparePair(_, _), UiLang::Ko) => "비교".into(),
            (Self::ComparePair(_, _), UiLang::Ru) => "Сравнить".into(),
            (Self::ComparePair(_, _), UiLang::Sv) => "Jämför".into(),
            (Self::ComparePair(_, _), UiLang::Sl) => "Primerjaj".into(),
            (Self::ComparePair(_, _), UiLang::Th) => "เปรียบเทียบ".into(),
            (Self::ComparePair(_, _), UiLang::Vi) => "So sánh".into(),
            (Self::ComparePair(_, _), UiLang::ZhCn) => "比较".into(),

            (Self::CompareWithSaved { left, .. }, UiLang::PtBr) => {
                format!("Comparar com {}", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::En) => {
                format!("Compare to {}", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::De) => {
                format!("Vergleichen mit {}", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::Es) => {
                format!("Comparar con {}", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::Fr) => {
                format!("Comparer avec {}", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::Ga) => {
                format!("Cuir i gComparáid le {}", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::It) => {
                format!("Confronta con {}", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::Ja) => {
                format!("{} と比較", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::Ko) => {
                format!("{}와(과) 비교", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::Ru) => {
                format!("Сравнить с {}", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::Sv) => {
                format!("Jämför med {}", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::Sl) => {
                format!("Primerjaj z {}", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::Th) => {
                format!("เปรียบเทียบกับ {}", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::Vi) => {
                format!("So sánh với {}", short_display_name(left))
            }
            (Self::CompareWithSaved { left, .. }, UiLang::ZhCn) => {
                format!("与 {} 比较", short_display_name(left))
            }

            (Self::ClearSavedLeft, UiLang::PtBr) => "Limpar seleção esquerda (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::En) => "Clear Left Selection (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::De) => "Linke Auswahl löschen (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::Es) => "Borrar selección izquierda (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::Fr) => "Effacer la sélection gauche (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::Ga) => "Glan Roghnúchán Clé (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::It) => "Cancella selezione sinistra (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::Ja) => "左側の選択をクリア (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::Ko) => "왼쪽 선택 지우기 (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::Ru) => "Очистить выбор слева (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::Sv) => "Rensa vänster val (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::Sl) => "Počisti levi izbor (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::Th) => "ล้างรายการที่เลือกด้านซ้าย (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::Vi) => "Xóa lựa chọn bên trái (AnyCompare)".into(),
            (Self::ClearSavedLeft, UiLang::ZhCn) => "清除左侧选择 (AnyCompare)".into(),

            (Self::SelectLeft { is_dir: true, .. }, UiLang::PtBr) => {
                "Selecionar pasta esquerda para comparar".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::En) => {
                "Select Left Folder for Compare".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::De) => {
                "Linken Ordner zum Vergleichen auswählen".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::Es) => {
                "Seleccionar carpeta izquierda para comparar".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::Fr) => {
                "Sélectionner le dossier de gauche pour comparer".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::Ga) => {
                "Roghnaigh Fillteán Clé le Comparáid".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::It) => {
                "Seleziona cartella sinistra per il confronto".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::Ja) => {
                "比較する左側のフォルダーを選択".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::Ko) => {
                "비교할 왼쪽 폴더 선택".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::Ru) => {
                "Выбрать левую папку для сравнения".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::Sv) => {
                "Välj vänster mapp för jämförelse".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::Sl) => {
                "Izberi levo mapo za primerjavo".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::Th) => {
                "เลือกโฟลเดอร์ซ้ายเพื่อเปรียบเทียบ".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::Vi) => {
                "Chọn thư mục bên trái để so sánh".into()
            }
            (Self::SelectLeft { is_dir: true, .. }, UiLang::ZhCn) => {
                "选择左侧文件夹进行比较".into()
            }

            (Self::SelectLeft { is_dir: false, .. }, UiLang::PtBr) => {
                "Selecionar arquivo esquerdo para comparar".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::En) => {
                "Select Left File for Compare".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::De) => {
                "Linke Datei zum Vergleichen auswählen".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::Es) => {
                "Seleccionar archivo izquierdo para comparar".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::Fr) => {
                "Sélectionner le fichier de gauche pour comparer".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::Ga) => {
                "Roghnaigh Comhad Clé le Comparáid".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::It) => {
                "Seleziona file sinistro per il confronto".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::Ja) => {
                "比較する左側のファイルを選択".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::Ko) => {
                "비교할 왼쪽 파일 선택".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::Ru) => {
                "Выбрать левый файл для сравнения".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::Sv) => {
                "Välj vänster fil för jämförelse".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::Sl) => {
                "Izberi levo datoteko za primerjavo".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::Th) => {
                "เลือกไฟล์ซ้ายเพื่อเปรียบเทียบ".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::Vi) => {
                "Chọn tệp bên trái để so sánh".into()
            }
            (Self::SelectLeft { is_dir: false, .. }, UiLang::ZhCn) => {
                "选择左侧文件进行比较".into()
            }
        }
    }

    fn tooltip(&self, lang: UiLang) -> String {
        match (self, lang) {
            (Self::ComparePair(_, _), UiLang::PtBr) => {
                "Comparar os dois itens selecionados no AnyCompare".into()
            }
            (Self::ComparePair(_, _), _) => {
                "Compare the two selected items in AnyCompare".into()
            }
            (Self::CompareWithSaved { left, .. }, UiLang::PtBr) => {
                format!("Comparar {} com este item no AnyCompare", left.display())
            }
            (Self::CompareWithSaved { left, .. }, _) => {
                format!("Compare {} with this item in AnyCompare", left.display())
            }
            (Self::ClearSavedLeft, UiLang::PtBr) => {
                "Desmarcar o item esquerdo salvo para comparação".into()
            }
            (Self::ClearSavedLeft, _) => {
                "Clear the remembered left item for comparison".into()
            }
            (Self::SelectLeft { .. }, UiLang::PtBr) => {
                "Guardar este item como lado esquerdo para a próxima comparação".into()
            }
            (Self::SelectLeft { .. }, _) => {
                "Remember this item as the left side for the next comparison".into()
            }
        }
    }
}

fn state_file_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|p| p.join("AnyCompare").join("shellex_left.txt"))
}

fn load_saved_left() -> Option<PathBuf> {
    let path = state_file_path()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let saved = PathBuf::from(trimmed);
    saved.exists().then_some(saved)
}

fn save_left(path: &Path) {
    if let Some(file) = state_file_path() {
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(file, path.to_string_lossy().as_bytes());
    }
}

fn clear_saved_left() {
    if let Some(file) = state_file_path() {
        let _ = std::fs::remove_file(file);
    }
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn alloc_com_string(s: &str) -> Result<PWSTR> {
    let wide = to_wide_null(s);
    unsafe { SHStrDupW(PCWSTR(wide.as_ptr())) }
}

fn read_registry_string(root: HKEY, subkey: &str, value_name: &str) -> Option<String> {
    let subkey_w = to_wide_null(subkey);
    let value_w = to_wide_null(value_name);
    let mut bytes: u32 = 0;
    let status = unsafe {
        RegGetValueW(
            root,
            PCWSTR(subkey_w.as_ptr()),
            PCWSTR(value_w.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut bytes),
        )
    };
    if status.0 != 0 || bytes < 2 {
        return None;
    }
    let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
    let status = unsafe {
        RegGetValueW(
            root,
            PCWSTR(subkey_w.as_ptr()),
            PCWSTR(value_w.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut bytes),
        )
    };
    if status.0 != 0 {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let text = String::from_utf16_lossy(&buf[..end]);
    (!text.is_empty()).then_some(text)
}

fn current_dll_path() -> Option<PathBuf> {
    let raw = MODULE_HANDLE.load(Ordering::Acquire);
    if raw == 0 {
        return None;
    }
    let mut buf = vec![0u16; (MAX_PATH as usize) * 4];
    let len = unsafe { GetModuleFileNameW(HMODULE(raw as *mut c_void), &mut buf) } as usize;
    if len == 0 || len >= buf.len() {
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..len])))
}

/// Resolves the installed `anycompare.exe` path:
/// 1. `HKCU\Software\AnyCompare\ExePath`
/// 2. `HKLM\Software\AnyCompare\ExePath`
/// 3. `anycompare.exe` next to this DLL.
fn find_anycompare_exe() -> Option<PathBuf> {
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        if let Some(val) = read_registry_string(root, "Software\\AnyCompare", "ExePath") {
            let p = PathBuf::from(val);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    if let Some(dll) = current_dll_path() {
        if let Some(dir) = dll.parent() {
            let candidate = dir.join("anycompare.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn quote_win_arg(path: &Path) -> String {
    let raw = path.to_string_lossy();
    // Preserve trailing backslashes safely for CommandLineToArgvW.
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('"');
    let mut backslashes = 0usize;
    for ch in raw.chars() {
        if ch == '\\' {
            backslashes += 1;
        } else if ch == '"' {
            for _ in 0..(backslashes * 2 + 1) {
                out.push('\\');
            }
            out.push('"');
            backslashes = 0;
        } else {
            for _ in 0..backslashes {
                out.push('\\');
            }
            backslashes = 0;
            out.push(ch);
        }
    }
    for _ in 0..(backslashes * 2) {
        out.push('\\');
    }
    out.push('"');
    out
}

fn launch_compare(left: &Path, right: &Path) -> Result<()> {
    let exe = find_anycompare_exe().ok_or_else(|| Error::from(E_FAIL))?;
    let exe_w = to_wide_null(&exe.to_string_lossy());
    let params = format!("{} {}", quote_win_arg(left), quote_win_arg(right));
    let params_w = to_wide_null(&params);
    let ret = unsafe {
        ShellExecuteW(
            HWND::default(),
            w!("open"),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR(params_w.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if (ret.0 as isize) > 32 {
        Ok(())
    } else {
        Err(Error::from(E_FAIL))
    }
}

fn extract_paths(items: Option<&IShellItemArray>) -> Vec<PathBuf> {
    let Some(array) = items else {
        return Vec::new();
    };
    let count = unsafe { array.GetCount() }.unwrap_or(0);
    // Avoid scanning huge multi-selections; AnyCompare acts on 1 or 2 items only.
    if count == 0 || count > 2 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(count as usize);
    for idx in 0..count {
        let Ok(item) = (unsafe { array.GetItemAt(idx) }) else {
            return Vec::new();
        };
        let Ok(pwstr) = (unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }) else {
            return Vec::new();
        };
        if !pwstr.is_null() {
            let text = unsafe { pwstr.to_string() }.unwrap_or_default();
            unsafe { CoTaskMemFree(Some(pwstr.0 as *const c_void)) };
            if !text.is_empty() {
                out.push(PathBuf::from(text));
            }
        }
    }
    out
}

fn current_action(items: Option<&IShellItemArray>) -> Option<MenuAction> {
    let paths = extract_paths(items);
    let saved = load_saved_left();
    resolve_action(&paths, saved.as_deref(), |p| p.is_dir())
}

#[implement(IExplorerCommand)]
struct AnyCompareCommand;

impl AnyCompareCommand {
    fn new() -> Self {
        LIVE_OBJECTS.fetch_add(1, Ordering::AcqRel);
        Self
    }
}

impl Drop for AnyCompareCommand {
    fn drop(&mut self) {
        LIVE_OBJECTS.fetch_sub(1, Ordering::AcqRel);
    }
}

impl IExplorerCommand_Impl for AnyCompareCommand_Impl {
    fn GetTitle(&self, psiitemarray: Option<&IShellItemArray>) -> Result<PWSTR> {
        let lang = current_lang();
        let title = match current_action(psiitemarray) {
            Some(action) => action.title(lang),
            None => match lang {
                UiLang::PtBr => "Comparar com AnyCompare".into(),
                UiLang::En => "Compare with AnyCompare".into(),
                UiLang::De => "Mit AnyCompare vergleichen".into(),
                UiLang::Es => "Comparar con AnyCompare".into(),
                UiLang::Fr => "Comparer avec AnyCompare".into(),
                UiLang::Ga => "Cuir i gComparáid le AnyCompare".into(),
                UiLang::It => "Confronta con AnyCompare".into(),
                UiLang::Ja => "AnyCompare で比較".into(),
                UiLang::Ko => "AnyCompare로 비교".into(),
                UiLang::Ru => "Сравнить с помощью AnyCompare".into(),
                UiLang::Sv => "Jämför med AnyCompare".into(),
                UiLang::Sl => "Primerjaj z AnyCompare".into(),
                UiLang::Th => "เปรียบเทียบด้วย AnyCompare".into(),
                UiLang::Vi => "So sánh bằng AnyCompare".into(),
                UiLang::ZhCn => "使用 AnyCompare 比较".into(),
            },
        };
        alloc_com_string(&title)
    }

    fn GetIcon(&self, _psiitemarray: Option<&IShellItemArray>) -> Result<PWSTR> {
        let icon_spec = find_anycompare_exe()
            .map(|p| format!("{},0", p.display()))
            .unwrap_or_else(|| "shell32.dll,1".into());
        alloc_com_string(&icon_spec)
    }

    fn GetToolTip(&self, psiitemarray: Option<&IShellItemArray>) -> Result<PWSTR> {
        let lang = current_lang();
        let tip = match current_action(psiitemarray) {
            Some(action) => action.tooltip(lang),
            None => "AnyCompare".into(),
        };
        alloc_com_string(&tip)
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(CANONICAL_COMMAND_ID)
    }

    fn GetState(&self, psiitemarray: Option<&IShellItemArray>, _foktobeslow: BOOL) -> Result<u32> {
        if current_action(psiitemarray).is_some() {
            Ok(ECS_ENABLED.0 as u32)
        } else {
            Ok(ECS_HIDDEN.0 as u32)
        }
    }

    fn Invoke(
        &self,
        psiitemarray: Option<&IShellItemArray>,
        _pbc: Option<&IBindCtx>,
    ) -> Result<()> {
        let Some(action) = current_action(psiitemarray) else {
            return Ok(());
        };
        match action {
            MenuAction::ComparePair(left, right) => launch_compare(&left, &right),
            MenuAction::CompareWithSaved { left, right } => {
                clear_saved_left();
                launch_compare(&left, &right)
            }
            MenuAction::ClearSavedLeft => {
                clear_saved_left();
                Ok(())
            }
            MenuAction::SelectLeft { path, .. } => {
                save_left(&path);
                Ok(())
            }
        }
    }

    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_DEFAULT.0 as u32)
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Err(Error::from(E_NOTIMPL))
    }
}

#[implement(IClassFactory)]
struct AnyCompareClassFactory;

impl AnyCompareClassFactory {
    fn new() -> Self {
        LIVE_OBJECTS.fetch_add(1, Ordering::AcqRel);
        Self
    }
}

impl Drop for AnyCompareClassFactory {
    fn drop(&mut self) {
        LIVE_OBJECTS.fetch_sub(1, Ordering::AcqRel);
    }
}

impl IClassFactory_Impl for AnyCompareClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Option<&IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        if ppvobject.is_null() {
            return Err(Error::from(E_POINTER));
        }
        unsafe { *ppvobject = std::ptr::null_mut() };
        if punkouter.is_some() {
            return Err(Error::from(CLASS_E_NOAGGREGATION));
        }
        if riid.is_null() {
            return Err(Error::from(E_INVALIDARG));
        }
        let cmd: IExplorerCommand = AnyCompareCommand::new().into();
        let hr = unsafe { cmd.query(riid, ppvobject) };
        if hr.is_ok() {
            Ok(())
        } else {
            Err(Error::from(hr))
        }
    }

    fn LockServer(&self, flock: BOOL) -> Result<()> {
        if flock.as_bool() {
            SERVER_LOCKS.fetch_add(1, Ordering::AcqRel);
        } else {
            SERVER_LOCKS.fetch_sub(1, Ordering::AcqRel);
        }
        Ok(())
    }
}

fn reg_set_sz(root: HKEY, subkey: &str, name: Option<&str>, value: &str) -> bool {
    let subkey_w = to_wide_null(subkey);
    let mut hkey = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            root,
            PCWSTR(subkey_w.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )
    };
    if status.0 != 0 {
        return false;
    }
    let value_w = to_wide_null(value);
    let bytes =
        unsafe { std::slice::from_raw_parts(value_w.as_ptr() as *const u8, value_w.len() * 2) };
    let name_w = name.map(to_wide_null);
    let name_ptr = name_w
        .as_ref()
        .map_or(PCWSTR::null(), |w| PCWSTR(w.as_ptr()));
    let write_ok = unsafe { RegSetValueExW(hkey, name_ptr, 0, REG_SZ, Some(bytes)) }.0 == 0;
    let _ = unsafe { RegCloseKey(hkey) };
    write_ok
}

fn reg_delete_tree(root: HKEY, subkey: &str) {
    let subkey_w = to_wide_null(subkey);
    let _ = unsafe { RegDeleteTreeW(root, PCWSTR(subkey_w.as_ptr())) };
}

/// Standard Win32 DLL entry point.
///
/// # Safety
/// Invoked exclusively by the Windows loader with valid process/module handles.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllMain(
    hinst: HINSTANCE,
    reason: u32,
    _reserved: *mut c_void,
) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        MODULE_HANDLE.store(hinst.0 as isize, Ordering::Release);
    }
    BOOL(1)
}

/// Standard COM class factory entry point.
///
/// # Safety
/// `rclsid`, `riid`, and `ppv` must be valid pointers per COM `DllGetClassObject` contract.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if ppv.is_null() {
        return E_POINTER;
    }
    *ppv = std::ptr::null_mut();
    if rclsid.is_null() || riid.is_null() {
        return E_INVALIDARG;
    }
    if *rclsid != CLSID_ANYCOMPARE_COMMAND {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = AnyCompareClassFactory::new().into();
    factory.query(riid, ppv)
}

/// Reports whether the COM runtime may unload this DLL.
///
/// # Safety
/// Called by the COM runtime on any thread.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllCanUnloadNow() -> HRESULT {
    if LIVE_OBJECTS.load(Ordering::Acquire) == 0 && SERVER_LOCKS.load(Ordering::Acquire) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

/// Registers the COM server and classic Windows 10 / 11 Explorer `ExplorerCommandHandler`
/// entries under `HKCU\Software\Classes` (per-user, no elevation required).
///
/// # Safety
/// Invoked by `regsvr32.exe` or the AnyCompare installer after `DllMain`.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllRegisterServer() -> HRESULT {
    let Some(dll) = current_dll_path() else {
        return E_FAIL;
    };
    let dll_str = dll.to_string_lossy();
    let clsid_key = format!("Software\\Classes\\CLSID\\{CLSID_STR}");
    let inproc_key = format!("{clsid_key}\\InprocServer32");

    let mut ok = true;
    ok &= reg_set_sz(
        HKEY_CURRENT_USER,
        &clsid_key,
        None,
        "AnyCompare Shell Extension",
    );
    ok &= reg_set_sz(HKEY_CURRENT_USER, &inproc_key, None, &dll_str);
    ok &= reg_set_sz(
        HKEY_CURRENT_USER,
        &inproc_key,
        Some("ThreadingModel"),
        "Apartment",
    );

    for verb_key in [
        "Software\\Classes\\*\\shell\\AnyCompare",
        "Software\\Classes\\Directory\\shell\\AnyCompare",
        "Software\\Classes\\Directory\\Background\\shell\\AnyCompare",
        "Software\\Classes\\Drive\\shell\\AnyCompare",
    ] {
        ok &= reg_set_sz(
            HKEY_CURRENT_USER,
            verb_key,
            Some("ExplorerCommandHandler"),
            CLSID_STR,
        );
    }

    if ok {
        S_OK
    } else {
        E_FAIL
    }
}

/// Removes the per-user COM registration and classic Explorer context menu verbs.
///
/// # Safety
/// Invoked by `regsvr32.exe /u` or the AnyCompare uninstaller after `DllMain`.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllUnregisterServer() -> HRESULT {
    let clsid_key = format!("Software\\Classes\\CLSID\\{CLSID_STR}");
    reg_delete_tree(HKEY_CURRENT_USER, &clsid_key);
    for verb_key in [
        "Software\\Classes\\*\\shell\\AnyCompare",
        "Software\\Classes\\Directory\\shell\\AnyCompare",
        "Software\\Classes\\Directory\\Background\\shell\\AnyCompare",
        "Software\\Classes\\Drive\\shell\\AnyCompare",
    ] {
        reg_delete_tree(HKEY_CURRENT_USER, verb_key);
    }
    clear_saved_left();
    S_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_is_dir(p: &Path) -> bool {
        p.extension().is_none()
    }

    #[test]
    fn resolves_two_item_and_two_step_workflows_and_rejects_mixed_types() {
        let f1 = PathBuf::from(r"C:\tmp\a.txt");
        let f2 = PathBuf::from(r"C:\tmp\b.txt");
        let d1 = PathBuf::from(r"C:\tmp\dir_a");
        let d2 = PathBuf::from(r"C:\tmp\dir_b");

        // Two files -> instant ComparePair
        let act = resolve_action(&[f1.clone(), f2.clone()], None, fake_is_dir).unwrap();
        assert_eq!(act, MenuAction::ComparePair(f1.clone(), f2.clone()));
        assert_eq!(act.title(UiLang::PtBr), "Comparar");
        assert_eq!(act.title(UiLang::En), "Compare");

        // Two folders -> instant ComparePair
        let act = resolve_action(&[d1.clone(), d2.clone()], None, fake_is_dir).unwrap();
        assert_eq!(act, MenuAction::ComparePair(d1.clone(), d2.clone()));

        // Mixed file + folder -> hidden (None)
        assert_eq!(
            resolve_action(&[f1.clone(), d1.clone()], None, fake_is_dir),
            None
        );

        // One file with no saved left -> SelectLeft
        let act = resolve_action(std::slice::from_ref(&f1), None, fake_is_dir).unwrap();
        assert_eq!(
            act,
            MenuAction::SelectLeft {
                path: f1.clone(),
                is_dir: false
            }
        );
        assert_eq!(
            act.title(UiLang::PtBr),
            "Selecionar arquivo esquerdo para comparar"
        );

        // Second file after first is saved -> CompareWithSaved
        let act =
            resolve_action(std::slice::from_ref(&f2), Some(f1.as_path()), fake_is_dir).unwrap();
        assert_eq!(
            act,
            MenuAction::CompareWithSaved {
                left: f1.clone(),
                right: f2.clone()
            }
        );
        assert_eq!(act.title(UiLang::PtBr), "Comparar com a.txt");

        // Same file re-clicked -> ClearSavedLeft
        let act =
            resolve_action(std::slice::from_ref(&f1), Some(f1.as_path()), fake_is_dir).unwrap();
        assert_eq!(act, MenuAction::ClearSavedLeft);

        // Folder clicked while a file was saved -> resets to SelectLeft for folder
        let act =
            resolve_action(std::slice::from_ref(&d1), Some(f1.as_path()), fake_is_dir).unwrap();
        assert_eq!(
            act,
            MenuAction::SelectLeft {
                path: d1.clone(),
                is_dir: true
            }
        );
        assert_eq!(
            act.title(UiLang::PtBr),
            "Selecionar pasta esquerda para comparar"
        );
    }

    #[test]
    fn quotes_windows_paths_with_spaces_and_trailing_backslashes() {
        assert_eq!(
            quote_win_arg(Path::new(r"C:\Meus Arquivos\foo.txt")),
            r#""C:\Meus Arquivos\foo.txt""#
        );
        assert_eq!(quote_win_arg(Path::new(r"C:\")), r#""C:\\""#);
    }
}
