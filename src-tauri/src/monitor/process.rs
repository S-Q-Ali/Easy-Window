//! Read-only per-process snapshot for the Processes page.
//!
//! The snapshot is the identity layer: PID, start time, executable path,
//! publisher, and signature. Later tasks add CPU/GPU/disk samples on top. No
//! command lines, environment blocks, or file contents are ever collected.
//!
//! Signature verification uses two documented paths:
//!
//! 1. `WinVerifyTrust(WTD_CHOICE_FILE)` for an embedded Authenticode signature.
//! 2. A catalog lookup by file hash for system binaries such as `notepad.exe`,
//!    whose signature lives in a signed `.cat` file rather than in the PE.
//!
//! The catalog path is required: on this machine `WinVerifyTrust` returns
//! `TRUST_E_NOSIGNATURE` for `notepad.exe` even though Windows trusts it. The
//! catalog itself is then verified before its publisher is reported.

use std::collections::HashMap;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::risk::denylist::{self, CriticalReason, ProcessFacts};

use windows::Win32::Foundation::{
    CERT_E_CHAINING, CERT_E_EXPIRED, CERT_E_REVOKED, CERT_E_UNTRUSTEDROOT, CloseHandle,
    GENERIC_READ, HANDLE, HWND, LPARAM, TRUST_E_BAD_DIGEST, TRUST_E_EXPLICIT_DISTRUST,
    TRUST_E_NOSIGNATURE, TRUST_E_SUBJECT_FORM_UNKNOWN, TRUST_E_SUBJECT_NOT_TRUSTED, WPARAM,
};
use windows::Win32::Security::Cryptography::Catalog::{
    CATALOG_INFO, CryptCATAdminAcquireContext, CryptCATAdminAcquireContext2,
    CryptCATAdminCalcHashFromFileHandle2, CryptCATAdminEnumCatalogFromHash,
    CryptCATAdminReleaseCatalogContext, CryptCATAdminReleaseContext,
    CryptCATCatalogInfoFromContext,
};
use windows::Win32::Security::Cryptography::{
    CERT_CONTEXT, CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW,
};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE,
    WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvCertFromChain,
    WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows::Win32::System::SystemInformation::{GetSystemDirectoryW, GetWindowsDirectoryW};
use windows::Win32::System::Threading::{
    IsProcessCritical, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongPtrW, GetWindowThreadProcessId,
    IsWindowVisible, SMTO_ABORTIFHUNG, SMTO_BLOCK, SendMessageTimeoutW, WM_GETTEXT,
    WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PCWSTR, w};

/// Whether Windows can prove where an executable came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureStatus {
    /// A valid embedded or catalog signature chains to a trusted root.
    Trusted,
    /// No embedded signature and no catalog entry contains the file hash.
    Unsigned,
    /// A signature exists but does not verify (bad digest, untrusted root, ...).
    Invalid,
    /// The check could not run, for example because the path was unavailable.
    Unknown,
}

/// One process as shown in the table. Every field is read-only metadata.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSnapshot {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    /// Display form of the executable path. Actions re-resolve the real path
    /// before changing anything; this value is for display and grouping.
    pub exe_path: Option<String>,
    pub publisher: Option<String>,
    pub signature: SignatureStatus,
    pub session_id: Option<u32>,
    pub window_title: Option<String>,
    /// Seconds since the Unix epoch. Together with the PID this distinguishes a
    /// recycled PID from the process the user actually selected.
    pub start_time: u64,
    pub memory_bytes: u64,
    pub critical_reason: Option<CriticalReason>,
    /// False for every process the UI must refuse to close.
    pub selectable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SignatureInfo {
    status: SignatureStatus,
    publisher: Option<String>,
}

impl SignatureInfo {
    fn new(status: SignatureStatus, publisher: Option<String>) -> Self {
        Self { status, publisher }
    }

    fn unknown() -> Self {
        Self::new(SignatureStatus::Unknown, None)
    }

    fn unsigned() -> Self {
        Self::new(SignatureStatus::Unsigned, None)
    }

    fn invalid() -> Self {
        Self::new(SignatureStatus::Invalid, None)
    }

    fn trusted(publisher: Option<String>) -> Self {
        Self::new(SignatureStatus::Trusted, publisher)
    }
}

/// Collects process snapshots and caches signature results by executable path.
///
/// Signature checks are the expensive part of a snapshot, so a path is verified
/// once per sampler. Task 6's scheduler can drop the cache when it wants a
/// fresh view; the identity checks themselves are cheap.
pub struct ProcessSampler {
    system: System,
    signatures: HashMap<String, SignatureInfo>,
    system_dirs: Vec<PathBuf>,
}

impl ProcessSampler {
    pub fn new() -> Result<Self, sysinfo::Error> {
        Ok(Self {
            system: System::new()?,
            signatures: HashMap::new(),
            system_dirs: trusted_system_dirs(),
        })
    }

    /// Refreshes the process list and returns a stable, PID-sorted snapshot.
    pub fn snapshot(&mut self) -> Vec<ProcessSnapshot> {
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        let window_titles = window_titles_by_pid();

        let raw: Vec<RawProcess> = self
            .system
            .processes()
            .iter()
            .map(|(pid, process)| RawProcess {
                pid: pid.as_u32(),
                parent_pid: process.parent().map(Pid::as_u32),
                name: process.name().to_string_lossy().into_owned(),
                exe_path: process.exe().map(Path::to_path_buf),
                session_id: process.session_id().map(Pid::as_u32),
                start_time: process.start_time(),
                memory_bytes: process.memory(),
            })
            .collect();

        let mut snapshot: Vec<ProcessSnapshot> = raw
            .into_iter()
            .map(|process| {
                let signature = self.signature_for(process.exe_path.as_deref());
                let windows_critical = is_windows_critical(process.pid);
                let facts = ProcessFacts {
                    pid: process.pid,
                    name: &process.name,
                    exe_path: process.exe_path.as_deref(),
                    session_id: process.session_id,
                    windows_critical,
                };
                let critical_reason = denylist::assess(&facts, &self.system_dirs);

                ProcessSnapshot {
                    pid: process.pid,
                    parent_pid: process.parent_pid,
                    name: process.name,
                    exe_path: process
                        .exe_path
                        .as_deref()
                        .map(|path| path.to_string_lossy().into_owned()),
                    publisher: signature.publisher,
                    signature: signature.status,
                    session_id: process.session_id,
                    window_title: window_titles.get(&process.pid).cloned(),
                    start_time: process.start_time,
                    memory_bytes: process.memory_bytes,
                    critical_reason,
                    selectable: critical_reason.is_none(),
                }
            })
            .collect();

        snapshot.sort_by_key(|process| process.pid);
        snapshot
    }

    fn signature_for(&mut self, path: Option<&Path>) -> SignatureInfo {
        let Some(path) = path else {
            return SignatureInfo::unknown();
        };

        let key = path.to_string_lossy().to_ascii_lowercase();
        if let Some(info) = self.signatures.get(&key) {
            return info.clone();
        }

        let info = verify_signature(path);
        self.signatures.insert(key, info.clone());
        info
    }
}

struct RawProcess {
    pid: u32,
    parent_pid: Option<u32>,
    name: String,
    exe_path: Option<PathBuf>,
    session_id: Option<u32>,
    start_time: u64,
    memory_bytes: u64,
}

/// Verifies a file without network access: embedded signature first, then a
/// catalog lookup by SHA-256 hash with a SHA-1 fallback for older catalogs.
fn verify_signature(path: &Path) -> SignatureInfo {
    if !path.is_file() {
        return SignatureInfo::unknown();
    }

    match verify_embedded_signature(path) {
        TrustOutcome::Trusted(publisher) => SignatureInfo::trusted(publisher),
        TrustOutcome::NoSignature => match find_catalog(path) {
            CatalogLookup::Found(catalog_path) => match verify_embedded_signature(&catalog_path) {
                TrustOutcome::Trusted(publisher) => SignatureInfo::trusted(publisher),
                TrustOutcome::NoSignature | TrustOutcome::Invalid => SignatureInfo::invalid(),
                TrustOutcome::Unavailable => SignatureInfo::unknown(),
            },
            CatalogLookup::NotFound => SignatureInfo::unsigned(),
            CatalogLookup::Unavailable => SignatureInfo::unknown(),
        },
        TrustOutcome::Invalid => SignatureInfo::invalid(),
        TrustOutcome::Unavailable => SignatureInfo::unknown(),
    }
}

enum TrustOutcome {
    Trusted(Option<String>),
    NoSignature,
    Invalid,
    Unavailable,
}

fn verify_embedded_signature(path: &Path) -> TrustOutcome {
    let wide_path = wide_path(path);
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide_path.as_ptr()),
        hFile: HANDLE::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        // No network: use cached revocation information only, and skip
        // revocation checks entirely. The app never phones home.
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;

    let result = unsafe {
        WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        )
    };

    let publisher = if result == 0 {
        publisher_from_state(data.hWVTStateData)
    } else {
        None
    };

    // Always release the provider state, including after a failed check.
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        let _ = WinVerifyTrust(
            HWND::default(),
            &mut action,
            (&mut data as *mut WINTRUST_DATA).cast(),
        );
    }

    classify_trust_result(result, publisher)
}

fn classify_trust_result(result: i32, publisher: Option<String>) -> TrustOutcome {
    if result == 0 {
        return TrustOutcome::Trusted(publisher);
    }

    if result == TRUST_E_NOSIGNATURE.0 || result == TRUST_E_SUBJECT_FORM_UNKNOWN.0 {
        return TrustOutcome::NoSignature;
    }

    let known_failure = [
        TRUST_E_BAD_DIGEST.0,
        TRUST_E_EXPLICIT_DISTRUST.0,
        TRUST_E_SUBJECT_NOT_TRUSTED.0,
        CERT_E_UNTRUSTEDROOT.0,
        CERT_E_EXPIRED.0,
        CERT_E_REVOKED.0,
        CERT_E_CHAINING.0,
    ];
    if known_failure.contains(&result) {
        TrustOutcome::Invalid
    } else {
        TrustOutcome::Unavailable
    }
}

fn publisher_from_state(state: HANDLE) -> Option<String> {
    if state.is_invalid() {
        return None;
    }

    unsafe {
        let provider = WTHelperProvDataFromStateData(state);
        if provider.is_null() {
            return None;
        }

        let signer = WTHelperGetProvSignerFromChain(provider, 0, false, 0);
        if signer.is_null() {
            return None;
        }

        let certificate = WTHelperGetProvCertFromChain(signer, 0);
        if certificate.is_null() {
            return None;
        }

        let context = (*certificate).pCert;
        if context.is_null() {
            return None;
        }

        certificate_display_name(context)
    }
}

fn certificate_display_name(certificate: *const CERT_CONTEXT) -> Option<String> {
    unsafe {
        let length = CertGetNameStringW(certificate, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, None);
        if length <= 1 {
            return None;
        }

        let mut buffer = vec![0u16; length as usize];
        let written = CertGetNameStringW(
            certificate,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            0,
            None,
            Some(&mut buffer),
        );
        if written == 0 {
            return None;
        }

        let end = buffer
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(buffer.len());
        if end == 0 {
            return None;
        }

        Some(String::from_utf16_lossy(&buffer[..end]))
    }
}

enum CatalogLookup {
    Found(PathBuf),
    NotFound,
    Unavailable,
}

/// Finds the installed catalog that contains this file's hash.
///
/// `CryptCATAdminEnumCatalogFromHash` searches the CatRoot store. A returned
/// catalog is not automatically trusted, so the caller verifies the catalog's
/// own signature before reporting the file as trusted.
fn find_catalog(path: &Path) -> CatalogLookup {
    let Some(file) = open_for_reading(path) else {
        return CatalogLookup::Unavailable;
    };

    let mut primary_unavailable = false;
    let result = match find_catalog_with_sha256(file) {
        CatalogLookup::Found(catalog_path) => CatalogLookup::Found(catalog_path),
        CatalogLookup::NotFound => match find_catalog_with_sha1(file) {
            CatalogLookup::Found(catalog_path) => CatalogLookup::Found(catalog_path),
            CatalogLookup::NotFound => CatalogLookup::NotFound,
            CatalogLookup::Unavailable => CatalogLookup::Unavailable,
        },
        CatalogLookup::Unavailable => {
            primary_unavailable = true;
            match find_catalog_with_sha1(file) {
                CatalogLookup::Found(catalog_path) => CatalogLookup::Found(catalog_path),
                CatalogLookup::NotFound => CatalogLookup::NotFound,
                CatalogLookup::Unavailable => CatalogLookup::Unavailable,
            }
        }
    };

    unsafe {
        let _ = CloseHandle(file);
    }

    if primary_unavailable && matches!(result, CatalogLookup::NotFound) {
        return CatalogLookup::Unavailable;
    }

    result
}

fn find_catalog_with_sha256(file: HANDLE) -> CatalogLookup {
    let mut admin: isize = 0;
    let acquired =
        unsafe { CryptCATAdminAcquireContext2(&mut admin, None, w!("SHA256"), None, None) };
    if acquired.is_err() || admin == 0 {
        return CatalogLookup::Unavailable;
    }

    let result = catalog_lookup_with_admin(admin, file);
    unsafe {
        let _ = CryptCATAdminReleaseContext(admin, 0);
    }
    result
}

fn find_catalog_with_sha1(file: HANDLE) -> CatalogLookup {
    let mut admin: isize = 0;
    let acquired = unsafe { CryptCATAdminAcquireContext(&mut admin, None, None) };
    if acquired.is_err() || admin == 0 {
        return CatalogLookup::Unavailable;
    }

    let result = catalog_lookup_with_admin(admin, file);
    unsafe {
        let _ = CryptCATAdminReleaseContext(admin, 0);
    }
    result
}

fn catalog_lookup_with_admin(admin: isize, file: HANDLE) -> CatalogLookup {
    let Some(hash) = catalog_hash(admin, file) else {
        return CatalogLookup::Unavailable;
    };

    let catalog = unsafe { CryptCATAdminEnumCatalogFromHash(admin, &hash, None, None) };
    if catalog == 0 {
        return CatalogLookup::NotFound;
    }

    let mut info = CATALOG_INFO {
        cbStruct: size_of::<CATALOG_INFO>() as u32,
        ..Default::default()
    };
    let info_result = unsafe { CryptCATCatalogInfoFromContext(catalog, &mut info, 0) };
    let catalog_path = info_result.ok().and_then(|()| catalog_info_path(&info));

    unsafe {
        let _ = CryptCATAdminReleaseCatalogContext(admin, catalog, 0);
    }

    match catalog_path {
        Some(path) => CatalogLookup::Found(path),
        None => CatalogLookup::Unavailable,
    }
}

fn catalog_hash(admin: isize, file: HANDLE) -> Option<Vec<u8>> {
    let mut size: u32 = 0;
    let first = unsafe { CryptCATAdminCalcHashFromFileHandle2(admin, file, &mut size, None, None) };
    if first.is_err() || size == 0 || size > 128 {
        return None;
    }

    let mut hash = vec![0u8; size as usize];
    let second = unsafe {
        CryptCATAdminCalcHashFromFileHandle2(admin, file, &mut size, Some(hash.as_mut_ptr()), None)
    };
    if second.is_err() || size == 0 || size as usize > hash.len() {
        return None;
    }

    hash.truncate(size as usize);
    Some(hash)
}

fn catalog_info_path(info: &CATALOG_INFO) -> Option<PathBuf> {
    let end = info.wszCatalogFile.iter().position(|&unit| unit == 0)?;
    if end == 0 {
        return None;
    }

    Some(PathBuf::from(String::from_utf16_lossy(
        &info.wszCatalogFile[..end],
    )))
}

fn open_for_reading(path: &Path) -> Option<HANDLE> {
    let wide_path = wide_path(path);
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_path.as_ptr()),
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    };

    handle.ok().filter(|handle| !handle.is_invalid())
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn is_windows_critical(pid: u32) -> Option<bool> {
    // PID 0 and PID 4 are pseudo-processes that cannot be opened.
    if pid == 0 || pid == 4 {
        return None;
    }

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut critical = BOOL(0);
    let result = unsafe { IsProcessCritical(handle, &mut critical) };
    unsafe {
        let _ = CloseHandle(handle);
    }

    result.ok().map(|()| critical.as_bool())
}

/// Returns the title of the topmost unowned, visible window for each process.
fn window_titles_by_pid() -> HashMap<u32, String> {
    let mut titles: HashMap<u32, String> = HashMap::new();
    let data = &mut titles as *mut HashMap<u32, String>;

    unsafe {
        let _ = EnumWindows(Some(collect_window_title), LPARAM(data as isize));
    }

    titles
}

unsafe extern "system" fn collect_window_title(window: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        if !IsWindowVisible(window).as_bool() {
            return BOOL(1);
        }

        let ex_style = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
        if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
            return BOOL(1);
        }

        // Owned windows are usually dialogs and popups; the owner is the app's
        // real top-level window.
        if GetWindow(window, GW_OWNER).is_ok() {
            return BOOL(1);
        }

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(window, Some(&mut pid));
        if pid == 0 {
            return BOOL(1);
        }

        if let Some(title) = window_title(window) {
            let titles = &mut *(lparam.0 as *mut HashMap<u32, String>);
            titles.entry(pid).or_insert(title);
        }
    }

    BOOL(1)
}

fn window_title(window: HWND) -> Option<String> {
    let mut buffer = [0u16; 512];
    let mut result: usize = 0;

    let sent = unsafe {
        SendMessageTimeoutW(
            window,
            WM_GETTEXT,
            WPARAM(buffer.len()),
            LPARAM(buffer.as_mut_ptr() as isize),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            100,
            Some(&mut result as *mut usize),
        )
    };
    if sent.0 == 0 {
        return None;
    }

    let end = buffer.iter().position(|&unit| unit == 0)?;
    if end == 0 {
        return None;
    }

    let title = String::from_utf16_lossy(&buffer[..end]);
    let title = title.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

/// Only the real system directories protect a process by name. A file called
/// `csrss.exe` in a temp folder must remain selectable.
fn trusted_system_dirs() -> Vec<PathBuf> {
    let mut directories = Vec::with_capacity(2);

    if let Some(system32) = system_directory() {
        directories.push(system32);
    }

    if let Some(windows) = windows_directory() {
        let syswow64 = windows.join("SysWOW64");
        if syswow64.is_dir() {
            directories.push(syswow64);
        }
    }

    directories
}

fn system_directory() -> Option<PathBuf> {
    read_system_path(|buffer| unsafe { GetSystemDirectoryW(Some(buffer)) })
}

fn windows_directory() -> Option<PathBuf> {
    read_system_path(|buffer| unsafe { GetWindowsDirectoryW(Some(buffer)) })
}

fn read_system_path(mut read: impl FnMut(&mut [u16]) -> u32) -> Option<PathBuf> {
    let mut size = 260usize;

    for _ in 0..2 {
        let mut buffer = vec![0u16; size];
        let length = read(&mut buffer);
        if length == 0 {
            return None;
        }

        let length = length as usize;
        if length < buffer.len() {
            return Some(PathBuf::from(String::from_utf16_lossy(&buffer[..length])));
        }

        size = length + 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sampler() -> ProcessSampler {
        ProcessSampler::new().expect("process sampler")
    }

    #[test]
    fn snapshot_contains_the_current_process_with_identity_fields() {
        let mut sampler = sampler();
        let snapshot = sampler.snapshot();
        let current = snapshot
            .iter()
            .find(|process| process.pid == std::process::id())
            .expect("current process in the snapshot");

        assert!(current.start_time > 0);
        assert!(current.exe_path.is_some());
        assert_eq!(current.selectable, current.critical_reason.is_none());
    }

    #[test]
    fn system_process_is_flagged_and_not_selectable() {
        let mut sampler = sampler();
        let snapshot = sampler.snapshot();
        let system = snapshot
            .iter()
            .find(|process| process.pid == 4)
            .expect("PID 4 in the snapshot");

        assert_eq!(system.critical_reason, Some(CriticalReason::SystemProcess));
        assert!(!system.selectable);
    }

    #[test]
    fn no_critical_process_is_selectable() {
        let mut sampler = sampler();

        for process in sampler.snapshot() {
            assert_eq!(
                process.selectable,
                process.critical_reason.is_none(),
                "pid {} ({})",
                process.pid,
                process.name
            );
        }
    }

    #[test]
    fn processes_without_an_exe_path_are_not_selectable() {
        let mut sampler = sampler();
        let snapshot = sampler.snapshot();
        let unverified: Vec<_> = snapshot
            .iter()
            .filter(|process| process.pid > 4 && process.exe_path.is_none())
            .collect();

        assert!(
            !unverified.is_empty(),
            "expected protected processes whose path is unreadable"
        );
        for process in unverified {
            assert!(
                process.critical_reason.is_some(),
                "pid {} ({})",
                process.pid,
                process.name
            );
            assert!(!process.selectable);
        }
    }

    #[test]
    fn system_binaries_are_trusted_via_catalog() {
        let path = PathBuf::from(std::env::var("SystemRoot").expect("SystemRoot"))
            .join("System32")
            .join("notepad.exe");
        if !path.is_file() {
            return;
        }

        assert!(
            matches!(find_catalog(&path), CatalogLookup::Found(_)),
            "notepad.exe should have a catalog entry"
        );

        let info = verify_signature(&path);
        assert_eq!(info.status, SignatureStatus::Trusted, "{}", path.display());
        assert!(info.publisher.is_some());
    }

    #[test]
    fn missing_files_are_unknown() {
        let info = verify_signature(Path::new(r"C:\EasyWindow\does-not-exist\ghost.exe"));

        assert_eq!(info.status, SignatureStatus::Unknown);
        assert_eq!(info.publisher, None);
    }

    #[test]
    fn unsigned_files_are_reported_as_unsigned() {
        let path =
            std::env::temp_dir().join(format!("easy-window-unsigned-{}.bin", std::process::id()));
        std::fs::write(&path, b"not a signed Windows binary").expect("write temp file");

        let info = verify_signature(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(info.status, SignatureStatus::Unsigned);
        assert_eq!(info.publisher, None);
    }

    #[test]
    fn signature_cache_reuses_results() {
        let mut sampler = sampler();
        let path = PathBuf::from(std::env::var("SystemRoot").expect("SystemRoot"))
            .join("System32")
            .join("kernel32.dll");
        if !path.is_file() {
            return;
        }

        let first = sampler.signature_for(Some(&path));
        let second = sampler.signature_for(Some(&path));

        assert_eq!(first, second);
        assert_eq!(sampler.signatures.len(), 1);
    }
}
