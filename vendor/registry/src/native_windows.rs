//! Captura nativa de Registro do Windows. Exclusivamente leitura.
//! A escolha das APIs é fundamentada nos imports do BC5 e nas APIs Win32;
//! a semântica completa do BC5 continua dependente de comparação A/B.
#![cfg(windows)]
use super::native_buffers::{next_value_buffers, MAX_NAME_UNITS, MAX_VALUE_BYTES};
use super::{canonical, ensure_parent_keys, RegValue, Registry, MAX_REG_BYTES};
use std::{collections::VecDeque, ptr};

type Hkey = isize;
const ERROR_SUCCESS: i32 = 0;
const ERROR_MORE_DATA: i32 = 234;
const ERROR_NO_MORE_ITEMS: i32 = 259;
const KEY_READ: u32 = 0x0002_0019;
const KEY_WOW64_32KEY: u32 = 0x0200;
const KEY_WOW64_64KEY: u32 = 0x0100;
const MAX_KEYS: usize = 200_000;
const MAX_DEPTH: usize = 64;
const MAX_PENDING_HANDLES: usize = 4096;

#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(root: Hkey, subkey: *const u16, options: u32, sam: u32, out: *mut Hkey)
        -> i32;
    fn RegCloseKey(key: Hkey) -> i32;
    fn RegConnectRegistryW(machine: *const u16, root: Hkey, out: *mut Hkey) -> i32;
    fn RegEnumKeyExW(
        key: Hkey,
        index: u32,
        name: *mut u16,
        name_len: *mut u32,
        reserved: *mut u32,
        class_name: *mut u16,
        class_len: *mut u32,
        last_write: *mut std::ffi::c_void,
    ) -> i32;
    fn RegEnumValueW(
        key: Hkey,
        index: u32,
        name: *mut u16,
        name_len: *mut u32,
        reserved: *mut u32,
        kind: *mut u32,
        data: *mut u8,
        data_len: *mut u32,
    ) -> i32;
}

struct OwnedKey(Hkey);
impl Drop for OwnedKey {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
fn error(action: &str, status: i32) -> String {
    format!("{action}: erro Win32 {status}")
}
fn root_and_subkey(full: &str) -> Result<(Hkey, &str), String> {
    if full.is_empty()
        || full.contains('\0')
        || full.contains('/')
        || full.split('\\').any(str::is_empty)
    {
        return Err("caminho de Registro inválido".into());
    }
    let (head, tail) = full.split_once('\\').map_or((full, ""), |(a, b)| (a, b));
    let root = if head.eq_ignore_ascii_case("HKEY_CLASSES_ROOT") {
        0x80000000u32
    } else if head.eq_ignore_ascii_case("HKEY_CURRENT_USER") {
        0x80000001u32
    } else if head.eq_ignore_ascii_case("HKEY_LOCAL_MACHINE") {
        0x80000002u32
    } else if head.eq_ignore_ascii_case("HKEY_USERS") {
        0x80000003u32
    } else if head.eq_ignore_ascii_case("HKEY_CURRENT_CONFIG") {
        0x80000005u32
    } else {
        return Err("chave raiz HKEY_... não reconhecida".into());
    };
    // HKEY predefined é um handle assinado, com extensão de sinal em x64.
    Ok((root as i32 as isize, tail))
}
fn enum_values(
    handle: Hkey,
    key_path: &str,
    reg: &mut Registry,
    total_bytes: &mut usize,
) -> Result<(), String> {
    let mut index = 0u32;
    loop {
        let mut name_cap = 256usize;
        let mut data_cap = 1024usize;
        let (name, kind, bytes) = loop {
            if name_cap > MAX_NAME_UNITS || data_cap > MAX_VALUE_BYTES {
                return Err(format!(
                    "metadado de Registro ultrapassa limite em {key_path}"
                ));
            }
            let mut name_buffer = vec![0u16; name_cap];
            let mut data_buffer = vec![0u8; data_cap];
            let mut name_len = name_buffer.len() as u32;
            let mut data_len = data_buffer.len() as u32;
            let mut kind = 0u32;
            let status = unsafe {
                RegEnumValueW(
                    handle,
                    index,
                    name_buffer.as_mut_ptr(),
                    &mut name_len,
                    ptr::null_mut(),
                    &mut kind,
                    data_buffer.as_mut_ptr(),
                    &mut data_len,
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                return Ok(());
            }
            if status == ERROR_MORE_DATA {
                // O tamanho necessário dos dados (bytes) e o do nome (UTF-16)
                // são independentes. Dobrar ambos rejeita valores binários grandes
                // quando o nome chega ao limite sem necessidade.
                (name_cap, data_cap) = next_value_buffers(name_cap, data_cap, data_len)?;
                continue;
            }
            if status != ERROR_SUCCESS {
                return Err(error("RegEnumValueW", status));
            }
            if name_len as usize > name_buffer.len() || data_len as usize > data_buffer.len() {
                return Err("RegEnumValueW devolveu comprimentos incompatíveis".into());
            }
            let name = String::from_utf16(&name_buffer[..name_len as usize])
                .map_err(|_| "nome de valor UTF-16 inválido".to_string())?;
            data_buffer.truncate(data_len as usize);
            break (name, kind, data_buffer);
        };
        *total_bytes = total_bytes
            .checked_add(bytes.len())
            .ok_or("soma de dados excedida")?;
        if *total_bytes > MAX_REG_BYTES {
            return Err("snapshot excede 64 MiB de dados".into());
        }
        let key = reg
            .keys
            .get_mut(&canonical(key_path))
            .ok_or("chave não encontrada")?;
        let id = canonical(&name);
        if key
            .values
            .insert(
                id,
                RegValue {
                    name,
                    type_id: kind,
                    bytes,
                },
            )
            .is_some()
        {
            return Err("RegEnumValueW retornou nome duplicado".into());
        }
        index = index.checked_add(1).ok_or("muitos valores")?;
    }
}
fn enum_children(handle: Hkey) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut index = 0u32;
    loop {
        let mut cap = 256usize;
        let name = loop {
            if cap > MAX_NAME_UNITS {
                return Err("nome de chave muito grande".into());
            }
            let mut buf = vec![0u16; cap];
            let mut len = cap as u32;
            let status = unsafe {
                RegEnumKeyExW(
                    handle,
                    index,
                    buf.as_mut_ptr(),
                    &mut len,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                return Ok(out);
            }
            if status == ERROR_MORE_DATA {
                cap = cap.saturating_mul(2);
                continue;
            }
            if status != ERROR_SUCCESS {
                return Err(error("RegEnumKeyExW", status));
            }
            if len as usize > buf.len() {
                return Err("RegEnumKeyExW retornou tamanho inválido".into());
            }
            break String::from_utf16(&buf[..len as usize])
                .map_err(|_| "nome de chave UTF-16 inválido".to_string())?;
        };
        if name.is_empty() || name.contains('\\') || name.contains('\0') {
            return Err("subchave inválida retornada pela API".into());
        }
        out.push(name);
        if out.len() > MAX_KEYS {
            return Err("excesso de subchaves".into());
        }
        index = index.checked_add(1).ok_or("muitas subchaves")?;
    }
}

pub(super) fn read_native(
    host: Option<&str>,
    full_path: &str,
    view: super::RegistryView,
) -> Result<Registry, String> {
    let (root, subkey) = root_and_subkey(full_path)?;
    let connected = if let Some(machine) = host {
        // RegConnectRegistryW não fornece HKCU/HKCR remotos.
        let head = full_path.split('\\').next().unwrap_or("");
        if !head.eq_ignore_ascii_case("HKEY_LOCAL_MACHINE")
            && !head.eq_ignore_ascii_case("HKEY_USERS")
        {
            return Err("registro remoto exige HKEY_LOCAL_MACHINE ou HKEY_USERS".into());
        }
        let clean = machine.trim_matches('\\');
        if clean.is_empty()
            || clean
                .chars()
                .any(|c| c == '\\' || c == '/' || c == '\0' || c.is_control())
        {
            return Err("nome de computador remoto inválido".into());
        }
        let remote = wide(&format!("\\\\{clean}"));
        let mut handle = 0;
        let code = unsafe { RegConnectRegistryW(remote.as_ptr(), root, &mut handle) };
        if code != ERROR_SUCCESS {
            return Err(error("RegConnectRegistryW", code));
        }
        Some(OwnedKey(handle))
    } else {
        None
    };
    let root_handle = connected.as_ref().map_or(root, |h| h.0);
    let mut opened = 0;
    let sam = KEY_READ
        | match view {
            super::RegistryView::Native => 0,
            super::RegistryView::View32 => KEY_WOW64_32KEY,
            super::RegistryView::View64 => KEY_WOW64_64KEY,
        };
    let key_name = if subkey.is_empty() {
        None
    } else {
        Some(wide(subkey))
    };
    let code = unsafe {
        RegOpenKeyExW(
            root_handle,
            key_name.as_ref().map_or(ptr::null(), |v| v.as_ptr()),
            0,
            sam,
            &mut opened,
        )
    };
    if code != ERROR_SUCCESS {
        return Err(error("RegOpenKeyExW", code));
    }
    let mut queue = VecDeque::new();
    queue.push_back((OwnedKey(opened), full_path.to_string(), 0usize));
    let mut reg = Registry::default();
    let mut total_bytes = 0usize;
    while let Some((handle, path, depth)) = queue.pop_front() {
        if depth > MAX_DEPTH {
            return Err("profundidade de Registro excede limite".into());
        }
        ensure_parent_keys(&mut reg, &path);
        if reg.keys.len() > MAX_KEYS {
            return Err("snapshot excede limite de chaves".into());
        }
        enum_values(handle.0, &path, &mut reg, &mut total_bytes)?;
        let children = enum_children(handle.0)?;
        for child in children {
            if depth >= MAX_DEPTH {
                return Err("profundidade de Registro excede limite".into());
            }
            let mut child_handle = 0;
            let wide_child = wide(&child);
            let code =
                unsafe { RegOpenKeyExW(handle.0, wide_child.as_ptr(), 0, sam, &mut child_handle) };
            if code != ERROR_SUCCESS {
                return Err(error("RegOpenKeyExW(subchave)", code));
            }
            queue.push_back((
                OwnedKey(child_handle),
                format!("{path}\\{child}"),
                depth + 1,
            ));
            if queue.len() > MAX_PENDING_HANDLES {
                return Err("muitas subchaves pendentes (limite de handles)".into());
            }
        }
    }
    Ok(reg)
}
