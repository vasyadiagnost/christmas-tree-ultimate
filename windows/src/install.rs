//! Per-user installation with rollback. Tests redirect both HKCU and every known folder.

#[cfg(not(test))]
use std::ffi::c_void;
use std::fs;
use std::os::windows::fs::MetadataExt;
#[cfg(not(test))]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, GetLastError,
    HANDLE, WIN32_ERROR,
};
#[cfg(not(test))]
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD, REG_SZ, REG_VALUE_TYPE, RRF_RT_REG_SZ, RegCloseKey, RegDeleteKeyValueW, RegDeleteTreeW,
    RegEnumKeyExW, RegEnumValueW, RegGetValueW, RegOpenKeyExW, RegSetKeyValueW,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Shell::{FOLDERID_LocalAppData, FOLDERID_Programs, FOLDERID_UserProgramFiles, IShellLinkW, ShellExecuteW, ShellLink};
#[cfg(not(test))]
use windows::Win32::UI::Shell::{KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{GUID, HSTRING, Interface, PCWSTR, PWSTR, w};

const UNINSTALL: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\ChristmasTreeUltimate");
const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const SETTINGS: PCWSTR = w!("Software\\ChristmasTreeUltimate");
const NAME: PCWSTR = w!("ChristmasTreeUltimate");
const LEGACY_NAME: PCWSTR = w!("GovnoWithSmoke");
const LEGACY_UNINSTALL: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\GovnoWithSmoke");
const SHORTCUT_NAMES: [&str; 4] = ["christmas tree ultimate.lnk", "ГОВНО С ДЫМОМ.lnk", "govno with smoke.lnk", "GovnoWithSmoke.lnk"];
const EXE: &str = "christmas-tree-ultimate.exe";
const STAGED: &str = "christmas-tree-ultimate.exe.new";
const LOG: &str = "christmas-tree-ultimate.log";
const REPARSE_POINT: u32 = 0x400;
const INSTALL_FILES: [&str; 4] = [EXE, "LICENSE", "THIRD_PARTY_NOTICES.txt", STAGED];

pub fn installed_version() -> Option<String> {
    read_string(UNINSTALL, w!("DisplayVersion")).or_else(|| read_string(LEGACY_UNINSTALL, w!("DisplayVersion")))
}

pub fn install(app: &[u8], version: &str) -> Result<PathBuf, String> {
    let _instances = installation_lock()?;
    let paths = Paths::new()?;
    let transaction = Transaction::capture(&paths)?;
    let result = (|| {
        crate::identity::migrate_settings()?;
        fs::create_dir_all(&paths.folder).map_err(|error| format!("Не удалось создать папку приложения: {error}"))?;
        fs::write(paths.folder.join("LICENSE"), include_bytes!("../../LICENSE")).map_err(|error| format!("Не удалось записать лицензию: {error}"))?;
        fs::write(paths.folder.join("THIRD_PARTY_NOTICES.txt"), include_bytes!("../../THIRD_PARTY_NOTICES.txt"))
            .map_err(|error| format!("Не удалось записать сведения об авторах: {error}"))?;
        let staged = paths.folder.join(STAGED);
        fs::write(&staged, app).map_err(|error| format!("Не удалось подготовить приложение: {error}"))?;
        retry_file(|| fs::rename(&staged, &paths.exe)).map_err(|error| format!("Не удалось заменить приложение: {error}"))?;
        checkpoint("files")?;
        fs::create_dir_all(&paths.programs).map_err(|error| format!("Не удалось создать папку ярлыка: {error}"))?;
        shortcut(&paths.exe, &paths.programs.join(SHORTCUT_NAMES[0])).map_err(|error| format!("Не удалось создать ярлык: {error}"))?;
        for name in &SHORTCUT_NAMES[1..] {
            remove_file_if_present(&paths.programs.join(name))?;
        }
        checkpoint("shortcut")?;
        let quoted = format!("\"{}\"", paths.exe.display());
        write_string(UNINSTALL, w!("DisplayName"), "christmas tree ultimate")?;
        write_string(UNINSTALL, w!("DisplayVersion"), version)?;
        write_string(UNINSTALL, w!("Publisher"), "christmas tree ultimate (based on Snowhush by Mykhailo Bezruchko)")?;
        write_string(UNINSTALL, w!("DisplayIcon"), &paths.exe.display().to_string())?;
        write_string(UNINSTALL, w!("InstallLocation"), &paths.folder.display().to_string())?;
        write_string(UNINSTALL, w!("UninstallString"), &format!("{quoted} --uninstall"))?;
        write_string(UNINSTALL, w!("QuietUninstallString"), &format!("{quoted} --uninstall --silent"))?;
        write_number(UNINSTALL, w!("NoModify"), 1)?;
        write_number(UNINSTALL, w!("NoRepair"), 1)?;
        write_number(UNINSTALL, w!("EstimatedSize"), app.len().div_ceil(1024) as u32)?;
        checkpoint("registration")?;
        // Installing explicitly enables startup; portable launches never call this.
        write_string(RUN, NAME, &quoted)?;
        if read_string(RUN, NAME).as_deref() != Some(quoted.as_str()) {
            return Err("Не удалось проверить автозапуск установленного приложения.".to_string());
        }
        remove_run(LEGACY_NAME)?;
        remove_key(LEGACY_UNINSTALL)?;
        checkpoint("autostart")?;
        Ok(paths.exe.clone())
    })();
    transaction.finish(result)
}

/// Only the registered per-user installation can remove its own profile.
/// Legacy portable backup settings are deliberately retained.
pub fn uninstall() -> Result<(), String> {
    let paths = Paths::new()?;
    let registered = read_string(UNINSTALL, w!("InstallLocation")).ok_or("Установленная копия приложения не найдена.")?;
    if Path::new(&registered) != paths.folder {
        return Err("Удаление остановлено: папка установки не совпадает с папкой этого приложения.".to_string());
    }
    reject_reparse(&paths.folder)?;
    let _instances = installation_lock()?;
    let transaction = Transaction::capture(&paths)?;
    let result = (|| {
        remove_run(NAME)?;
        remove_key(UNINSTALL)?;
        remove_key(SETTINGS)?;
        remove_file_if_present(&paths.programs.join(SHORTCUT_NAMES[0]))?;
        remove_file_if_present(&paths.data.join(LOG))?;
        let _ = fs::remove_dir(&paths.data);
        schedule_cleanup(&paths.folder)?;
        Ok(())
    })();
    transaction.finish(result)
}

struct Paths {
    folder: PathBuf,
    exe: PathBuf,
    programs: PathBuf,
    data: PathBuf,
}
impl Paths {
    fn new() -> Result<Self, String> {
        let folder =
            known_folder(&FOLDERID_UserProgramFiles).ok_or("Не удалось найти пользовательскую папку программ.")?.join("ChristmasTreeUltimate");
        let programs = known_folder(&FOLDERID_Programs).ok_or("Не удалось найти меню «Пуск».")?;
        let data = known_folder(&FOLDERID_LocalAppData).ok_or("Не удалось найти пользовательскую папку данных.")?.join("ChristmasTreeUltimate");
        reject_reparse(&folder)?;
        reject_reparse(&programs)?;
        reject_reparse(&data)?;
        Ok(Self { exe: folder.join(EXE), folder, programs, data })
    }
    fn files(&self) -> Vec<PathBuf> {
        let mut files: Vec<_> = INSTALL_FILES.map(|name| self.folder.join(name)).into();
        files.push(self.data.join(LOG));
        files.extend(SHORTCUT_NAMES.map(|name| self.programs.join(name)));
        files
    }
}

fn reject_reparse(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_attributes() & REPARSE_POINT != 0 => {
            Err(format!("Небезопасный путь: {} является ссылкой или точкой перенаправления.", path.display()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Не удалось проверить {}: {error}", path.display())),
    }
}

struct SavedFile {
    path: PathBuf,
    original: Option<(Vec<u8>, fs::Permissions)>,
}
impl SavedFile {
    fn capture(path: PathBuf) -> Result<Self, String> {
        reject_reparse(&path)?;
        let original = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => {
                Some((fs::read(&path).map_err(|error| format!("Не удалось сохранить {}: {error}", path.display()))?, metadata.permissions()))
            }
            Ok(_) => return Err(format!("Ожидался файл: {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("Не удалось проверить {}: {error}", path.display())),
        };
        Ok(Self { path, original })
    }
    fn restore(&self) -> Result<(), String> {
        reject_reparse(&self.path)?;
        if let Some((bytes, permissions)) = &self.original {
            if fs::read(&self.path).ok().as_deref() != Some(bytes.as_slice()) {
                if let Some(parent) = self.path.parent() {
                    fs::create_dir_all(parent).map_err(|error| format!("Откат папки: {error}"))?;
                }
                retry_file(|| fs::write(&self.path, bytes)).map_err(|error| format!("Откат {}: {error}", self.path.display()))?;
            }
            fs::set_permissions(&self.path, permissions.clone()).map_err(|error| format!("Откат прав файла: {error}"))?;
            Ok(())
        } else {
            remove_file_if_present(&self.path)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawValue {
    name: Vec<u16>,
    kind: REG_VALUE_TYPE,
    bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct RegistryTree {
    values: Vec<RawValue>,
    children: Vec<(Vec<u16>, RegistryTree)>,
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open_key(parent: HKEY, name: PCWSTR) -> Result<Option<Key>, String> {
    let mut key = HKEY::default();
    let status = unsafe { RegOpenKeyExW(parent, name, None, KEY_READ, &mut key) };
    if absent(status) {
        return Ok(None);
    }
    registry_status(status, "Не удалось прочитать реестр")?;
    Ok(Some(Key(key)))
}

fn read_tree(key: HKEY) -> Result<RegistryTree, String> {
    let mut values = Vec::new();
    let mut index = 0;
    loop {
        let mut name = vec![0u16; 16_384];
        let mut bytes = vec![0u8; 4096];
        let value = loop {
            let mut name_length = name.len() as u32;
            let mut length = bytes.len() as u32;
            let mut kind = 0;
            let status = unsafe {
                RegEnumValueW(
                    key,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut name_length,
                    None,
                    Some(&mut kind),
                    Some(bytes.as_mut_ptr()),
                    Some(&mut length),
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                break None;
            }
            if status == ERROR_MORE_DATA {
                let needed = (length as usize).max(bytes.len().saturating_mul(2));
                if needed > 16 * 1024 * 1024 {
                    return Err("Слишком большое значение настроек для резервной копии.".to_string());
                }
                bytes.resize(needed, 0);
                continue;
            }
            registry_status(status, "Не удалось сохранить значения реестра")?;
            name.truncate(name_length as usize + 1);
            bytes.truncate(length as usize);
            break Some(RawValue { name, kind: REG_VALUE_TYPE(kind), bytes });
        };
        let Some(value) = value else { break };
        values.push(value);
        index += 1;
    }
    let mut children = Vec::new();
    let mut index = 0;
    loop {
        let mut name = vec![0u16; 256];
        let mut length = name.len() as u32;
        let status = unsafe { RegEnumKeyExW(key, index, Some(PWSTR(name.as_mut_ptr())), &mut length, None, None, None, None) };
        if status == ERROR_NO_MORE_ITEMS {
            break;
        }
        registry_status(status, "Не удалось сохранить подразделы реестра")?;
        name.truncate(length as usize + 1);
        let child = open_key(key, PCWSTR(name.as_ptr()))?.ok_or("Подраздел настроек исчез во время чтения.")?;
        children.push((name, read_tree(child.0)?));
        index += 1;
    }
    values.sort_by(|a, b| a.name.cmp(&b.name));
    children.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(RegistryTree { values, children })
}

fn snapshot_key(path: PCWSTR) -> Result<Option<RegistryTree>, String> {
    open_key(HKEY_CURRENT_USER, path)?.map(|key| read_tree(key.0)).transpose()
}

fn write_tree(path: &[u16], tree: &RegistryTree) -> Result<(), String> {
    if tree.values.is_empty() && tree.children.is_empty() {
        use windows::Win32::System::Registry::{KEY_WRITE, REG_OPTION_NON_VOLATILE, RegCreateKeyExW};
        let mut key = HKEY::default();
        registry_status(
            unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(path.as_ptr()),
                    None,
                    PCWSTR::null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_WRITE,
                    None,
                    &mut key,
                    None,
                )
            },
            "Откат раздела реестра",
        )?;
        let _key = Key(key);
    }
    for value in &tree.values {
        write_raw(PCWSTR(path.as_ptr()), value)?;
    }
    for (name, child) in &tree.children {
        let mut child_path = path[..path.len() - 1].to_vec();
        child_path.push(b'\\' as u16);
        child_path.extend_from_slice(name);
        write_tree(&child_path, child)?;
    }
    Ok(())
}

struct Transaction {
    files: Vec<SavedFile>,
    keys: Vec<(Vec<u16>, Option<RegistryTree>)>,
    run: Vec<(Vec<u16>, Option<RawValue>)>,
    folder: PathBuf,
    folder_existed: bool,
    done: bool,
}
impl Transaction {
    fn capture(paths: &Paths) -> Result<Self, String> {
        let files = paths.files().into_iter().map(SavedFile::capture).collect::<Result<Vec<_>, _>>()?;
        let keys = [SETTINGS, UNINSTALL, LEGACY_UNINSTALL]
            .into_iter()
            .map(|path| {
                let name = unsafe { path.as_wide() }.iter().copied().chain([0]).collect();
                snapshot_key(path).map(|tree| (name, tree))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let run_tree = snapshot_key(RUN)?;
        let run = [NAME, LEGACY_NAME]
            .into_iter()
            .map(|name| {
                let name: Vec<u16> = unsafe { name.as_wide() }.iter().copied().chain([0]).collect();
                let value = run_tree.as_ref().and_then(|tree| tree.values.iter().find(|value| value.name == name)).cloned();
                (name, value)
            })
            .collect();
        Ok(Self { files, keys, run, folder: paths.folder.clone(), folder_existed: paths.folder.exists(), done: false })
    }
    fn rollback(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        for file in self.files.iter().rev() {
            if let Err(error) = file.restore() {
                errors.push(error);
            }
        }
        for (path, tree) in &self.keys {
            if let Err(error) = remove_key(PCWSTR(path.as_ptr())).and_then(|_| match tree {
                Some(tree) => write_tree(path, tree),
                None => Ok(()),
            }) {
                errors.push(error);
            }
        }
        for (name, value) in &self.run {
            let result = if let Some(value) = value { write_raw(RUN, value) } else { remove_run(PCWSTR(name.as_ptr())) };
            if let Err(error) = result {
                errors.push(error);
            }
        }
        if !self.folder_existed {
            let _ = fs::remove_dir(&self.folder);
        }
        self.done = true;
        if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
    }
    fn finish<T>(mut self, result: Result<T, String>) -> Result<T, String> {
        match result {
            Ok(value) => {
                self.done = true;
                Ok(value)
            }
            Err(error) => match self.rollback() {
                Ok(()) => Err(format!("{error}\nПредыдущие файлы и настройки восстановлены.")),
                Err(rollback) => Err(format!("{error}\nНе удалось полностью восстановить прежнюю установку: {rollback}")),
            },
        }
    }
}
impl Drop for Transaction {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.rollback();
        }
    }
}

fn write_raw(key: PCWSTR, value: &RawValue) -> Result<(), String> {
    registry_status(
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key,
                PCWSTR(value.name.as_ptr()),
                value.kind.0,
                Some(value.bytes.as_ptr().cast()),
                value.bytes.len() as u32,
            )
        },
        "Не удалось записать реестр",
    )
}
fn retry_file(mut action: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    let started = Instant::now();
    loop {
        match action() {
            Ok(()) => return Ok(()),
            Err(error) if started.elapsed() >= Duration::from_secs(5) => return Err(error),
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}
fn remove_file_if_present(path: &Path) -> Result<(), String> {
    reject_reparse(path)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Не удалось удалить {}: {error}", path.display())),
    }
}

#[cfg(not(test))]
fn schedule_cleanup(folder: &Path) -> Result<(), String> {
    let literal = |path: &Path| format!("'{}'", path.display().to_string().replace('\'', "''"));
    let directory = literal(folder);
    let files = INSTALL_FILES.map(|name| literal(&folder.join(name))).join(",");
    // No recursive deletion: private documents or unknown files survive.
    let script = format!(
        "Start-Sleep -Seconds 3; $operationGuard=$null; try{{$createdNew=$false; $operationGuard=[System.Threading.Mutex]::new($false,'Local\\ChristmasTreeUltimateInstallationOperation',[ref]$createdNew); if(-not $createdNew){{return}}; if(Test-Path -LiteralPath 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\ChristmasTreeUltimate' -ErrorAction Stop){{return}}; $d={directory}; for($n=0;$n -lt 50;$n++){{try{{if(Test-Path -LiteralPath $d){{$i=Get-Item -LiteralPath $d -Force;if(($i.Attributes -band 1024) -ne 0){{throw 'redirected directory'}};foreach($p in @({files})){{if(Test-Path -LiteralPath $p){{$f=Get-Item -LiteralPath $p -Force;if($f.PSIsContainer -or (($f.Attributes -band 1024) -ne 0)){{throw 'unsafe file'}};Remove-Item -LiteralPath $p -Force -ErrorAction Stop}}}};if(-not (Get-ChildItem -LiteralPath $d -Force | Select-Object -First 1)){{Remove-Item -LiteralPath $d -Force -ErrorAction Stop}}}};break}}catch{{Start-Sleep -Milliseconds 200}}}}}}finally{{if($null -ne $operationGuard){{$operationGuard.Dispose()}}}}"
    );
    std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x0800_0000)
        .spawn()
        .map_err(|error| format!("Не удалось завершить удаление приложения: {error}"))?;
    Ok(())
}
#[cfg(test)]
fn schedule_cleanup(folder: &Path) -> Result<(), String> {
    reject_reparse(folder)?;
    for name in INSTALL_FILES {
        remove_file_if_present(&folder.join(name))?;
    }
    let _ = fs::remove_dir(folder);
    Ok(())
}
struct OperationGuard(HANDLE);

impl OperationGuard {
    fn acquire(name: PCWSTR) -> Result<Self, String> {
        let handle = unsafe { CreateMutexW(None, false, name) }.map_err(|error| format!("Не удалось защитить установку: {error}"))?;
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err("Установка/удаление уже выполняется.".to_string());
        }
        Ok(Self(handle))
    }
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(not(test))]
struct InstallationLock {
    _operation: OperationGuard,
    _instance: crate::identity::InstanceGuard,
}

#[cfg(not(test))]
fn installation_lock() -> Result<InstallationLock, String> {
    let operation = OperationGuard::acquire(w!("Local\\ChristmasTreeUltimateInstallationOperation"))?;
    let instance = crate::identity::InstanceGuard::installation()?;
    Ok(InstallationLock { _operation: operation, _instance: instance })
}
#[cfg(test)]
fn installation_lock() -> Result<(), String> {
    if tests::active() { Ok(()) } else { Err("Installer tests require an isolated profile.".to_string()) }
}
fn checkpoint(_name: &str) -> Result<(), String> {
    #[cfg(test)]
    if tests::should_fail(_name) {
        return Err(format!("Проверка отката: {_name}"));
    }
    Ok(())
}

pub fn launch(exe: &Path) {
    let path = exe.as_os_str().encode_wide_null();
    unsafe {
        ShellExecuteW(None, w!("open"), PCWSTR(path.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}
fn shortcut(target: &Path, at: &Path) -> windows::core::Result<()> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(PCWSTR(target.as_os_str().encode_wide_null().as_ptr()))?;
        link.SetWorkingDirectory(PCWSTR(target.parent().unwrap_or(target).as_os_str().encode_wide_null().as_ptr()))?;
        link.SetDescription(&HSTRING::from("christmas tree ultimate"))?;
        link.cast::<IPersistFile>()?.Save(PCWSTR(at.as_os_str().encode_wide_null().as_ptr()), true)
    }
}
fn known_folder(id: &GUID) -> Option<PathBuf> {
    #[cfg(test)]
    {
        return tests::folder(id);
    }
    #[cfg(not(test))]
    unsafe {
        let path = SHGetKnownFolderPath(id, KF_FLAG_DONT_VERIFY, None).ok()?;
        let text = path.to_string().ok();
        CoTaskMemFree(Some(path.0 as *const c_void));
        text.map(PathBuf::from)
    }
}
fn read_string(key: PCWSTR, name: PCWSTR) -> Option<String> {
    let mut buffer = [0u16; 32_768];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe { RegGetValueW(HKEY_CURRENT_USER, key, name, RRF_RT_REG_SZ, None, Some(buffer.as_mut_ptr().cast()), Some(&mut size)) };
    if status != ERROR_SUCCESS {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..buffer.iter().position(|&unit| unit == 0)?]))
}
fn write_string(key: PCWSTR, name: PCWSTR, value: &str) -> Result<(), String> {
    let bytes: Vec<u8> = value.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect();
    let name = unsafe { name.as_wide() }.iter().copied().chain([0]).collect();
    write_raw(key, &RawValue { name, kind: REG_SZ, bytes })
}
fn write_number(key: PCWSTR, name: PCWSTR, value: u32) -> Result<(), String> {
    let name = unsafe { name.as_wide() }.iter().copied().chain([0]).collect();
    write_raw(key, &RawValue { name, kind: REG_DWORD, bytes: value.to_le_bytes().to_vec() })
}
fn remove_run(name: PCWSTR) -> Result<(), String> {
    let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, name) };
    if absent(status) { Ok(()) } else { registry_status(status, "Не удалось удалить запись автозапуска") }
}
fn remove_key(key: PCWSTR) -> Result<(), String> {
    let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, key) };
    if absent(status) { Ok(()) } else { registry_status(status, "Не удалось удалить регистрацию приложения") }
}
fn absent(status: WIN32_ERROR) -> bool {
    status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND
}
fn registry_status(status: WIN32_ERROR, action: &str) -> Result<(), String> {
    if status == ERROR_SUCCESS { Ok(()) } else { Err(format!("{action}: Windows error {}", status.0)) }
}
trait Wide {
    fn encode_wide_null(&self) -> Vec<u16>;
}
impl Wide for std::ffi::OsStr {
    fn encode_wide_null(&self) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        self.encode_wide().chain([0]).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::sync::Mutex;
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
    use windows::Win32::System::Registry::{KEY_ALL_ACCESS, REG_BINARY, REG_OPTION_NON_VOLATILE, RegCreateKeyExW, RegOverridePredefKey};
    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn operation_mutex_rejects_a_second_transaction_and_releases_on_drop() {
        let name: Vec<u16> = format!(
            "Local\\ChristmasTreeUltimateInstallationOperationTest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        )
        .encode_utf16()
        .chain([0])
        .collect();
        let first = OperationGuard::acquire(PCWSTR(name.as_ptr())).unwrap();
        assert!(OperationGuard::acquire(PCWSTR(name.as_ptr())).is_err());
        drop(first);
        let next = OperationGuard::acquire(PCWSTR(name.as_ptr())).expect("the rejected second caller must close its handle too");
        drop(next);
    }
    thread_local! {
        static ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
        static FAIL: Cell<Option<&'static str>> = const { Cell::new(None) };
    }
    pub(super) fn active() -> bool {
        ROOT.with_borrow(Option::is_some)
    }
    pub(super) fn should_fail(name: &str) -> bool {
        FAIL.get() == Some(name)
    }
    pub(super) fn folder(id: &GUID) -> Option<PathBuf> {
        ROOT.with_borrow(|root| {
            root.as_ref().map(|root| {
                root.join(if *id == FOLDERID_UserProgramFiles {
                    "Programs"
                } else if *id == FOLDERID_Programs {
                    "StartMenu"
                } else {
                    "Data"
                })
            })
        })
    }
    struct Sandbox {
        path: PathBuf,
        registry: Vec<u16>,
        key: HKEY,
    }
    impl Sandbox {
        fn new() -> Self {
            let unique = format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
            let path = std::env::temp_dir().join(format!("ChristmasTreeUltimateInstallerTests-{unique}"));
            fs::create_dir(&path).unwrap();
            let registry: Vec<u16> = format!("Software\\ChristmasTreeUltimateInstallerTests\\{unique}").encode_utf16().chain([0]).collect();
            let mut key = HKEY::default();
            unsafe {
                CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
                assert_eq!(
                    RegCreateKeyExW(
                        HKEY_CURRENT_USER,
                        PCWSTR(registry.as_ptr()),
                        None,
                        PCWSTR::null(),
                        REG_OPTION_NON_VOLATILE,
                        KEY_ALL_ACCESS,
                        None,
                        &mut key,
                        None
                    ),
                    ERROR_SUCCESS
                );
                assert_eq!(RegOverridePredefKey(HKEY_CURRENT_USER, Some(key)), ERROR_SUCCESS);
            }
            ROOT.set(Some(path.clone()));
            Self { path, registry, key }
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            FAIL.set(None);
            ROOT.set(None);
            unsafe {
                assert_eq!(RegOverridePredefKey(HKEY_CURRENT_USER, None), ERROR_SUCCESS);
                let _ = RegCloseKey(self.key);
                let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(self.registry.as_ptr()));
                CoUninitialize();
            }
            assert_eq!(self.path.parent(), Some(std::env::temp_dir().as_path()));
            let _ = fs::remove_dir_all(&self.path);
        }
    }
    /// Run with --test-threads=1: HKCU overrides are confined to this test process.
    #[test]
    fn sandbox_install_update_rollback_and_scoped_uninstall() {
        let _serial = SERIAL.lock().unwrap();
        let sandbox = Sandbox::new();
        let paths = Paths::new().unwrap();
        write_number(crate::identity::LEGACY_SETTINGS, w!("holidayTreeX"), 137).unwrap();
        write_string(RUN, LEGACY_NAME, "\"old portable.exe\"").unwrap();
        write_string(RUN, w!("Unrelated"), "leave this alone").unwrap();
        for stage in ["files", "shortcut", "registration", "autostart"] {
            FAIL.set(Some(stage));
            assert!(install(b"new executable", "0.2.2").is_err(), "fresh failure at {stage}");
            assert!(!paths.exe.exists());
            assert!(!paths.folder.exists());
            assert_eq!(snapshot_key(SETTINGS).unwrap(), None);
            assert_eq!(snapshot_key(UNINSTALL).unwrap(), None);
            assert_eq!(read_string(RUN, LEGACY_NAME).as_deref(), Some("\"old portable.exe\""));
        }
        FAIL.set(None);
        assert_eq!(install(b"old executable", "0.2.1").unwrap(), paths.exe);
        assert_eq!(read_string(RUN, NAME).as_deref(), Some(format!("\"{}\"", paths.exe.display()).as_str()));
        assert_eq!(read_string(RUN, LEGACY_NAME), None);
        assert!(paths.programs.join(SHORTCUT_NAMES[0]).is_file());
        write_number(SETTINGS, w!("holidayTreeX"), 889).unwrap();
        write_number(SETTINGS, w!("mode"), 2).unwrap();
        write_raw(SETTINGS, &RawValue { name: "opaque".encode_utf16().chain([0]).collect(), kind: REG_BINARY, bytes: vec![0, 255, 7] }).unwrap();
        write_string(LEGACY_UNINSTALL, w!("DisplayVersion"), "0.1.0").unwrap();
        write_string(RUN, LEGACY_NAME, "legacy startup").unwrap();
        remove_run(NAME).unwrap();
        fs::write(paths.programs.join(SHORTCUT_NAMES[1]), b"legacy shortcut").unwrap();
        let settings = snapshot_key(SETTINGS).unwrap();
        let registration = snapshot_key(UNINSTALL).unwrap();
        let old_registration = snapshot_key(LEGACY_UNINSTALL).unwrap();
        let license = fs::read(paths.folder.join("LICENSE")).unwrap();
        let notices = fs::read(paths.folder.join("THIRD_PARTY_NOTICES.txt")).unwrap();
        for stage in ["files", "shortcut", "registration", "autostart"] {
            FAIL.set(Some(stage));
            assert!(install(b"replacement", "0.2.2").is_err(), "update failure at {stage}");
            assert_eq!(fs::read(&paths.exe).unwrap(), b"old executable");
            assert_eq!(fs::read(paths.folder.join("LICENSE")).unwrap(), license);
            assert_eq!(fs::read(paths.folder.join("THIRD_PARTY_NOTICES.txt")).unwrap(), notices);
            assert_eq!(fs::read(paths.programs.join(SHORTCUT_NAMES[1])).unwrap(), b"legacy shortcut");
            assert_eq!(snapshot_key(SETTINGS).unwrap(), settings);
            assert_eq!(snapshot_key(UNINSTALL).unwrap(), registration);
            assert_eq!(snapshot_key(LEGACY_UNINSTALL).unwrap(), old_registration);
            assert_eq!(read_string(RUN, NAME), None);
            assert_eq!(read_string(RUN, LEGACY_NAME).as_deref(), Some("legacy startup"));
        }
        FAIL.set(None);
        install(b"replacement", "0.2.2").unwrap();
        assert_eq!(snapshot_key(SETTINGS).unwrap(), settings);
        assert_eq!(installed_version().as_deref(), Some("0.2.2"));
        assert!(read_string(RUN, NAME).is_some());
        assert_eq!(read_string(RUN, LEGACY_NAME), None);
        assert_eq!(snapshot_key(LEGACY_UNINSTALL).unwrap(), None);
        assert_eq!(read_string(RUN, w!("Unrelated")).as_deref(), Some("leave this alone"));
        let outside = sandbox.path.join("unrelated");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep.txt"), b"private document").unwrap();
        write_string(UNINSTALL, w!("InstallLocation"), &outside.display().to_string()).unwrap();
        assert!(uninstall().is_err());
        assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"private document");
        assert_eq!(snapshot_key(SETTINGS).unwrap(), settings);
        write_string(UNINSTALL, w!("InstallLocation"), &paths.folder.display().to_string()).unwrap();
        fs::write(paths.folder.join("keep.txt"), b"user file").unwrap();
        fs::create_dir_all(&paths.data).unwrap();
        fs::write(paths.data.join(LOG), b"application log").unwrap();
        fs::write(paths.data.join("keep.txt"), b"another user file").unwrap();
        uninstall().unwrap();
        assert!(!paths.exe.exists());
        assert_eq!(fs::read(paths.folder.join("keep.txt")).unwrap(), b"user file");
        assert_eq!(fs::read(paths.data.join("keep.txt")).unwrap(), b"another user file");
        assert!(!paths.data.join(LOG).exists());
        assert_eq!(snapshot_key(SETTINGS).unwrap(), None);
        assert_eq!(snapshot_key(UNINSTALL).unwrap(), None);
        assert_eq!(read_string(RUN, NAME), None);
        assert!(snapshot_key(crate::identity::LEGACY_SETTINGS).unwrap().is_some());
        assert_eq!(read_string(RUN, w!("Unrelated")).as_deref(), Some("leave this alone"));
    }
}
