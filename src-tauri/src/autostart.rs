//! "Start with Windows" via HKCU\Software\Microsoft\Windows\CurrentVersion\Run.

use std::path::Path;
use windows::{
    core::{w, PCWSTR},
    Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
    },
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: PCWSTR = w!("TaskbarSeparators");

pub fn is_enabled() -> bool {
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
        .is_ok()
    }
}

pub fn set(enabled: bool, exe: &Path) -> windows::core::Result<()> {
    unsafe {
        if enabled {
            let cmd: Vec<u16> = format!("\"{}\" --minimized", exe.display())
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE,
                REG_SZ.0,
                Some(cmd.as_ptr() as *const _),
                (cmd.len() * 2) as u32,
            )
            .ok()
        } else {
            let r = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE);
            // Already absent is fine.
            if r.is_err() && r != windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
                return r.ok();
            }
            Ok(())
        }
    }
}
