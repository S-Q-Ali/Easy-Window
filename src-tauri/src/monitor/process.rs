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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::risk::denylist::{self, CriticalReason, ProcessFacts};

use windows::Win32::Foundation::{
    CERT_E_CHAINING, CERT_E_EXPIRED, CERT_E_REVOKED, CERT_E_UNTRUSTEDROOT, CloseHandle,
    GENERIC_READ, HANDLE, HWND, LPARAM, TRUST_E_BAD_DIGEST, TRUST_E_EXPLICIT_DISTRUST,
    TRUST_E_NOSIGNATURE, TRUST_E_SUBJECT_FORM_UNKNOWN, TRUST_E_SUBJECT_NOT_TRUSTED,
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
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_BEGIN, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING, SetFilePointerEx,
};
use windows::Win32::System::SystemInformation::{GetSystemDirectoryW, GetWindowsDirectoryW};
use windows::Win32::System::Threading::{
    IsProcessCritical, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongPtrW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, WS_EX_TOOLWINDOW,
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
    /// Why the signature is `invalid` or `unknown`: a Windows error name such
    /// as `CERT_E_EXPIRED`, a raw code such as `0x80096004`, or one of the
    /// local codes spelled here. An expired certificate on a genuine binary is
    /// a boring problem, and the UI must be able to say so.
    pub signature_reason: Option<String>,
    pub session_id: Option<u32>,
    pub window_title: Option<String>,
    /// Seconds since the Unix epoch. Together with the PID this distinguishes a
    /// recycled PID from the process the user actually selected.
    pub start_time: u64,
    pub memory_bytes: u64,
    /// Share of the whole machine, so one saturated core on an eight-core PC
    /// reads 12.5 — the number Task Manager shows next to the same process.
    /// `None` means unmeasured, never idle: the first tick has no window yet,
    /// and a process whose handle is unavailable cannot be sampled at all.
    pub cpu_percent: Option<f32>,
    pub disk_read_bytes_per_sec: Option<f64>,
    pub disk_write_bytes_per_sec: Option<f64>,
    pub critical_reason: Option<CriticalReason>,
    /// False for every process the UI must refuse to close.
    pub selectable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SignatureInfo {
    status: SignatureStatus,
    publisher: Option<String>,
    reason: Option<String>,
}

impl SignatureInfo {
    fn new(status: SignatureStatus, publisher: Option<String>, reason: Option<String>) -> Self {
        Self {
            status,
            publisher,
            reason,
        }
    }

    fn unknown() -> Self {
        Self::new(SignatureStatus::Unknown, None, None)
    }

    fn unsigned() -> Self {
        Self::new(SignatureStatus::Unsigned, None, None)
    }

    fn invalid(reason: String) -> Self {
        Self::new(SignatureStatus::Invalid, None, Some(reason))
    }

    fn unverified(reason: String) -> Self {
        Self::new(SignatureStatus::Unknown, None, Some(reason))
    }

    fn trusted(publisher: Option<String>) -> Self {
        Self::new(SignatureStatus::Trusted, publisher, None)
    }
}

/// Signature results, keyed by path, size, and modification time.
///
/// A single check costs a few hundred milliseconds, so samplers share one
/// cache instead of re-verifying the same binaries on every refresh. The key
/// carries the file's size and mtime so a binary replaced by an update cannot
/// inherit an old `Trusted`, and `Unknown` is never stored: it came from a
/// check that could not run, and a cached guess has no way to expire.
///
/// Caching, not threading, is what makes this cheap: four worker threads took
/// the same 9.1 s for a cold snapshot as one thread took 9.2 s, because
/// Windows serialises the trust and catalog calls inside its own stores.
#[derive(Clone, Default)]
pub(crate) struct SignatureCache(Arc<Mutex<HashMap<String, SignatureInfo>>>);

impl SignatureCache {
    fn get(&self, key: &str) -> Option<SignatureInfo> {
        self.0.lock().expect("signature cache").get(key).cloned()
    }

    fn insert(&self, key: String, info: SignatureInfo) {
        self.0.lock().expect("signature cache").insert(key, info);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.lock().expect("signature cache").len()
    }
}

/// Collects process snapshots and caches signature results by executable path.
pub struct ProcessSampler {
    system: System,
    signatures: SignatureCache,
    system_dirs: Vec<PathBuf>,
    /// When the previous snapshot finished. CPU and disk rates are deltas
    /// between two snapshots, so the first one has no window to divide by.
    last_tick: Option<Instant>,
    /// Whether the machine's own idle/busy counters have been read once. The
    /// first read only records a baseline; it has no previous value to compare
    /// against, and `sysinfo` reports zero for it.
    system_cpu_sampled: bool,
}

impl ProcessSampler {
    pub fn new() -> Result<Self, sysinfo::Error> {
        Self::with_cache(SignatureCache::default())
    }

    pub(crate) fn with_cache(signatures: SignatureCache) -> Result<Self, sysinfo::Error> {
        Ok(Self {
            system: System::new()?,
            signatures,
            system_dirs: trusted_system_dirs(),
            last_tick: None,
            system_cpu_sampled: false,
        })
    }

    /// Refreshes the process list and returns a stable, PID-sorted snapshot.
    pub fn snapshot(&mut self) -> Vec<ProcessSnapshot> {
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        let elapsed = self
            .last_tick
            .replace(Instant::now())
            .map(|start| start.elapsed());
        // sysinfo scales its raw CPU figure by this same count, so dividing by
        // it converts to a share of the machine.
        let logical_cores = self.system.cpus().len();
        let window_titles = window_titles_by_pid();

        let raw: Vec<RawProcess> = self
            .system
            .processes()
            .iter()
            .map(|(pid, process)| {
                let disk = process.disk_usage();
                RawProcess {
                    pid: pid.as_u32(),
                    parent_pid: process.parent().map(Pid::as_u32),
                    name: process.name().to_string_lossy().into_owned(),
                    exe_path: process.exe().map(Path::to_path_buf),
                    session_id: process.session_id().map(Pid::as_u32),
                    start_time: process.start_time(),
                    memory_bytes: process.memory(),
                    cpu_usage: process.cpu_usage(),
                    read_bytes: disk.read_bytes,
                    written_bytes: disk.written_bytes,
                }
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

                // Without a readable path there is no open handle, and sysinfo
                // reports a permanent zero for those processes. That zero must
                // stay `None`, or the table would call an unmeasurable process
                // idle.
                let (cpu_percent, disk_read_bytes_per_sec, disk_write_bytes_per_sec) =
                    match (elapsed, process.exe_path.is_some()) {
                        (Some(elapsed), true) => (
                            cpu_percentage(process.cpu_usage, logical_cores),
                            bytes_per_sec(process.read_bytes, elapsed),
                            bytes_per_sec(process.written_bytes, elapsed),
                        ),
                        _ => (None, None, None),
                    };

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
                    signature_reason: signature.reason,
                    session_id: process.session_id,
                    window_title: window_titles.get(&process.pid).cloned(),
                    start_time: process.start_time,
                    memory_bytes: process.memory_bytes,
                    cpu_percent,
                    disk_read_bytes_per_sec,
                    disk_write_bytes_per_sec,
                    critical_reason,
                    selectable: critical_reason.is_none(),
                }
            })
            .collect();

        snapshot.sort_by_key(|process| process.pid);
        snapshot
    }

    /// The whole machine's CPU as a share of itself, over the window between
    /// two calls. `None` until there is such a window: `sysinfo` answers zero
    /// for a baseline it has nothing to compare, and a zero here would read as
    /// an idle machine.
    pub fn system_cpu_percent(&mut self) -> Option<f64> {
        let has_baseline = self.system_cpu_sampled;
        self.system_cpu_sampled = true;
        self.system.refresh_cpu_usage();

        has_baseline.then(|| self.system.global_cpu_usage() as f64)
    }

    fn signature_for(&self, path: Option<&Path>) -> SignatureInfo {
        let Some(path) = path else {
            return SignatureInfo::unknown();
        };

        // Without a stat there is nothing to key the result on, and a result
        // that cannot be invalidated must not be remembered.
        let Some(key) = cache_key(path) else {
            return verify_signature(path);
        };

        if let Some(info) = self.signatures.get(&key) {
            return info;
        }

        let info = verify_signature(path);
        if info.status != SignatureStatus::Unknown {
            self.signatures.insert(key, info.clone());
        }
        info
    }
}

/// Cache key for a verified file: its lowercase path plus the identity of the
/// bytes at that path. A binary replaced by an update gets a new key, so an
/// old `Trusted` result can never be reused for different content.
fn cache_key(path: &Path) -> Option<String> {
    let metadata = path.metadata().ok()?;
    let modified = metadata.modified().ok()?;
    let age = modified.duration_since(UNIX_EPOCH).ok()?;

    Some(format!(
        "{}\u{1}{}\u{1}{}",
        path.to_string_lossy().to_ascii_lowercase(),
        metadata.len(),
        age.as_secs()
    ))
}

struct RawProcess {
    pid: u32,
    parent_pid: Option<u32>,
    name: String,
    exe_path: Option<PathBuf>,
    session_id: Option<u32>,
    start_time: u64,
    memory_bytes: u64,
    cpu_usage: f32,
    read_bytes: u64,
    written_bytes: u64,
}

/// Converts sysinfo's per-core CPU figure into a share of the whole machine.
fn cpu_percentage(cpu_usage: f32, logical_cores: usize) -> Option<f32> {
    if logical_cores == 0 {
        return None;
    }
    Some(cpu_usage / logical_cores as f32)
}

/// Converts bytes counted between two snapshots into a per-second rate.
fn bytes_per_sec(bytes: u64, elapsed: Duration) -> Option<f64> {
    if elapsed.is_zero() {
        return None;
    }
    Some(bytes as f64 / elapsed.as_secs_f64())
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
                TrustOutcome::NoSignature => SignatureInfo::invalid("CATALOG_UNSIGNED".to_string()),
                TrustOutcome::Invalid(reason) => SignatureInfo::invalid(reason),
                TrustOutcome::Unavailable(reason) => SignatureInfo::unverified(reason),
            },
            CatalogLookup::NotFound => SignatureInfo::unsigned(),
            CatalogLookup::Unavailable => SignatureInfo::unknown(),
        },
        TrustOutcome::Invalid(reason) => SignatureInfo::invalid(reason),
        TrustOutcome::Unavailable(reason) => SignatureInfo::unverified(reason),
    }
}

#[derive(Debug)]
enum TrustOutcome {
    Trusted(Option<String>),
    NoSignature,
    Invalid(String),
    Unavailable(String),
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
        // No network: cached URLs only, so chain building cannot fetch a
        // missing intermediate certificate. Revocation is skipped outright
        // (WTD_REVOKE_NONE), so a revoked but otherwise well-formed
        // certificate still reports Trusted; only the machine's local
        // Disallowed store surfaces as a failure. Trusted does not mean
        // "not revoked", and risk scoring must not claim it.
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

/// Trust failures we can name. Anything else keeps its raw code, because an
/// unrecognised code is a reason to report `unknown`, not to guess.
const TRUST_FAILURES: &[(&str, i32)] = &[
    ("TRUST_E_BAD_DIGEST", TRUST_E_BAD_DIGEST.0),
    ("TRUST_E_EXPLICIT_DISTRUST", TRUST_E_EXPLICIT_DISTRUST.0),
    ("TRUST_E_SUBJECT_NOT_TRUSTED", TRUST_E_SUBJECT_NOT_TRUSTED.0),
    ("CERT_E_UNTRUSTEDROOT", CERT_E_UNTRUSTEDROOT.0),
    ("CERT_E_EXPIRED", CERT_E_EXPIRED.0),
    ("CERT_E_REVOKED", CERT_E_REVOKED.0),
    ("CERT_E_CHAINING", CERT_E_CHAINING.0),
];

fn classify_trust_result(result: i32, publisher: Option<String>) -> TrustOutcome {
    if result == 0 {
        return TrustOutcome::Trusted(publisher);
    }

    if result == TRUST_E_NOSIGNATURE.0 || result == TRUST_E_SUBJECT_FORM_UNKNOWN.0 {
        return TrustOutcome::NoSignature;
    }

    match TRUST_FAILURES.iter().find(|(_, code)| *code == result) {
        Some((name, _)) => TrustOutcome::Invalid((*name).to_string()),
        None => TrustOutcome::Unavailable(format!("0x{:08X}", result as u32)),
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
    if !rewind(file) {
        return None;
    }

    let first = unsafe { CryptCATAdminCalcHashFromFileHandle2(admin, file, &mut size, None, None) };
    if first.is_err() || size == 0 || size > 128 {
        return None;
    }

    let mut hash = vec![0u8; size as usize];
    if !rewind(file) {
        return None;
    }

    let second = unsafe {
        CryptCATAdminCalcHashFromFileHandle2(admin, file, &mut size, Some(hash.as_mut_ptr()), None)
    };
    if second.is_err() || size == 0 || size as usize > hash.len() {
        return None;
    }

    hash.truncate(size as usize);
    Some(hash)
}

/// Microsoft documents neither where the file pointer starts nor where it ends
/// up after a hash calculation, so every calculation is told to begin at byte
/// zero. The SHA-1 fallback reuses the handle after a SHA-256 pass, which would
/// otherwise hash only whatever the first pass left unread.
fn rewind(file: HANDLE) -> bool {
    unsafe { SetFilePointerEx(file, 0, None, FILE_BEGIN).is_ok() }
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

    // GetWindowTextW takes the caption from the window manager rather than
    // sending WM_GETTEXT to the owning thread, so a hung process can neither
    // stall the snapshot nor write into this buffer after the call returned.
    let length = unsafe { GetWindowTextW(window, &mut buffer) };
    if length <= 0 {
        return None;
    }

    let end = (length as usize).min(buffer.len());
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

    fn notepad_path() -> PathBuf {
        PathBuf::from(std::env::var("SystemRoot").expect("SystemRoot"))
            .join("System32")
            .join("notepad.exe")
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
    fn core_windows_binaries_are_never_selectable() {
        let mut sampler = sampler();
        let snapshot = sampler.snapshot();
        let mut found = 0;

        for process in &snapshot {
            let name = process.name.to_ascii_lowercase();
            if ["csrss.exe", "lsass.exe", "services.exe", "wininit.exe"].contains(&name.as_str()) {
                found += 1;
                assert!(
                    !process.selectable,
                    "pid {} ({})",
                    process.pid, process.name
                );
            }
        }

        assert!(found > 0, "expected core Windows binaries in the snapshot");
    }

    #[test]
    fn processes_without_an_exe_path_are_not_selectable() {
        let mut sampler = sampler();

        for process in sampler.snapshot() {
            if process.pid <= 4 || process.exe_path.is_some() {
                continue;
            }

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
        let path = notepad_path();
        assert!(path.is_file(), "{} is missing", path.display());

        assert!(
            matches!(find_catalog(&path), CatalogLookup::Found(_)),
            "notepad.exe should have a catalog entry"
        );

        let info = verify_signature(&path);
        assert_eq!(info.status, SignatureStatus::Trusted, "{}", path.display());
        assert!(info.publisher.is_some());
    }

    #[test]
    fn an_expired_certificate_keeps_the_windows_reason() {
        let outcome = classify_trust_result(CERT_E_EXPIRED.0, None);

        assert!(
            matches!(&outcome, TrustOutcome::Invalid(reason) if reason == "CERT_E_EXPIRED"),
            "{outcome:?}"
        );
    }

    #[test]
    fn an_unlisted_failure_code_is_kept_as_hex() {
        let outcome = classify_trust_result(-1, None);

        assert!(
            matches!(&outcome, TrustOutcome::Unavailable(reason) if reason == "0xFFFFFFFF"),
            "{outcome:?}"
        );
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
    fn a_shared_signature_cache_serves_the_next_sampler() {
        let path = notepad_path();
        let seeded = SignatureInfo::trusted(Some("Sentinel Publisher".to_string()));
        let cache = SignatureCache::default();
        cache.insert(cache_key(&path).expect("cache key"), seeded.clone());

        let sampler = ProcessSampler::with_cache(cache.clone()).expect("process sampler");
        let info = sampler.signature_for(Some(&path));

        assert_eq!(info, seeded);
    }

    #[test]
    fn a_replaced_binary_is_verified_again() {
        let path =
            std::env::temp_dir().join(format!("easy-window-cache-{}.bin", std::process::id()));
        let cache = SignatureCache::default();
        let sampler = ProcessSampler::with_cache(cache.clone()).expect("process sampler");

        std::fs::write(&path, b"first").expect("write the first file");
        sampler.signature_for(Some(&path));
        std::fs::write(&path, b"a longer second file").expect("write the second file");
        sampler.signature_for(Some(&path));

        let _ = std::fs::remove_file(&path);

        assert_eq!(
            cache.len(),
            2,
            "the replaced file must not reuse the earlier result"
        );
    }

    #[test]
    fn a_file_that_cannot_be_statted_is_not_cached() {
        let cache = SignatureCache::default();
        let sampler = ProcessSampler::with_cache(cache.clone()).expect("process sampler");

        let info =
            sampler.signature_for(Some(Path::new(r"C:\EasyWindow\does-not-exist\ghost.exe")));

        assert_eq!(info.status, SignatureStatus::Unknown);
        assert_eq!(
            cache.len(),
            0,
            "a result with nothing to key it on is transient"
        );
    }

    #[test]
    fn signature_cache_reuses_results() {
        let sampler = sampler();
        let path = PathBuf::from(std::env::var("SystemRoot").expect("SystemRoot"))
            .join("System32")
            .join("kernel32.dll");
        assert!(path.is_file(), "{} is missing", path.display());

        let first = sampler.signature_for(Some(&path));
        let second = sampler.signature_for(Some(&path));

        assert_eq!(first, second);
        assert_eq!(sampler.signatures.len(), 1);
    }

    #[test]
    fn cpu_usage_is_reported_as_a_share_of_the_whole_machine() {
        // sysinfo reports 100% per saturated logical core, so a process using one
        // of eight cores arrives as 100.0. Task Manager shows the same process at
        // 12.5%, and that is the number the user will compare against.
        assert_eq!(cpu_percentage(800.0, 8), Some(100.0));
        assert_eq!(cpu_percentage(100.0, 8), Some(12.5));
        assert_eq!(cpu_percentage(0.0, 8), Some(0.0));
    }

    #[test]
    fn a_machine_without_a_known_core_count_has_no_cpu_percentage() {
        assert_eq!(cpu_percentage(100.0, 0), None);
    }

    #[test]
    fn disk_bytes_between_ticks_become_a_rate() {
        assert_eq!(
            bytes_per_sec(2048, Duration::from_millis(500)),
            Some(4096.0)
        );
        assert_eq!(bytes_per_sec(0, Duration::from_millis(500)), Some(0.0));
    }

    #[test]
    fn a_rate_needs_time_to_have_passed() {
        // Dividing by a zero-length window is how a snapshot invents infinity.
        assert_eq!(bytes_per_sec(2048, Duration::ZERO), None);
    }

    #[test]
    fn the_first_tick_has_no_cpu_or_disk_samples() {
        let mut sampler = sampler();

        for process in sampler.snapshot() {
            assert!(
                process.cpu_percent.is_none()
                    && process.disk_read_bytes_per_sec.is_none()
                    && process.disk_write_bytes_per_sec.is_none(),
                "pid {} ({}) was sampled before anything was sampled",
                process.pid,
                process.name
            );
        }
    }

    #[test]
    fn a_later_tick_samples_cpu_and_disk_for_this_process() {
        let mut sampler = sampler();
        sampler.snapshot();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL * 2);
        let own_pid = std::process::id();

        let myself = sampler
            .snapshot()
            .into_iter()
            .find(|process| process.pid == own_pid)
            .expect("this test process is in its own snapshot");

        assert!(
            myself.cpu_percent.is_some(),
            "a second tick must report a number, even a zero"
        );
        assert!(myself.disk_read_bytes_per_sec.is_some());
        assert!(myself.disk_write_bytes_per_sec.is_some());
        assert!(myself.cpu_percent.unwrap_or_default() >= 0.0);
    }

    #[test]
    fn the_machine_cpu_has_no_figure_before_its_second_window() {
        let mut sampler = sampler();

        assert_eq!(
            sampler.system_cpu_percent(),
            None,
            "the first refresh has no previous idle/busy counters to compare"
        );
    }

    #[test]
    fn the_machine_cpu_has_a_figure_once_two_windows_are_sampled() {
        let mut sampler = sampler();
        sampler.system_cpu_percent();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL * 2);

        let Some(cpu) = sampler.system_cpu_percent() else {
            panic!("two sampled windows must give the machine a CPU figure");
        };

        assert!(
            (0.0..=100.0).contains(&cpu),
            "machine CPU must be a share of the whole, got {cpu}"
        );
    }
}
