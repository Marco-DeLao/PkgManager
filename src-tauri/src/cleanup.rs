use crate::models::{InstalledPackage, PackageSource};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum CleanupError {
    #[error("cleanup was not explicitly confirmed")]
    NotConfirmed,
    #[error("cleanup is blocked: {0}")]
    Unsafe(String),
    #[error("cleanup target does not exist: {0}")]
    MissingTarget(String),
    #[error("could not determine audit log path")]
    AuditPath,
    #[error("audit log error: {0}")]
    Audit(#[from] std::io::Error),
    #[error("cleanup command failed: {0}")]
    Command(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPreview {
    pub package_name: String,
    pub source: PackageSource,
    pub install_path: String,
    pub size_bytes: u64,
    pub command: String,
    pub requires_privilege: bool,
    pub safe_to_suggest: bool,
    pub safety_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupResult {
    pub package_name: String,
    pub install_path: String,
    pub freed_bytes: u64,
    pub audit_log: String,
}

pub fn preview(package: &InstalledPackage) -> CleanupPreview {
    let (command, requires_privilege) = match package.source {
        PackageSource::Apt => (format!("apt-get remove -y -- {}", shell_quote(&package.package_id)), true),
        PackageSource::Rpm => (format!("dnf remove -y -- {}", shell_quote(&package.package_id)), true),
        PackageSource::Pacman => (format!("pacman -R --noconfirm -- {}", shell_quote(&package.package_id)), true),
        PackageSource::Flatpak => (format!("flatpak uninstall --assumeyes -- {}", shell_quote(&package.package_id)), package.install_path.starts_with("/var/lib/flatpak")),
        PackageSource::Snap => (format!("snap remove -- {}", shell_quote(&package.package_id)), true),
        PackageSource::AppImage => (format!("rm -- {}", shell_quote(&package.install_path)), false),
    };
    let safety_reason = unsafe_reason(package);

    CleanupPreview {
        package_name: package.name.clone(),
        source: package.source,
        install_path: package.install_path.clone(),
        size_bytes: package.size_bytes,
        command,
        requires_privilege,
        safe_to_suggest: safety_reason.is_none(),
        safety_reason,
    }
}

pub fn confirmation_text(package: &InstalledPackage) -> String {
    format!("REMOVE {}", package.name)
}

pub fn execute(package: &InstalledPackage, confirmation: &str) -> Result<CleanupResult, CleanupError> {
    let _mutation_guard = acquire_mutation_lock().map_err(CleanupError::Command)?;
    let audit_path = default_audit_path()?;
    execute_with(package, confirmation, &audit_path, run_command)
}

static MUTATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) fn acquire_mutation_lock() -> Result<MutexGuard<'static, ()>, String> {
    MUTATION_LOCK.get_or_init(|| Mutex::new(())).lock().map_err(|_| "mutation lock poisoned".to_string())
}

fn execute_with<F>(
    package: &InstalledPackage,
    confirmation: &str,
    audit_path: &Path,
    runner: F,
) -> Result<CleanupResult, CleanupError>
where
    F: FnOnce(&[String]) -> Result<Output, CleanupError>,
{
    if confirmation != confirmation_text(package) {
        return Err(CleanupError::NotConfirmed);
    }

    let cleanup_preview = preview(package);
    validate_package_target(package)?;
    if let Some(reason) = cleanup_preview.safety_reason {
        return Err(CleanupError::Unsafe(reason));
    }
    let command_args = command_args(package);
    log::info!("Cleanup command constructed: {}", format_command(&command_args));
    let output = runner(&command_args)?;
    log::info!(
        "Cleanup command completed: status={} stdout={:?} stderr={:?}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        log::error!(
            "Cleanup command failed: command={} status={} stderr={:?}",
            format_command(&command_args),
            output.status,
            detail
        );
        return Err(CleanupError::Command(if detail.is_empty() {
            format!("process exited with {}", output.status)
        } else {
            detail
        }));
    }

    let log_path = append_audit_record(audit_path, "remove", package.source, &package.package_id, &package.install_path, package.size_bytes, package.size_bytes, &command_args)?;
    Ok(CleanupResult {
        package_name: package.name.clone(),
        install_path: package.install_path.clone(),
        freed_bytes: package.size_bytes,
        audit_log: log_path.display().to_string(),
    })
}

fn command_args(package: &InstalledPackage) -> Vec<String> {
    if package.source == PackageSource::AppImage {
        return vec!["/usr/bin/rm".into(), "--".into(), package.install_path.clone()];
    }
    let mut args = vec!["pkexec".to_string()];
    match package.source {
        PackageSource::Apt => args.extend(["apt-get".into(), "remove".into(), "-y".into(), "--".into(), package.package_id.clone()]),
        PackageSource::Rpm => args.extend(["dnf".into(), "remove".into(), "-y".into(), "--".into(), package.package_id.clone()]),
        PackageSource::Pacman => args.extend(["pacman".into(), "-R".into(), "--noconfirm".into(), "--".into(), package.package_id.clone()]),
        PackageSource::Flatpak => {
            if package.install_path.starts_with("/var/lib/flatpak") {
                args.extend(["flatpak".into(), "uninstall".into(), "--assumeyes".into(), "--".into(), package.package_id.clone()]);
            } else {
                args = vec!["flatpak".into(), "uninstall".into(), "--user".into(), "--assumeyes".into(), "--".into(), package.package_id.clone()];
            }
        }
        PackageSource::Snap => args.extend(["snap".into(), "remove".into(), "--".into(), package.package_id.clone()]),
        PackageSource::AppImage => unreachable!(),
    }
    args
}

pub(crate) fn run_command(args: &[String]) -> Result<Output, CleanupError> {
    let (program, command_args) = args.split_first().ok_or_else(|| CleanupError::Command("empty command".to_string()))?;
    let root_without_pkexec = program == &"pkexec" && running_as_root() && !command_available("pkexec");
    let program = if program == &"pkexec" && std::env::var_os("FLATPAK_ID").is_some() {
        "/run/host/usr/bin/pkexec"
    } else if root_without_pkexec {
        command_args.first().map(String::as_str).unwrap_or(program)
    } else {
        program
    };
    let command_args = if root_without_pkexec { &command_args[1..] } else { command_args };
    log::info!(
        "Executing cleanup process: {}",
        format_command_with_program(program, command_args)
    );
    Command::new(program)
        .args(command_args)
        .output()
        .map_err(|error| CleanupError::Command(error.to_string()))
}

fn running_as_root() -> bool {
    Command::new("id").args(["-u"]).output().ok().and_then(|output| String::from_utf8(output.stdout).ok()).map(|uid| uid.trim() == "0").unwrap_or(false)
}

fn command_available(command: &str) -> bool {
    Command::new("sh").args(["-c", "command -v -- \"$1\" >/dev/null 2>&1", "pkgclean", command]).status().map(|status| status.success()).unwrap_or(false)
}

pub(crate) fn format_command(args: &[String]) -> String {
    match args.split_first() {
        Some((program, command_args)) => format_command_with_program(program, command_args),
        None => String::new(),
    }
}

fn format_command_with_program(program: &str, args: &[String]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn default_audit_path() -> Result<PathBuf, CleanupError> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .ok_or(CleanupError::AuditPath)?;
    Ok(base.join("pkgclean/cleanup.log"))
}

pub(crate) fn append_audit_record(
    path: &Path,
    action: &str,
    source: PackageSource,
    package_id: &str,
    install_path: &str,
    size_bytes: u64,
    freed_bytes: u64,
    command: &[String],
) -> Result<PathBuf, CleanupError> {
    let parent = path.parent().ok_or(CleanupError::AuditPath)?;
    fs::create_dir_all(parent)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let command_line = command.iter().map(|part| shell_quote(part)).collect::<Vec<_>>().join(" ");
    writeln!(
        file,
        "timestamp={} action={} source={} package={} path={} size_bytes={} freed_bytes={} command={}",
        timestamp,
        action,
        source,
        shell_quote(package_id),
        shell_quote(install_path),
        size_bytes,
        freed_bytes,
        command_line
    )?;
    Ok(path.to_path_buf())
}

pub(crate) fn append_audit_record_details(
    path: &Path,
    action: &str,
    source: PackageSource,
    package_id: &str,
    command: &str,
    status: &str,
    detail: &str,
    stdout: &str,
    stderr: &str,
) -> Result<PathBuf, CleanupError> {
    let parent = path.parent().ok_or(CleanupError::AuditPath)?;
    fs::create_dir_all(parent)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    writeln!(file, "timestamp={} action={} source={} package={} status={} detail={} command={} stdout={} stderr={}", timestamp, action, source, shell_quote(package_id), shell_quote(status), shell_quote(detail), shell_quote(command), shell_quote(stdout), shell_quote(stderr))?;
    Ok(path.to_path_buf())
}

fn unsafe_reason(package: &InstalledPackage) -> Option<String> {
    let normalized = package.name.to_ascii_lowercase();
    let protected_names = [
        "base", "bash", "coreutils", "glibc", "linux", "linux-headers",
        "systemd", "init", "kernel", "sudo", "util-linux",
    ];
    if protected_names.iter().any(|name| normalized == *name || normalized.starts_with(&format!("{}-", name))) {
        return Some("Core system, kernel, or service package; removal is not auto-suggested.".to_string());
    }
    None
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t").replace('\'', "'\\''"))
}

fn validate_package_target(package: &InstalledPackage) -> Result<(), CleanupError> {
    if package.package_id.is_empty() || !package.package_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._+:-".contains(&byte)) {
        return Err(CleanupError::Unsafe("package identifier contains invalid characters".into()));
    }
    let valid_path = match package.source {
        PackageSource::Pacman => package.install_path.starts_with("/var/lib/pacman/local/") && Path::new(&package.install_path).is_dir() && Path::new(&package.install_path).file_name().and_then(|name| name.to_str()).map(|name| name == package.package_id || name.starts_with(&format!("{}-", package.package_id))).unwrap_or(false),
        PackageSource::Flatpak => (package.install_path.starts_with("/var/lib/flatpak/") || package.install_path.starts_with("/home/")) && Path::new(&package.install_path).is_dir() && Path::new(&package.install_path).file_name().and_then(|name| name.to_str()).map(|name| name == package.package_id).unwrap_or(false),
        PackageSource::Snap => package.install_path.starts_with("/var/lib/snapd/") && Path::new(&package.install_path).exists(),
        PackageSource::Apt | PackageSource::Rpm => true,
        PackageSource::AppImage => package.install_path.to_ascii_lowercase().ends_with(".appimage") && Path::new(&package.install_path).is_file(),
    };
    if valid_path { Ok(()) } else { Err(CleanupError::Unsafe("package target path is outside the known source location".into())) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn package(source: PackageSource, name: &str, path: &str) -> InstalledPackage {
        InstalledPackage {
            name: name.to_string(),
            package_id: name.to_string(),
            source,
            version: "1.0".to_string(),
            size_bytes: 42,
            install_path: path.to_string(),
        }
    }

    #[test]
    fn preview_quotes_package_ids_and_requires_privilege() {
        let result = preview(&package(PackageSource::Pacman, "demo package", "/var/lib/pacman/local/demo"));
        assert_eq!(result.command, "pacman -R --noconfirm -- 'demo package'");
        assert!(result.requires_privilege);
        assert!(result.safe_to_suggest);
    }

    #[test]
    fn appimage_cleanup_does_not_use_pkexec() {
        let args = command_args(&package(PackageSource::AppImage, "demo", "/tmp/demo.AppImage"));
        assert_eq!(args, ["/usr/bin/rm", "--", "/tmp/demo.AppImage"]);
    }

    #[test]
    fn malformed_package_target_is_rejected_before_runner() {
        let package = package(PackageSource::Pacman, "demo; touch /tmp/pwned", "/var/lib/pacman/local/demo");
        let result = execute_with(&package, &confirmation_text(&package), Path::new("/tmp/pkgclean-security.log"), |_| {
            panic!("runner must not execute for malformed package identifiers");
        });
        assert!(matches!(result, Err(CleanupError::Unsafe(_))));
    }

    #[test]
    fn core_packages_are_not_safe_to_suggest() {
        let result = preview(&package(PackageSource::Apt, "linux-image", "/usr/lib/linux"));
        assert!(!result.safe_to_suggest);
        assert!(result.safety_reason.is_some());
    }

    #[test]
    fn confirmed_fixture_cleanup_executes_and_writes_audit_log() {
        let root = std::env::temp_dir().join(format!("pkgclean-cleanup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let target = root.join("demo.AppImage");
        fs::write(&target, b"fixture").unwrap();
        let audit = root.join("state/cleanup.log");
        let observed = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured = Arc::clone(&observed);
        let package = package(PackageSource::AppImage, "demo", target.to_str().unwrap());
        let target_for_runner = package.install_path.clone();

        let result = execute_with(&package, &confirmation_text(&package), &audit, move |args| {
            captured.lock().unwrap().extend(args.iter().cloned());
            let status = std::process::Command::new("/usr/bin/rm").args(["--", &target_for_runner]).status().unwrap();
            Ok(Output { status, stdout: Vec::new(), stderr: Vec::new() })
        }).unwrap();

        assert!(!target.exists());
        assert_eq!(result.freed_bytes, 42);
        let audit_content = std::fs::read_to_string(&audit).unwrap();
        assert!(audit_content.contains("package='demo'"));
        assert!(audit_content.contains("size_bytes=42 freed_bytes=42"));
        assert_eq!(observed.lock().unwrap()[0], "/usr/bin/rm");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn wrong_confirmation_never_runs_command() {
        let package = package(PackageSource::AppImage, "demo", "/tmp/demo.AppImage");
        let ran = Arc::new(Mutex::new(false));
        let marker = Arc::clone(&ran);
        let result = execute_with(&package, "REMOVE something else", Path::new("/tmp/pkgclean-test.log"), move |_| {
            *marker.lock().unwrap() = true;
            Err(CleanupError::Command("should not run".to_string()))
        });
        assert!(matches!(result, Err(CleanupError::NotConfirmed)));
        assert!(!*ran.lock().unwrap());
    }

    #[test]
    fn actual_pkexec_fixture_cleanup_and_audit_work() {
        let root = std::env::temp_dir().join(format!("pkgclean-pkexec-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let target = root.join("actual.AppImage");
        let audit = root.join("state/cleanup.log");
        fs::write(&target, b"fixture").unwrap();
        let package = package(PackageSource::AppImage, "actual", target.to_str().unwrap());

        let result = execute_with(&package, &confirmation_text(&package), &audit, run_command);

        assert!(result.is_ok(), "pkexec fixture cleanup failed: {:?}", result.err());
        assert!(!target.exists());
        let audit_content = fs::read_to_string(&audit).unwrap();
        assert!(audit_content.contains("source=AppImage"));
        assert!(audit_content.contains("size_bytes=42 freed_bytes=42"));
        let _ = fs::remove_dir_all(root);
    }

}
