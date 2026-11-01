//! Conservative critical-process denylist.
//!
//! Windows can mark a process as critical through `IsProcessCritical`, but that
//! flag does not cover every process a user must never terminate. This module
//! adds a name-plus-path rule set: a binary called `csrss.exe` in a temp folder
//! is not protected by accident, while the real one in `System32` is.
//!
//! A process whose executable path cannot be read is blocked too. The app's
//! identity contract is path plus signature; half an identity is not evidence
//! that a close is safe.
//!
//! The module is deliberately pure. Callers pass the trusted system directories,
//! so the rules can be tested without querying the live machine.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Why a process must not be offered as a close candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CriticalReason {
    SystemIdle,
    SystemProcess,
    WindowsCritical,
    ProtectedSystemBinary,
    SessionZeroService,
    /// The executable path is unavailable, so neither the publisher nor the
    /// session can be checked. Most often this is a Windows-protected process
    /// that refuses the query.
    IdentityUnavailable,
}

impl CriticalReason {
    /// Short, human-readable explanation for the UI and audit log.
    pub const fn label(self) -> &'static str {
        match self {
            Self::SystemIdle => "System Idle Process",
            Self::SystemProcess => "Windows System process",
            Self::WindowsCritical => "Marked critical by Windows",
            Self::ProtectedSystemBinary => "Protected Windows binary",
            Self::SessionZeroService => "Service running in session 0",
            Self::IdentityUnavailable => "Executable path could not be verified",
        }
    }
}

/// The facts the denylist needs. Everything is read-only and already gathered
/// by the monitor, so this module never opens a process or touches the disk.
#[derive(Debug, Clone, Copy)]
pub struct ProcessFacts<'a> {
    pub pid: u32,
    pub name: &'a str,
    pub exe_path: Option<&'a Path>,
    pub session_id: Option<u32>,
    pub windows_critical: Option<bool>,
}

/// Core Windows executables that must never be terminated when they run from a
/// protected system directory. The path check is what makes the name safe to
/// use: a process can call itself `svchost.exe`, but it cannot move itself into
/// `System32` without administrator rights.
const PROTECTED_SYSTEM_BINARIES: &[&str] = &[
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "lsaiso.exe",
    "svchost.exe",
    "fontdrvhost.exe",
    "dwm.exe",
    "audiodg.exe",
];

/// Returns the first blocking reason, or `None` when the process is selectable.
///
/// Order matters: the most specific reason wins. For example, `csrss.exe` is
/// reported as a protected system binary rather than as a session-0 service.
pub fn assess(facts: &ProcessFacts<'_>, system_dirs: &[PathBuf]) -> Option<CriticalReason> {
    if facts.pid == 0 {
        return Some(CriticalReason::SystemIdle);
    }

    if facts.pid == 4 {
        return Some(CriticalReason::SystemProcess);
    }

    if facts.windows_critical == Some(true) {
        return Some(CriticalReason::WindowsCritical);
    }

    if is_protected_system_binary(facts.name, facts.exe_path, system_dirs) {
        return Some(CriticalReason::ProtectedSystemBinary);
    }

    if facts.session_id == Some(0) {
        return Some(CriticalReason::SessionZeroService);
    }

    // Identity is incomplete. The name alone is never a reason to protect a
    // process, but it is also never enough evidence to close one.
    if facts.exe_path.is_none() {
        return Some(CriticalReason::IdentityUnavailable);
    }

    None
}

/// True only when both the executable name and its directory match. A name
/// match alone is never enough to protect a process.
pub fn is_protected_system_binary(
    name: &str,
    exe_path: Option<&Path>,
    system_dirs: &[PathBuf],
) -> bool {
    let Some(exe_path) = exe_path else {
        return false;
    };

    let name = name.to_ascii_lowercase();
    if !PROTECTED_SYSTEM_BINARIES.contains(&name.as_str()) {
        return false;
    }

    system_dirs
        .iter()
        .any(|directory| path_is_under(exe_path, directory))
}

fn path_is_under(path: &Path, directory: &Path) -> bool {
    let path = normalize_path(path);
    let directory = normalize_path(directory);

    if path == directory {
        return true;
    }

    path.strip_prefix(&directory)
        .is_some_and(|rest| rest.starts_with('\\'))
}

/// Normalizes a Windows path for comparison: removes the verbatim prefix,
/// resolves `.` and `..` lexically, unifies separators, and folds case.
///
/// This is not a filesystem canonicalizer. It deliberately avoids touching the
/// disk so a hostile path cannot cause I/O during classification. The close
/// action in Task 9 re-verifies the real file identity before acting.
fn normalize_path(path: &Path) -> String {
    let mut raw = path.to_string_lossy().replace('/', "\\");

    if let Some(stripped) = raw.strip_prefix(r"\\?\") {
        raw = stripped.to_string();
    }

    let mut parts: Vec<&str> = Vec::new();
    for part in raw.split('\\') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }

    parts.join("\\").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system_dirs() -> Vec<PathBuf> {
        vec![
            PathBuf::from(r"C:\Windows\System32"),
            PathBuf::from(r"C:\Windows\SysWOW64"),
        ]
    }

    fn facts<'a>(
        pid: u32,
        name: &'a str,
        exe_path: Option<&'a Path>,
        session_id: Option<u32>,
        windows_critical: Option<bool>,
    ) -> ProcessFacts<'a> {
        ProcessFacts {
            pid,
            name,
            exe_path,
            session_id,
            windows_critical,
        }
    }

    #[test]
    fn pid_zero_is_the_idle_process() {
        let facts = facts(0, "System Idle Process", None, None, None);

        assert_eq!(
            assess(&facts, &system_dirs()),
            Some(CriticalReason::SystemIdle)
        );
    }

    #[test]
    fn pid_four_is_the_windows_system_process() {
        let facts = facts(4, "System", None, Some(0), None);

        assert_eq!(
            assess(&facts, &system_dirs()),
            Some(CriticalReason::SystemProcess)
        );
    }

    #[test]
    fn windows_critical_flag_blocks_the_process() {
        let path = PathBuf::from(r"C:\Apps\worker.exe");
        let facts = facts(1234, "worker.exe", Some(&path), Some(1), Some(true));

        assert_eq!(
            assess(&facts, &system_dirs()),
            Some(CriticalReason::WindowsCritical)
        );
    }

    #[test]
    fn core_binary_in_system32_is_protected() {
        let path = PathBuf::from(r"C:\Windows\System32\csrss.exe");
        let facts = facts(600, "csrss.exe", Some(&path), Some(1), None);

        assert_eq!(
            assess(&facts, &system_dirs()),
            Some(CriticalReason::ProtectedSystemBinary)
        );
    }

    #[test]
    fn core_binary_outside_system32_is_not_protected() {
        let path = PathBuf::from(r"C:\Users\dev\AppData\Local\Temp\csrss.exe");
        let facts = facts(600, "csrss.exe", Some(&path), Some(1), None);

        assert_eq!(assess(&facts, &system_dirs()), None);
    }

    #[test]
    fn core_binary_without_a_path_is_blocked_as_unverifiable() {
        let facts = facts(600, "csrss.exe", None, Some(1), None);

        assert_eq!(
            assess(&facts, &system_dirs()),
            Some(CriticalReason::IdentityUnavailable)
        );
    }

    #[test]
    fn any_process_without_an_exe_path_is_blocked() {
        let facts = facts(5000, "wudfhost.exe", None, None, Some(false));

        assert_eq!(
            assess(&facts, &system_dirs()),
            Some(CriticalReason::IdentityUnavailable)
        );
    }

    #[test]
    fn pid_zero_and_pid_four_keep_their_specific_reasons() {
        let idle = facts(0, "System Idle Process", None, None, None);
        let system = facts(4, "System", None, None, None);

        assert_eq!(
            assess(&idle, &system_dirs()),
            Some(CriticalReason::SystemIdle)
        );
        assert_eq!(
            assess(&system, &system_dirs()),
            Some(CriticalReason::SystemProcess)
        );
    }

    #[test]
    fn syswow64_binaries_are_protected() {
        let path = PathBuf::from(r"C:\Windows\SysWOW64\svchost.exe");
        let facts = facts(700, "svchost.exe", Some(&path), Some(0), None);

        assert_eq!(
            assess(&facts, &system_dirs()),
            Some(CriticalReason::ProtectedSystemBinary)
        );
    }

    #[test]
    fn path_matching_is_case_insensitive() {
        let path = PathBuf::from(r"c:\windows\system32\CSRSS.EXE");
        let facts = facts(600, "CsRss.ExE", Some(&path), Some(1), None);

        assert_eq!(
            assess(&facts, &system_dirs()),
            Some(CriticalReason::ProtectedSystemBinary)
        );
    }

    #[test]
    fn verbatim_prefix_is_normalized() {
        let path = PathBuf::from(r"\\?\C:\Windows\System32\lsass.exe");
        let facts = facts(800, "lsass.exe", Some(&path), Some(1), None);

        assert_eq!(
            assess(&facts, &system_dirs()),
            Some(CriticalReason::ProtectedSystemBinary)
        );
    }

    #[test]
    fn dot_segments_are_normalized() {
        let path = PathBuf::from(r"C:\Windows\System32\.\..\System32\winlogon.exe");
        let facts = facts(900, "winlogon.exe", Some(&path), Some(1), None);

        assert_eq!(
            assess(&facts, &system_dirs()),
            Some(CriticalReason::ProtectedSystemBinary)
        );
    }

    #[test]
    fn sibling_directory_with_a_shared_prefix_is_not_protected() {
        let path = PathBuf::from(r"C:\Windows\System32evil\csrss.exe");
        let facts = facts(600, "csrss.exe", Some(&path), Some(1), None);

        assert_eq!(assess(&facts, &system_dirs()), None);
    }

    #[test]
    fn session_zero_is_blocked() {
        let path = PathBuf::from(r"C:\Program Files\Vendor\service.exe");
        let facts = facts(1000, "service.exe", Some(&path), Some(0), None);

        assert_eq!(
            assess(&facts, &system_dirs()),
            Some(CriticalReason::SessionZeroService)
        );
    }

    #[test]
    fn session_zero_wins_over_the_unverified_identity() {
        let facts = facts(1300, "service.exe", None, Some(0), None);

        assert_eq!(
            assess(&facts, &system_dirs()),
            Some(CriticalReason::SessionZeroService)
        );
    }

    #[test]
    fn user_session_process_is_selectable() {
        let path = PathBuf::from(r"C:\Program Files\Vendor\app.exe");
        let facts = facts(1100, "app.exe", Some(&path), Some(1), Some(false));

        assert_eq!(assess(&facts, &system_dirs()), None);
    }

    #[test]
    fn unknown_session_does_not_block() {
        let path = PathBuf::from(r"C:\Program Files\Vendor\app.exe");
        let facts = facts(1200, "app.exe", Some(&path), None, None);

        assert_eq!(assess(&facts, &system_dirs()), None);
    }
}
