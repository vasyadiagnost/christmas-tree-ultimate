//! Startup coordination and a resumable, non-destructive migration from the first personal build.
//! These functions run only when the user starts the app or its installer.

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, GetLastError,
    HANDLE, LPARAM, WAIT_OBJECT_0, WIN32_ERROR, WPARAM,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_VALUE_TYPE, RRF_RT_REG_DWORD, RegCloseKey,
    RegCreateKeyExW, RegEnumValueW, RegGetValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::Win32::System::Threading::{CreateMutexW, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId, PostMessageW, WM_CLOSE};
use windows::core::{PCWSTR, PWSTR, w};

pub const SETTINGS: PCWSTR = w!("Software\\ChristmasTreeUltimate");
pub const LEGACY_SETTINGS: PCWSTR = w!("Software\\GovnoWithSmoke");
const MIGRATED: PCWSTR = w!("migratedFromGovnoWithSmoke");
const CONTROLLER: PCWSTR = w!("ChristmasTreeUltimateController");
const LEGACY_CONTROLLER: PCWSTR = w!("GovnoWithSmokeController");

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Both named objects stay alive until the app or installation finishes.
/// The old executable's existing single-instance check then also refuses a second overlay.
pub struct InstanceGuard {
    _current: Handle,
    _legacy: Handle,
}

impl InstanceGuard {
    pub fn app() -> Result<Option<Self>, String> {
        let (current, existed) = mutex(w!("Local\\ChristmasTreeUltimate"))?;
        if existed {
            return Ok(None);
        }
        let (legacy, _) = mutex(w!("Local\\GovnoWithSmoke"))?;
        close_controller(LEGACY_CONTROLLER)?;
        Ok(Some(Self { _current: current, _legacy: legacy }))
    }

    pub fn installation() -> Result<Self, String> {
        let (current, _) = mutex(w!("Local\\ChristmasTreeUltimate"))?;
        let (legacy, _) = mutex(w!("Local\\GovnoWithSmoke"))?;
        close_running()?;
        Ok(Self { _current: current, _legacy: legacy })
    }
}

fn mutex(name: PCWSTR) -> Result<(Handle, bool), String> {
    let handle = unsafe { CreateMutexW(None, false, name) }.map_err(|error| format!("single-instance lock: {error}"))?;
    let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    Ok((Handle(handle), existed))
}

pub fn close_running() -> Result<(), String> {
    close_controller(CONTROLLER)?;
    close_controller(LEGACY_CONTROLLER)
}

fn close_controller(class: PCWSTR) -> Result<(), String> {
    let Ok(window) = (unsafe { FindWindowW(class, PCWSTR::null()) }) else { return Ok(()) };
    let mut process = 0;
    unsafe { GetWindowThreadProcessId(window, Some(&mut process)) };
    if process == 0 {
        return Err("cannot identify the running app".to_string());
    }
    let process =
        Handle(unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, process) }.map_err(|error| format!("waiting for the running app: {error}"))?);
    if unsafe { WaitForSingleObject(process.0, 0) } == WAIT_OBJECT_0 {
        return Ok(());
    }
    unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) }.map_err(|error| format!("asking the running app to quit: {error}"))?;
    if unsafe { WaitForSingleObject(process.0, 10_000) } != WAIT_OBJECT_0 {
        return Err("the previous app did not exit within 10 seconds; close it from its tray menu and try again".to_string());
    }
    Ok(())
}

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn checked(status: WIN32_ERROR, action: &str) -> Result<(), String> {
    if status == ERROR_SUCCESS { Ok(()) } else { Err(format!("{action}: Windows error {}", status.0)) }
}

/// Existing destination values win. An interrupted copy can resume without overwriting settings changed in the new app.
/// The legacy key is a backup and is never changed or removed by migration.
pub fn migrate_settings() -> Result<(), String> {
    if settings_migrated() {
        return Ok(());
    }
    let mut source = HKEY::default();
    let opened = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, LEGACY_SETTINGS, None, KEY_QUERY_VALUE, &mut source) };
    if opened == ERROR_FILE_NOT_FOUND || opened == ERROR_PATH_NOT_FOUND {
        return Ok(());
    }
    checked(opened, "opening previous settings")?;
    let source = Key(source);
    let mut target = HKEY::default();
    checked(
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                SETTINGS,
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                None,
                &mut target,
                None,
            )
        },
        "creating migrated settings",
    )?;
    let target = Key(target);
    copy_missing_values(&source, &target)
}

pub fn settings_migrated() -> bool {
    let mut marker = 0u32;
    let mut marker_size = 4;
    let status = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, SETTINGS, MIGRATED, RRF_RT_REG_DWORD, None, Some((&raw mut marker).cast()), Some(&mut marker_size))
    };
    status == ERROR_SUCCESS && marker == 1
}

fn copy_missing_values(source: &Key, target: &Key) -> Result<(), String> {
    let mut index = 0;
    loop {
        let mut name = vec![0u16; 16_384];
        let mut data = vec![0u8; 4096];
        let (kind, length) = loop {
            let mut name_length = name.len() as u32;
            let mut data_length = data.len() as u32;
            let mut kind = 0;
            let status = unsafe {
                RegEnumValueW(
                    source.0,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut name_length,
                    None,
                    Some(&mut kind),
                    Some(data.as_mut_ptr()),
                    Some(&mut data_length),
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                checked(
                    unsafe { RegSetValueExW(target.0, MIGRATED, None, REG_DWORD, Some(&1u32.to_le_bytes())) },
                    "marking settings migration complete",
                )?;
                return Ok(());
            }
            if status == ERROR_MORE_DATA {
                let needed = (data_length as usize).max(data.len().saturating_mul(2));
                if needed > 16 * 1024 * 1024 {
                    return Err("a previous settings value is too large to migrate".to_string());
                }
                data.resize(needed, 0);
                continue;
            }
            checked(status, "reading previous settings")?;
            break (REG_VALUE_TYPE(kind), data_length as usize);
        };
        let value_name = PCWSTR(name.as_ptr());
        let existing = unsafe { RegQueryValueExW(target.0, value_name, None, None, None, None) };
        if existing == ERROR_FILE_NOT_FOUND {
            checked(unsafe { RegSetValueExW(target.0, value_name, None, kind, Some(&data[..length])) }, "copying previous settings")?;
        } else if existing != ERROR_SUCCESS && existing != ERROR_MORE_DATA {
            checked(existing, "checking existing settings")?;
        }
        index += 1;
    }
}
