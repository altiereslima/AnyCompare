//! Small modal Windows input dialog. It uses ordinary system controls and an owned
//! message loop, so Enter, Escape, modality and the caption follow Windows dialogs.
use super::dialogs::DialogField;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::Input::KeyboardAndMouse::SetFocus,
    UI::WindowsAndMessaging::*,
};

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn word(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend(value.to_le_bytes());
}
fn dword(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend(value.to_le_bytes());
}
fn string(bytes: &mut Vec<u8>, value: &str) {
    for value in wide(value) {
        word(bytes, value);
    }
}

fn item(bytes: &mut Vec<u8>, style: u32, rect: [u16; 4], id: u16, class: u16, text: &str) {
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
    dword(bytes, style | WS_CHILD | WS_VISIBLE);
    dword(bytes, 0);
    for value in rect {
        word(bytes, value);
    }
    word(bytes, id);
    word(bytes, 0xffff);
    word(bytes, class);
    string(bytes, text);
    word(bytes, 0);
}

struct State {
    fields: Vec<DialogField>,
    accepted: bool,
    auto_answer: Option<bool>,
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    match message {
        WM_INITDIALOG => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam);
            let state = &*(lparam as *const State);
            for (index, field) in state.fields.iter().enumerate() {
                let id = 101 + index as i32;
                match field {
                    DialogField::Notice(_) => {}
                    DialogField::Text(_, value) => {
                        SetDlgItemTextW(hwnd, id, wide(value).as_ptr());
                    }
                    DialogField::Check(_, value) => {
                        SendDlgItemMessageW(hwnd, id, BM_SETCHECK, usize::from(*value), 0);
                    }
                    DialogField::Choice(_, selected, choices) => {
                        for choice in choices {
                            SendDlgItemMessageW(
                                hwnd,
                                id,
                                CB_ADDSTRING,
                                0,
                                wide(choice).as_ptr() as isize,
                            );
                        }
                        SendDlgItemMessageW(hwnd, id, CB_SETCURSEL, *selected, 0);
                    }
                }
            }
            if let Some(index) = state
                .fields
                .iter()
                .position(|field| !matches!(field, DialogField::Notice(_)))
            {
                SetFocus(GetDlgItem(hwnd, 101 + index as i32));
            }
            let parent = GetParent(hwnd);
            if !parent.is_null() {
                let mut parent_rect = std::mem::zeroed();
                let mut rect = std::mem::zeroed();
                if GetWindowRect(parent, &mut parent_rect) != 0
                    && GetWindowRect(hwnd, &mut rect) != 0
                {
                    SetWindowPos(
                        hwnd,
                        std::ptr::null_mut(),
                        parent_rect.left
                            + (parent_rect.right - parent_rect.left - (rect.right - rect.left)) / 2,
                        parent_rect.top
                            + (parent_rect.bottom - parent_rect.top - (rect.bottom - rect.top)) / 2,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER,
                    );
                }
            }
            if let Some(accept) = state.auto_answer {
                // Controlled native smoke test: only this owned dialog is addressed.
                // It is placed outside the desktop and receives a queued answer.
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    -30000,
                    -30000,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER,
                );
                PostMessageW(
                    hwnd,
                    WM_COMMAND,
                    if accept {
                        IDOK as usize
                    } else {
                        IDCANCEL as usize
                    },
                    0,
                );
            }
            0
        }
        WM_COMMAND if wparam as u16 == IDOK as u16 => {
            let state = &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State);
            for (index, field) in state.fields.iter_mut().enumerate() {
                let id = 101 + index as i32;
                match field {
                    DialogField::Notice(_) => {}
                    DialogField::Text(_, value) => {
                        let count = GetWindowTextLengthW(GetDlgItem(hwnd, id));
                        let mut text = vec![0; count as usize + 1];
                        let used = GetDlgItemTextW(hwnd, id, text.as_mut_ptr(), text.len() as i32);
                        *value = String::from_utf16_lossy(&text[..used as usize]);
                    }
                    DialogField::Check(_, value) => {
                        *value = SendDlgItemMessageW(hwnd, id, BM_GETCHECK, 0, 0) == 1
                    }
                    DialogField::Choice(_, selected, choices) => {
                        let value = SendDlgItemMessageW(hwnd, id, CB_GETCURSEL, 0, 0);
                        if value >= 0 && (value as usize) < choices.len() {
                            *selected = value as usize;
                        }
                    }
                }
            }
            state.accepted = true;
            EndDialog(hwnd, IDOK as isize);
            1
        }
        WM_CLOSE => {
            EndDialog(hwnd, IDCANCEL as isize);
            1
        }
        WM_COMMAND if wparam as u16 == IDCANCEL as u16 => {
            EndDialog(hwnd, IDCANCEL as isize);
            1
        }
        _ => 0,
    }
}

pub fn input(
    parent: Option<&eframe::Frame>,
    title: &str,
    label: &str,
    initial: &str,
) -> Option<String> {
    let mut fields = [DialogField::Text(label.into(), initial.into())];
    if form(parent, title, &mut fields) {
        if let DialogField::Text(_, value) = &fields[0] {
            return Some(value.clone());
        }
    }
    None
}

pub fn form(parent: Option<&eframe::Frame>, title: &str, fields: &mut [DialogField]) -> bool {
    form_inner(parent, title, fields, None)
}

pub fn form_with_accept(
    parent: Option<&eframe::Frame>,
    title: &str,
    fields: &mut [DialogField],
    accept: &str,
) -> bool {
    form_inner_accept(parent, title, fields, None, accept)
}

fn form_inner(
    parent: Option<&eframe::Frame>,
    title: &str,
    fields: &mut [DialogField],
    auto_answer: Option<bool>,
) -> bool {
    form_inner_accept(parent, title, fields, auto_answer, "OK")
}

fn form_inner_accept(
    parent: Option<&eframe::Frame>,
    title: &str,
    fields: &mut [DialogField],
    auto_answer: Option<bool>,
    accept: &str,
) -> bool {
    if fields.len() > 20 {
        return false;
    }
    let mut bytes = Vec::new();
    dword(
        &mut bytes,
        WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_MODALFRAME as u32 | DS_SETFONT as u32,
    );
    dword(&mut bytes, 0);
    let controls = 2 + fields
        .iter()
        .map(|field| {
            if matches!(field, DialogField::Check(..) | DialogField::Notice(..)) {
                1
            } else {
                2
            }
        })
        .sum::<u16>();
    let row_height = |field: &DialogField| {
        if matches!(field, DialogField::Notice(_)) {
            34
        } else {
            24
        }
    };
    let button_y = 12 + fields.iter().map(row_height).sum::<u16>();
    for value in [controls, 0, 0, 290, button_y + 27, 0, 0] {
        word(&mut bytes, value);
    }
    string(&mut bytes, title);
    word(&mut bytes, 9);
    string(&mut bytes, "Segoe UI");
    let mut y = 9;
    for (index, field) in fields.iter().enumerate() {
        let id = 101 + index as u16;
        match field {
            DialogField::Notice(text) => item(&mut bytes, 0, [8, y, 274, 30], id, 0x82, text),
            DialogField::Check(label, _) => item(
                &mut bytes,
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                [8, y, 274, 14],
                id,
                0x80,
                label,
            ),
            DialogField::Text(label, _) | DialogField::Choice(label, _, _) => {
                item(
                    &mut bytes,
                    0,
                    [8, y + 2, 126, 16],
                    501 + index as u16,
                    0x82,
                    label,
                );
                if matches!(field, DialogField::Text(..)) {
                    item(
                        &mut bytes,
                        WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32,
                        [136, y, 146, 14],
                        id,
                        0x81,
                        "",
                    );
                } else {
                    item(
                        &mut bytes,
                        WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
                        [136, y, 146, 100],
                        id,
                        0x85,
                        "",
                    );
                }
            }
        }
        y += row_height(field);
    }
    item(
        &mut bytes,
        WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
        [175, button_y, 50, 14],
        IDOK as u16,
        0x80,
        accept,
    );
    item(
        &mut bytes,
        WS_TABSTOP,
        [233, button_y, 50, 14],
        IDCANCEL as u16,
        0x80,
        &crate::i18n::tr("Cancelar"),
    );
    // Vec<u32> gives DLGTEMPLATE the required DWORD alignment.
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
    let aligned: Vec<u32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(*b))
        .collect();
    let owner = parent
        .and_then(|frame| frame.window_handle().ok())
        .and_then(|handle| {
            if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                Some(handle.hwnd.get() as HWND)
            } else {
                None
            }
        })
        .unwrap_or(std::ptr::null_mut());
    let mut state = State {
        fields: fields.to_vec(),
        accepted: false,
        auto_answer,
    };
    unsafe {
        DialogBoxIndirectParamW(
            GetModuleHandleW(std::ptr::null()),
            aligned.as_ptr().cast(),
            owner,
            Some(procedure),
            &mut state as *mut State as isize,
        );
    }
    if state.accepted {
        fields.clone_from_slice(&state.fields);
    }
    state.accepted
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_template_roundtrips_text_checks_choices_and_cancel() {
        let mut fields = [
            DialogField::Notice("Caracteres Unicode: ação 🙂".into()),
            DialogField::Text("Linha:".into(), "ação 🦀 12:3".into()),
            DialogField::Check("Habilitada".into(), true),
            DialogField::Check("Desabilitada".into(), false),
            DialogField::Choice(
                "Formato:".into(),
                1,
                vec!["UTF-8".into(), "UTF-16 LE".into()],
            ),
        ];
        let original = fields.clone();
        assert!(form_inner(
            None,
            "AnyCompare native test",
            &mut fields,
            Some(true)
        ));
        assert_eq!(fields, original);
        assert!(!form_inner(
            None,
            "AnyCompare native test",
            &mut fields,
            Some(false)
        ));
        assert_eq!(fields, original);
    }
}
