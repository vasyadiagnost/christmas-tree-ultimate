//! Offline prerequisite bootstrap. Tests inject a backend; they never remove or
//! replace the computer's installed Microsoft runtimes.

use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::mem::size_of;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::{CloseHandle, FreeLibrary, HANDLE, HWND, WAIT_OBJECT_0};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE,
    WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WinVerifyTrust,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows::Win32::System::Registry::{
    HKEY_LOCAL_MACHINE, REG_ROUTINE_FLAGS, RRF_RT_REG_DWORD, RRF_SUBKEY_WOW6432KEY, RRF_SUBKEY_WOW6464KEY, RegGetValueW,
};
use windows::Win32::System::SystemInformation::{GetNativeSystemInfo, OSVERSIONINFOW, PROCESSOR_ARCHITECTURE_AMD64, SYSTEM_INFO};
use windows::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows::Win32::UI::Shell::{SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCSTR, PCWSTR, s, w};

use snowhush_core::text::{fill, tr};

const VC_KEY: PCWSTR = w!("SOFTWARE\\Microsoft\\VisualStudio\\14.0\\VC\\Runtimes\\x64");
/// The compiler is MSVC 14.51.36231. A sufficient existing runtime is preserved,
/// even when the bundled official installer happens to be newer.
pub const REQUIRED_VC_VERSION: RuntimeVersion = RuntimeVersion([14, 51, 36231, 0]);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuntimeVersion(pub [u32; 4]);

impl RuntimeVersion {
    fn parse(text: &str) -> Option<Self> {
        let mut values = [0; 4];
        let mut count = 0;
        for (i, piece) in text.trim_start_matches('v').split('.').enumerate() {
            if i >= 4 {
                return None;
            }
            values[i] = piece.parse().ok()?;
            count += 1;
        }
        (count >= 3).then_some(Self(values))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Phase {
    Windows,
    System,
    Runtime,
    Extracting,
    InstallingRuntime,
    VerifyingRuntime,
    Files,
}

impl Phase {
    pub fn label(self) -> &'static str {
        tr(match self {
            Self::Windows => "setup.progress.windows",
            Self::System => "setup.progress.system",
            Self::Runtime => "setup.progress.runtime",
            Self::Extracting => "setup.progress.extract",
            Self::InstallingRuntime => "setup.progress.install.runtime",
            Self::VerifyingRuntime => "setup.progress.verify.runtime",
            Self::Files => "setup.progress.files",
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub restart_required: bool,
}

trait Backend {
    fn installed_version(&mut self) -> Option<RuntimeVersion>;
    fn runtime_loads(&mut self) -> bool;
    fn payload_version(&self) -> RuntimeVersion;
    fn install(&mut self, progress: &mut dyn FnMut(Phase)) -> Result<u32, String>;
}

fn ensure_runtime(backend: &mut impl Backend, progress: &mut dyn FnMut(Phase)) -> Result<Outcome, String> {
    progress(Phase::Runtime);
    let before = backend.installed_version();
    let loadable = backend.runtime_loads();
    if before.is_some_and(|version| version >= REQUIRED_VC_VERSION) && loadable {
        return Ok(Outcome::default());
    }
    if before.is_some_and(|version| version > backend.payload_version()) {
        return Err(tr("setup.error.runtime.newer.broken").into());
    }
    let code = backend.install(progress)?;
    if code == 1602 {
        return Err(tr("setup.error.cancelled").into());
    }
    if code != 0 && code != 3010 && code != 1638 {
        return Err(fill("setup.error.runtime.exit", &[("code", &code.to_string())]));
    }
    progress(Phase::VerifyingRuntime);
    let version = backend.installed_version();
    let loadable = backend.runtime_loads();
    let restart_required = code == 3010;
    if !version.is_some_and(|version| version >= REQUIRED_VC_VERSION) || (!loadable && !restart_required) {
        return Err(tr("setup.error.runtime.verify").into());
    }
    Ok(Outcome { restart_required })
}

/// Pure read-only check used by installer diagnostics. It never extracts or runs
/// a redistributable, changes registration or asks for administrator rights.
pub fn check_ready() -> Result<(), String> {
    check_windows()?;
    check_system()?;
    if installed_runtime_version().is_some_and(|version| version >= REQUIRED_VC_VERSION) && runtime_loads() {
        Ok(())
    } else {
        Err(tr("setup.error.runtime.missing").into())
    }
}

pub fn prepare(payload: &'static [u8], payload_version: &str, mut progress: impl FnMut(Phase)) -> Result<Outcome, String> {
    progress(Phase::Windows);
    check_windows()?;
    progress(Phase::System);
    check_system()?;
    let payload_version = RuntimeVersion::parse(payload_version)
        .filter(|version| *version >= REQUIRED_VC_VERSION)
        .ok_or_else(|| tr("setup.error.payload.version").to_string())?;
    let mut backend = WindowsBackend { payload, payload_version, log: None };
    ensure_runtime(&mut backend, &mut progress).map_err(|error| {
        if let Some(log) = backend.log.filter(|path| path.exists()) { format!("{error}\n\n{}: {}", tr("setup.log"), log.display()) } else { error }
    })
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn RtlGetVersion(version: *mut OSVERSIONINFOW) -> i32;
}

fn check_windows() -> Result<(), String> {
    let mut version = OSVERSIONINFOW { dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32, ..Default::default() };
    if unsafe { RtlGetVersion(&mut version) } < 0 || version.dwMajorVersion < 10 {
        return Err(tr("setup.error.windows").into());
    }
    let mut system = SYSTEM_INFO::default();
    unsafe {
        GetNativeSystemInfo(&mut system);
    }
    if unsafe { system.Anonymous.Anonymous.wProcessorArchitecture } != PROCESSOR_ARCHITECTURE_AMD64 {
        return Err(tr("setup.error.architecture").into());
    }
    Ok(())
}

fn load_system(name: PCWSTR) -> windows::core::Result<windows::Win32::Foundation::HMODULE> {
    unsafe { LoadLibraryExW(name, None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
}

fn load_with_symbol(name: PCWSTR, symbol: PCSTR) -> bool {
    let Ok(module) = load_system(name) else {
        return false;
    };
    let found = unsafe { GetProcAddress(module, symbol) }.is_some();
    unsafe {
        let _ = FreeLibrary(module);
    }
    found
}

fn runtime_loads() -> bool {
    load_with_symbol(w!("vcruntime140.dll"), s!("_CxxThrowException")) && load_with_symbol(w!("ucrtbase.dll"), s!("malloc"))
}

fn check_system() -> Result<(), String> {
    for (name, wide, symbol) in [
        ("DirectComposition", w!("dcomp.dll"), s!("DCompositionCreateDevice")),
        ("Direct3D shader compiler", w!("d3dcompiler_47.dll"), s!("D3DCompile")),
        ("XAudio2", w!("xaudio2_9.dll"), s!("XAudio2Create")),
    ] {
        if !load_with_symbol(wide, symbol) {
            return Err(fill("setup.error.system", &[("component", name)]));
        }
    }
    let module = load_system(w!("d3d11.dll")).map_err(|_| fill("setup.error.system", &[("component", "Direct3D 11")]))?;
    let address = unsafe { GetProcAddress(module, s!("D3D11CreateDevice")) };
    type Create = unsafe extern "system" fn(*mut c_void, u32, HANDLE, u32, *const u32, u32, u32, *mut *mut c_void, *mut u32, *mut *mut c_void) -> i32;
    let result = if let Some(address) = address {
        let create: Create = unsafe { std::mem::transmute(address) };
        let levels = [0xb000_u32, 0xa100, 0xa000];
        let mut selected = 0;
        unsafe {
            create(
                std::ptr::null_mut(),
                1,
                HANDLE::default(),
                0x20,
                levels.as_ptr(),
                levels.len() as u32,
                7,
                std::ptr::null_mut(),
                &mut selected,
                std::ptr::null_mut(),
            )
        }
    } else {
        -1
    };
    unsafe {
        let _ = FreeLibrary(module);
    }
    if result < 0 {
        return Err(fill("setup.error.graphics", &[("code", &format!("0x{:08X}", result as u32))]));
    }
    Ok(())
}

pub fn installed_runtime_version() -> Option<RuntimeVersion> {
    [RRF_SUBKEY_WOW6464KEY, RRF_SUBKEY_WOW6432KEY]
        .into_iter()
        .filter_map(|view| {
            let read = |name| read_runtime_number(name, view);
            if read(w!("Installed")) != Some(1) {
                return None;
            }
            Some(RuntimeVersion([read(w!("Major"))?, read(w!("Minor"))?, read(w!("Bld"))?, read(w!("Rbld"))?]))
        })
        .max()
}

fn read_runtime_number(name: PCWSTR, view: REG_ROUTINE_FLAGS) -> Option<u32> {
    let mut value = 0_u32;
    let mut length = 4;
    unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, VC_KEY, name, RRF_RT_REG_DWORD | view, None, Some((&raw mut value).cast()), Some(&mut length)) }
        .is_ok()
        .then_some(value)
}

struct WindowsBackend {
    payload: &'static [u8],
    payload_version: RuntimeVersion,
    log: Option<PathBuf>,
}

impl Backend for WindowsBackend {
    fn installed_version(&mut self) -> Option<RuntimeVersion> {
        installed_runtime_version()
    }
    fn runtime_loads(&mut self) -> bool {
        runtime_loads()
    }
    fn payload_version(&self) -> RuntimeVersion {
        self.payload_version
    }
    fn install(&mut self, progress: &mut dyn FnMut(Phase)) -> Result<u32, String> {
        progress(Phase::Extracting);
        let payload = Payload::extract(self.payload)?;
        payload.verify_signature()?;
        let log = payload.directory.join("vc-runtime.log");
        self.log = Some(log.clone());
        let path = wide(&payload.path);
        let parameters = wide_str(&format!("/install /passive /norestart /log \"{}\"", log.display()));
        let directory = wide(&payload.directory);
        let mut launch = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpVerb: w!("runas"),
            lpFile: PCWSTR(path.as_ptr()),
            lpParameters: PCWSTR(parameters.as_ptr()),
            lpDirectory: PCWSTR(directory.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        progress(Phase::InstallingRuntime);
        if let Err(error) = unsafe { ShellExecuteExW(&mut launch) } {
            if error.code().0 as u32 & 0xffff == 1223 {
                return Err(tr("setup.error.cancelled").into());
            }
            return Err(fill("setup.error.runtime.launch", &[("error", &error.to_string())]));
        }
        if launch.hProcess.is_invalid() {
            return Err(tr("setup.error.runtime.process").into());
        }
        let process = Process(launch.hProcess);
        if unsafe { WaitForSingleObject(process.0, INFINITE) } != WAIT_OBJECT_0 {
            return Err(tr("setup.error.runtime.process").into());
        }
        let mut code = 0;
        unsafe { GetExitCodeProcess(process.0, &mut code) }.map_err(|error| fill("setup.error.runtime.launch", &[("error", &error.to_string())]))?;
        Ok(code)
    }
}

struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct Payload {
    directory: PathBuf,
    path: PathBuf,
    locked: Option<File>,
}

impl Payload {
    fn extract(bytes: &[u8]) -> Result<Self, String> {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| tr("setup.error.payload.folder").to_string())?
            .join("ChristmasTreeUltimate")
            .join("SetupCache");
        std::fs::create_dir_all(&base).map_err(|error| fill("setup.error.payload.write", &[("error", &error.to_string())]))?;
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let mut chosen = None;
        for attempt in 0..32 {
            let directory = base.join(format!("vc-{}-{nonce}-{attempt}", std::process::id()));
            match std::fs::create_dir(&directory) {
                Ok(()) => {
                    chosen = Some(directory);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(fill("setup.error.payload.write", &[("error", &error.to_string())])),
            }
        }
        let directory = chosen.ok_or_else(|| tr("setup.error.payload.folder").to_string())?;
        let path = directory.join("VC_redist.x64.exe");
        let mut payload = Self { directory, path, locked: None };
        let result = (|| -> std::io::Result<File> {
            let mut output = OpenOptions::new().create_new(true).write(true).share_mode(0).open(&payload.path)?;
            output.write_all(bytes)?;
            output.sync_all()?;
            drop(output);
            // Keep the exact embedded file locked against replacement/writing while
            // trust verification, elevation and the official installer use it.
            let mut locked = OpenOptions::new().read(true).share_mode(1).open(&payload.path)?;
            if locked.metadata()?.len() != bytes.len() as u64 {
                return Err(std::io::Error::other("payload length mismatch"));
            }
            let mut offset = 0;
            let mut buffer = [0_u8; 65536];
            loop {
                let n = locked.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                if buffer[..n] != bytes[offset..offset + n] {
                    return Err(std::io::Error::other("payload data mismatch"));
                }
                offset += n;
            }
            Ok(locked)
        })();
        payload.locked = Some(result.map_err(|error| fill("setup.error.payload.write", &[("error", &error.to_string())]))?);
        Ok(payload)
    }

    fn verify_signature(&self) -> Result<(), String> {
        let path = wide(&self.path);
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: PCWSTR(path.as_ptr()),
            hFile: HANDLE(self.locked.as_ref().unwrap().as_raw_handle()),
            ..Default::default()
        };
        let mut data = WINTRUST_DATA {
            cbStruct: size_of::<WINTRUST_DATA>() as u32,
            dwUIChoice: WTD_UI_NONE,
            fdwRevocationChecks: WTD_REVOKE_NONE,
            dwUnionChoice: WTD_CHOICE_FILE,
            Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
            dwStateAction: WTD_STATEACTION_VERIFY,
            dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
            ..Default::default()
        };
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let status = unsafe { WinVerifyTrust(HWND(-1_isize as *mut c_void), &mut action, (&raw mut data).cast()) };
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        unsafe {
            let _ = WinVerifyTrust(HWND(-1_isize as *mut c_void), &mut action, (&raw mut data).cast());
        }
        if status == 0 { Ok(()) } else { Err(fill("setup.error.payload.signature", &[("code", &format!("0x{:08X}", status as u32))])) }
    }
}

impl Drop for Payload {
    fn drop(&mut self) {
        drop(self.locked.take());
        let _ = std::fs::remove_file(&self.path);
        // Only our known payload and empty directory are removed. Diagnostic logs
        // survive a failed Microsoft installation; no recursive delete is used.
        let _ = std::fs::remove_dir(&self.directory);
    }
}

fn wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain([0]).collect()
}
fn wide_str(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        before: Option<RuntimeVersion>,
        after: Option<RuntimeVersion>,
        loads: bool,
        after_loads: bool,
        code: Result<u32, String>,
        calls: u32,
    }
    impl Backend for Fake {
        fn installed_version(&mut self) -> Option<RuntimeVersion> {
            if self.calls == 0 { self.before } else { self.after }
        }
        fn runtime_loads(&mut self) -> bool {
            if self.calls == 0 { self.loads } else { self.after_loads }
        }
        fn payload_version(&self) -> RuntimeVersion {
            RuntimeVersion([14, 52, 37029, 0])
        }
        fn install(&mut self, _: &mut dyn FnMut(Phase)) -> Result<u32, String> {
            self.calls += 1;
            self.code.clone()
        }
    }
    fn fake() -> Fake {
        Fake { before: None, after: Some(RuntimeVersion([14, 52, 37029, 0])), loads: false, after_loads: true, code: Ok(0), calls: 0 }
    }
    #[test]
    fn missing_runtime_installs_and_rechecks_without_touching_the_real_machine() {
        let mut f = fake();
        assert_eq!(ensure_runtime(&mut f, &mut |_| {}), Ok(Outcome::default()));
        assert_eq!(f.calls, 1);
    }
    #[test]
    fn sufficient_current_or_newer_runtime_never_runs_a_payload() {
        for version in [RuntimeVersion([14, 51, 36247, 0]), RuntimeVersion([14, 60, 1, 0])] {
            let mut f = fake();
            f.before = Some(version);
            f.loads = true;
            assert!(ensure_runtime(&mut f, &mut |_| {}).is_ok());
            assert_eq!(f.calls, 0);
        }
    }
    #[test]
    fn older_runtime_is_updated_and_cancel_or_failure_is_never_success() {
        for code in [1602, 1603] {
            let mut f = fake();
            f.before = Some(RuntimeVersion([14, 40, 1, 0]));
            f.code = Ok(code);
            assert!(ensure_runtime(&mut f, &mut |_| {}).is_err());
            assert_eq!(f.calls, 1);
        }
    }
    #[test]
    fn uac_cancellation_is_an_error_and_success_requires_postcheck() {
        let mut f = fake();
        f.code = Err("cancelled".into());
        assert!(ensure_runtime(&mut f, &mut |_| {}).is_err());
        let mut f = fake();
        f.after = None;
        assert!(ensure_runtime(&mut f, &mut |_| {}).is_err());
        let mut f = fake();
        f.after_loads = false;
        assert!(ensure_runtime(&mut f, &mut |_| {}).is_err());
    }
    #[test]
    fn reboot_required_is_distinct_from_ready_and_never_requests_reboot() {
        let mut f = fake();
        f.code = Ok(3010);
        f.after_loads = false;
        assert_eq!(ensure_runtime(&mut f, &mut |_| {}), Ok(Outcome { restart_required: true }));
    }
    #[test]
    fn broken_newer_runtime_is_not_downgraded() {
        let mut f = fake();
        f.before = Some(RuntimeVersion([14, 60, 1, 0]));
        assert!(ensure_runtime(&mut f, &mut |_| {}).is_err());
        assert_eq!(f.calls, 0);
    }
    #[test]
    fn runtime_versions_compare_all_four_components() {
        assert!(RuntimeVersion::parse("v14.51.36247.00").unwrap() >= REQUIRED_VC_VERSION);
        assert!(RuntimeVersion::parse("14.51.36230.9").unwrap() < REQUIRED_VC_VERSION);
        assert!(RuntimeVersion::parse("14.52.37029.0").unwrap() > REQUIRED_VC_VERSION);
        assert!(RuntimeVersion::parse("bad").is_none());
    }
    #[test]
    #[ignore]
    fn official_payload_extracts_with_write_lock_and_valid_authenticode_without_installing() {
        let path = std::env::var_os("CHRISTMAS_TREE_ULTIMATE_VC_REDIST").expect("set the official payload path");
        let bytes = std::fs::read(path).unwrap();
        let payload = Payload::extract(&bytes).unwrap();
        assert!(OpenOptions::new().write(true).open(&payload.path).is_err(), "payload must be protected from writing");
        assert!(std::fs::remove_file(&payload.path).is_err(), "payload must be protected from replacement");
        payload.verify_signature().unwrap();
        let directory = payload.directory.clone();
        drop(payload);
        assert!(!directory.exists(), "test extraction must clean up its own temporary directory");
    }
}
