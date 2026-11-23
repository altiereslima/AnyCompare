#[cfg(windows)]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::path::{Path, PathBuf};

#[cfg(windows)]
pub fn recycle(parent: Option<&eframe::Frame>, paths: &[PathBuf]) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::{Foundation::HWND, System::Com::*, UI::Shell::*},
    };
    let owner = parent
        .and_then(|frame| frame.window_handle().ok())
        .and_then(|handle| {
            if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                Some(HWND(handle.hwnd.get() as *mut _))
            } else {
                None
            }
        });
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        struct Apartment(bool);
        impl Drop for Apartment {
            fn drop(&mut self) {
                if self.0 {
                    unsafe {
                        CoUninitialize();
                    }
                }
            }
        }
        let _apartment = Apartment(initialized.is_ok());
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| e.to_string())?;
        operation
            .SetOperationFlags(FOFX_RECYCLEONDELETE | FOFX_ADDUNDORECORD | FOFX_EARLYFAILURE)
            .map_err(|e| e.to_string())?;
        if let Some(owner) = owner {
            operation.SetOwnerWindow(owner).map_err(|e| e.to_string())?;
        }
        for path in paths {
            let mut wide: Vec<_> = path.as_os_str().encode_wide().collect();
            let prefix: Vec<_> = "\\\\?\\".encode_utf16().collect();
            if wide.starts_with(&prefix) {
                wide.drain(..4);
                if wide.starts_with(&"UNC\\".encode_utf16().collect::<Vec<_>>()) {
                    wide.drain(..4);
                    wide.splice(0..0, "\\\\".encode_utf16());
                }
            }
            wide.push(0);
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)
                .map_err(|e| e.to_string())?;
            operation
                .DeleteItem(&item, None)
                .map_err(|e| e.to_string())?;
        }
        operation.PerformOperations().map_err(|e| e.to_string())?;
        if operation
            .GetAnyOperationsAborted()
            .map_err(|e| e.to_string())?
            .as_bool()
        {
            return Err(crate::i18n::tr("common.operation_canceled"));
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn recycle(_parent: Option<&eframe::Frame>, paths: &[PathBuf]) -> Result<(), String> {
    for path in paths {
        let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let status = std::process::Command::new("osascript")
            .args([
                "-e",
                "on run argv\nset p to POSIX file (item 1 of argv)\ntell application \"Finder\" to delete p\nend run",
                "--",
            ])
            .arg(&abs)
            .status()
            .map_err(|e| format!("Trash unavailable ({e})"))?;
        if !status.success() {
            return Err(format!(
                "Could not move {} to Trash (osascript exit {})",
                path.display(),
                status
            ));
        }
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn recycle(_parent: Option<&eframe::Frame>, paths: &[PathBuf]) -> Result<(), String> {
    for path in paths {
        let gio = std::process::Command::new("gio")
            .arg("trash")
            .arg(path)
            .status();
        match gio {
            Ok(s) if s.success() => continue,
            _ => {
                let trash_put = std::process::Command::new("trash-put").arg(path).status();
                match trash_put {
                    Ok(s) if s.success() => continue,
                    _ => {
                        return Err(format!(
                            "System trash unavailable for {} (install gio or trash-cli)",
                            path.display()
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn open(path: &Path, select: bool) -> Result<(), String> {
    #[cfg(windows)]
    let result = if select {
        std::process::Command::new("explorer.exe")
            .arg("/select,")
            .arg(path)
            .spawn()
    } else {
        std::process::Command::new("explorer.exe").arg(path).spawn()
    };
    #[cfg(target_os = "macos")]
    let result = if select {
        std::process::Command::new("open").arg("-R").arg(path).spawn()
    } else {
        std::process::Command::new("open").arg(path).spawn()
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = {
        let target = if select {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        std::process::Command::new("xdg-open").arg(target).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}
