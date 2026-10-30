use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::path::{Path, PathBuf};

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
            return Err("Operação cancelada.".into());
        }
    }
    Ok(())
}

pub fn open(path: &Path, select: bool) -> Result<(), String> {
    let result = if select {
        std::process::Command::new("explorer.exe")
            .arg("/select,")
            .arg(path)
            .spawn()
    } else {
        std::process::Command::new("explorer.exe").arg(path).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}
