use crate::models::PackageSource;
use serde::{Deserialize, Serialize};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

pub mod apt_install;
pub mod dnf_install;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResult {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub source: PackageSource,
    pub package_id: String,
    pub approx_size: Option<u64>,
    pub remote: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("search query must not be empty")]
    EmptyQuery,
    #[error("package manager is unavailable: {0}")]
    Unavailable(String),
    #[error("search command failed: {0}")]
    Command(String),
}

pub trait SearchAdapter: Send + Sync {
    fn source(&self) -> PackageSource;
    fn is_available(&self) -> bool;
    fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError>;
}

pub struct PacmanSearchAdapter;
pub struct FlatpakSearchAdapter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallPreview {
    pub package_name: String,
    pub source: PackageSource,
    pub package_id: String,
    pub approx_size: Option<u64>,
    pub command: String,
    pub requires_privilege: bool,
    pub confirmation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallOutcome {
    pub package_name: String,
    pub source: PackageSource,
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub audit_log: String,
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("installation was not explicitly confirmed")]
    NotConfirmed,
    #[error("selected package identifier is invalid")]
    InvalidIdentifier,
    #[error("selected package is no longer available from its source")]
    StaleResult,
    #[error("package source is unavailable: {0}")]
    Unavailable(String),
    #[error("installation command failed: {0}")]
    Command(String),
    #[error("audit log error: {0}")]
    Audit(String),
    #[error("installation was cancelled")]
    Cancelled,
}

static ACTIVE_INSTALL: OnceLock<Mutex<Option<Child>>> = OnceLock::new();

pub trait InstallAdapter: Send + Sync {
    fn install(&self, result: &SearchResult) -> Result<InstallOutcome, InstallError>;
}

pub struct PacmanInstallAdapter;
pub struct FlatpakInstallAdapter;

pub fn preview(result: &SearchResult) -> Result<InstallPreview, InstallError> {
    let args = command_args(result)?;
    Ok(InstallPreview {
        package_name: result.display_name.clone(),
        source: result.source,
        package_id: result.package_id.clone(),
        approx_size: result.approx_size,
        command: crate::cleanup::format_command(&args),
        requires_privilege: matches!(result.source, PackageSource::Pacman | PackageSource::Flatpak),
        confirmation: confirmation_text(result),
    })
}

pub fn confirmation_text(result: &SearchResult) -> String {
    format!("INSTALL {} FROM {}", result.package_id, result.source)
}

pub fn execute(result: &SearchResult, confirmation: &str) -> Result<InstallOutcome, InstallError> {
    let _mutation_guard = crate::cleanup::acquire_mutation_lock().map_err(InstallError::Command)?;
    let expected = confirmation_text(result);
    log::debug!("Install confirmation received=[{}] expected=[{}] received_len={} expected_len={}", confirmation, expected, confirmation.len(), expected.len());
    if confirmation != expected {
        log::warn!("Install confirmation mismatch: received=[{}] expected=[{}]", confirmation, expected);
        return Err(InstallError::NotConfirmed);
    }
    validate_result(result)?;
    match result.source {
        PackageSource::Pacman => PacmanInstallAdapter.install(result),
        PackageSource::Flatpak => FlatpakInstallAdapter.install(result),
        PackageSource::Apt => apt_install(result),
        PackageSource::Rpm => dnf_install(result),
        _ => Err(InstallError::Unavailable(result.source.to_string())),
    }
}

pub fn cancel() -> bool {
    let active = ACTIVE_INSTALL.get_or_init(|| Mutex::new(None));
    active.lock().ok().and_then(|mut child| child.as_mut().map(|process| process.kill().is_ok())).unwrap_or(false)
}

impl InstallAdapter for PacmanInstallAdapter {
    fn install(&self, result: &SearchResult) -> Result<InstallOutcome, InstallError> {
        run_install(result)
    }
}

impl InstallAdapter for FlatpakInstallAdapter {
    fn install(&self, result: &SearchResult) -> Result<InstallOutcome, InstallError> {
        if result.remote.is_none() { return Err(InstallError::Unavailable("Flatpak search result has no remote".into())); }
        run_install(result)
    }
}

fn apt_install(result: &SearchResult) -> Result<InstallOutcome, InstallError> { run_install(result) }
fn dnf_install(result: &SearchResult) -> Result<InstallOutcome, InstallError> { run_install(result) }

fn run_install(result: &SearchResult) -> Result<InstallOutcome, InstallError> {
    let args = command_args(result)?;
    let command = crate::cleanup::format_command(&args);
    log::info!("Install command constructed: {}", command);
    let output = run_cancellable_command(&args)?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    log::info!("Install command completed: command={} status={} stdout={:?} stderr={:?}", command, output.status, stdout, stderr);
    if !output.status.success() {
        let detail = if stderr.trim().is_empty() { format!("process exited with {}", output.status) } else { stderr.trim().to_string() };
        log::error!("Install command failed: command={} status={} stdout={:?} stderr={:?}", command, output.status, stdout, stderr);
        return Err(InstallError::Command(detail));
    }
    let audit = crate::cleanup::default_audit_path().map_err(|error| InstallError::Audit(error.to_string()))?;
    let audit_path = crate::cleanup::append_audit_record(&audit, "install", result.source, &result.package_id, "", result.approx_size.unwrap_or(0), 0, &args).map_err(|error| InstallError::Audit(error.to_string()))?;
    Ok(InstallOutcome { package_name: result.display_name.clone(), source: result.source, command, stdout, stderr, audit_log: audit_path.display().to_string() })
}

fn run_cancellable_command(args: &[String]) -> Result<Output, InstallError> {
    let (program, command_args) = args.split_first().ok_or_else(|| InstallError::Command("empty command".into()))?;
    let program = if program == "pkexec" && std::env::var_os("FLATPAK_ID").is_some() { "/run/host/usr/bin/pkexec" } else if program == "pkexec" && running_as_root() && !command_available("pkexec") { command_args.first().unwrap_or(program) } else { program };
    let command_args = if program != "pkexec" && args.first().map(String::as_str) == Some("pkexec") { &command_args[1..] } else { command_args };
    let child = Command::new(program).args(command_args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|error| InstallError::Command(error.to_string()))?;
    let active = ACTIVE_INSTALL.get_or_init(|| Mutex::new(None));
    *active.lock().map_err(|_| InstallError::Command("install process lock poisoned".into()))? = Some(child);
    loop {
        let finished = {
            let mut guard = active.lock().map_err(|_| InstallError::Command("install process lock poisoned".into()))?;
            guard.as_mut().map(|process| process.try_wait().map(|status| status.is_some())).transpose().map_err(|error| InstallError::Command(error.to_string()))?.unwrap_or(false)
        };
        if finished { break; }
        std::thread::sleep(Duration::from_millis(100));
    }
    let child = active.lock().map_err(|_| InstallError::Command("install process lock poisoned".into()))?.take().ok_or(InstallError::Cancelled)?;
    child.wait_with_output().map_err(|error| InstallError::Command(error.to_string()))
}

fn running_as_root() -> bool {
    Command::new("id").args(["-u"]).output().ok().and_then(|output| String::from_utf8(output.stdout).ok()).map(|uid| uid.trim() == "0").unwrap_or(false)
}

fn command_args(result: &SearchResult) -> Result<Vec<String>, InstallError> {
    validate_identifier(&result.package_id)?;
    let mut args = Vec::new();
    match result.source {
        PackageSource::Pacman => args.extend(["pkexec", "pacman", "-S", "--noconfirm", "--", &result.package_id].map(str::to_string)),
        PackageSource::Flatpak => {
            let remote = result.remote.as_ref().ok_or_else(|| InstallError::Unavailable("Flatpak search result has no remote".into()))?;
            validate_identifier(remote)?;
            args.extend(["pkexec", "flatpak", "install", "--system", "--assumeyes", remote, &result.package_id].map(str::to_string));
        }
        PackageSource::Apt => args.extend(["pkexec", "apt-get", "install", "-y", "--", &result.package_id].map(str::to_string)),
        PackageSource::Rpm => args.extend(["pkexec", "dnf", "install", "-y", "--", &result.package_id].map(str::to_string)),
        _ => return Err(InstallError::Unavailable(result.source.to_string())),
    }
    Ok(args)
}

fn validate_identifier(identifier: &str) -> Result<(), InstallError> {
    if identifier.is_empty() || identifier.len() > 256 || !identifier.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._+:-".contains(&byte)) { return Err(InstallError::InvalidIdentifier); }
    Ok(())
}

fn validate_result(result: &SearchResult) -> Result<(), InstallError> {
    command_args(result)?;
    let matches = match result.source {
        PackageSource::Pacman => PacmanSearchAdapter.search(&result.package_id).map(|items| items.into_iter().any(|item| item.package_id == result.package_id)),
        PackageSource::Flatpak => FlatpakSearchAdapter.search(&result.package_id).map(|items| items.into_iter().any(|item| item.package_id == result.package_id && item.remote == result.remote)),
        PackageSource::Apt => apt_install::AptSearchAdapter.search(&result.package_id).map(|items| items.into_iter().any(|item| item.package_id == result.package_id)),
        PackageSource::Rpm => {
            let query = [".x86_64", ".noarch", ".aarch64", ".i686"].iter().find_map(|suffix| result.package_id.strip_suffix(suffix)).unwrap_or(&result.package_id);
            dnf_install::DnfSearchAdapter.search(query).map(|items| items.into_iter().any(|item| item.package_id == result.package_id || same_unqualified_rpm_name(&item.package_id, &result.package_id)))
        },
        _ => Ok(false),
    }.map_err(|_| InstallError::StaleResult)?;
    if matches { Ok(()) } else { Err(InstallError::StaleResult) }
}

fn same_unqualified_rpm_name(left: &str, right: &str) -> bool {
    [".x86_64", ".noarch", ".aarch64", ".i686"].iter().any(|suffix| left.strip_suffix(suffix) == Some(right) || right.strip_suffix(suffix) == Some(left))
}

impl SearchAdapter for PacmanSearchAdapter {
    fn source(&self) -> PackageSource { PackageSource::Pacman }
    fn is_available(&self) -> bool { command_available("pacman") }
    fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        if query.trim().is_empty() { return Err(SearchError::EmptyQuery); }
        let mut args = vec!["-Ss".to_string()];
        args.extend(query.split_whitespace().map(str::to_string));
        let output = Command::new("pacman").args(&args).output().map_err(|e| SearchError::Command(e.to_string()))?;
        if !output.status.success() {
            return Err(SearchError::Command(command_detail(&output)));
        }
        let mut results = parse_pacman_search(&String::from_utf8_lossy(&output.stdout));
        for result in &mut results {
            result.approx_size = Command::new("pacman")
                .args(["-Si", "--", &result.package_id])
                .output()
                .ok()
                .and_then(|output| parse_metadata_size(&String::from_utf8_lossy(&output.stdout), &["Download Size"]));
        }
        Ok(results)
    }
}

impl SearchAdapter for FlatpakSearchAdapter {
    fn source(&self) -> PackageSource { PackageSource::Flatpak }
    fn is_available(&self) -> bool { command_available("flatpak") }
    fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        if query.trim().is_empty() { return Err(SearchError::EmptyQuery); }
        let output = Command::new("flatpak")
            .args(["search", "--system", "--columns=application,name,description,remotes", "-j", query])
            .output().map_err(|e| SearchError::Command(e.to_string()))?;
        if !output.status.success() { return Err(SearchError::Command(command_detail(&output))); }
        let mut results = parse_flatpak_search(&String::from_utf8_lossy(&output.stdout));
        for result in &mut results {
            let Some(remote) = result.remote.as_deref() else { continue };
            result.approx_size = Command::new("flatpak")
                .args(["remote-ls", "--system", "--columns=application,download-size,installed-size", remote])
                .output()
                .ok()
                .and_then(|output| parse_flatpak_size(&String::from_utf8_lossy(&output.stdout), &result.package_id));
        }
        Ok(results)
    }
}

pub fn search_available(query: &str) -> Vec<Result<Vec<SearchResult>, SearchError>> {
    let distro = crate::distro::DistroInfo::detect();
    let adapters: Vec<Box<dyn SearchAdapter>> = match distro.native_source() {
        Some(PackageSource::Pacman) => vec![Box::new(PacmanSearchAdapter), Box::new(FlatpakSearchAdapter)],
        Some(PackageSource::Apt) => vec![Box::new(apt_install::AptSearchAdapter), Box::new(FlatpakSearchAdapter)],
        Some(PackageSource::Rpm) => vec![Box::new(dnf_install::DnfSearchAdapter), Box::new(FlatpakSearchAdapter)],
        _ => vec![Box::new(FlatpakSearchAdapter)],
    };
    adapters.into_iter().filter(|adapter| adapter.is_available()).map(|adapter| adapter.search(query)).collect()
}

pub fn rank_search_results(mut results: Vec<SearchResult>, query: &str) -> Vec<SearchResult> {
    let query = query.trim().to_ascii_lowercase();
    results.sort_by(|left, right| {
        relevance_rank(left, &query)
            .cmp(&relevance_rank(right, &query))
            .then_with(|| left.package_id.to_ascii_lowercase().cmp(&right.package_id.to_ascii_lowercase()))
            .then_with(|| left.source.to_string().cmp(&right.source.to_string()))
    });
    results
}

fn relevance_rank(result: &SearchResult, query: &str) -> u8 {
    let name = result.name.to_ascii_lowercase();
    let package_id = result.package_id.to_ascii_lowercase();
    let display_name = result.display_name.to_ascii_lowercase();
    let description = result.description.to_ascii_lowercase();
    let name_fields = [&name, &package_id, &display_name];
    let query_tokens = query.split_whitespace().collect::<Vec<_>>();
    let name_has_all_tokens = !query_tokens.is_empty() && name_fields.iter().any(|field| query_tokens.iter().all(|token| field.contains(token)));
    if name_fields.iter().any(|field| *field == query) { 0 }
    else if name_fields.iter().any(|field| field.starts_with(query)) { 1 }
    else if name_fields.iter().any(|field| field.contains(query)) { 2 }
    else if name_has_all_tokens { 3 }
    else if name_fields.iter().any(|field| query_tokens.iter().any(|token| field.contains(token))) { 4 }
    else if description.contains(query) || query_tokens.iter().all(|token| description.contains(token)) { 5 }
    else { 6 }
}

fn command_available(command: &str) -> bool {
    Command::new("sh")
        .args(["-c", "command -v -- \"$1\" >/dev/null 2>&1", "pkgclean", command])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn command_detail(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() { format!("process exited with {}", output.status) } else { stderr }
}

fn parse_pacman_search(output: &str) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let mut lines = output.lines().peekable();
    while let Some(header) = lines.next() {
        let Some((id, _version)) = header.split_once(' ') else { continue };
        let Some((repo, name)) = id.split_once('/') else { continue };
        let description = lines.next().unwrap_or_default().trim().to_string();
        results.push(SearchResult { name: name.to_string(), display_name: name.to_string(), description, source: PackageSource::Pacman, package_id: name.to_string(), approx_size: None, remote: Some(repo.to_string()) });
    }
    results
}

pub(crate) fn parse_metadata_size(output: &str, fields: &[&str]) -> Option<u64> {
    output.lines().find_map(|line| {
        let (field, value) = line.split_once(':')?;
        if fields.iter().any(|candidate| field.trim().eq_ignore_ascii_case(candidate)) {
            parse_size_value(value.trim())
        } else {
            None
        }
    })
}

pub(crate) fn parse_size_value(value: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let number = parts.next()?.replace(',', ".").parse::<f64>().ok()?;
    let unit = parts.next().unwrap_or("B").to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "b" => 1.0,
        "k" | "kb" | "kib" => 1024.0,
        "m" | "mb" | "mib" => 1024.0 * 1024.0,
        "g" | "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" | "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((number * multiplier) as u64)
}

fn parse_flatpak_size(output: &str, package_id: &str) -> Option<u64> {
    output.lines().find_map(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.first().copied() != Some(package_id) { return None; }
        parse_size_value(&fields.get(1..3).unwrap_or_default().join(" "))
            .or_else(|| parse_size_value(&fields.get(3..5).unwrap_or_default().join(" ")))
    })
}

fn parse_flatpak_search(output: &str) -> Vec<SearchResult> {
    serde_json::from_str::<serde_json::Value>(output).ok()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| {
            let package_id = entry.get("application_id")?.as_str()?.to_string();
            let display_name = entry.get("name").and_then(|value| value.as_str()).unwrap_or(&package_id).to_string();
            let description = entry.get("description").and_then(|value| value.as_str()).unwrap_or_default().to_string();
            let remote = entry.get("remotes").and_then(|value| value.as_str()).and_then(|value| value.split(',').next()).map(|value| value.trim().to_string());
            Some(SearchResult { name: package_id.clone(), display_name, description, source: PackageSource::Flatpak, package_id, approx_size: None, remote })
        }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pacman_search_fixture() {
        let results = parse_pacman_search("extra/gimp 3.0-1\n    GNU Image Manipulation Program\n");
        assert_eq!(results[0].package_id, "gimp");
        assert_eq!(results[0].description, "GNU Image Manipulation Program");
    }

    #[test]
    fn parses_package_manager_sizes() {
        assert_eq!(parse_metadata_size("Download Size : 1.5 MiB\n", &["Download Size"]), Some(1572864));
        assert_eq!(parse_size_value("3.6 MB"), Some(3774873));
        assert_eq!(parse_flatpak_size("org.kde.kalk    3.6 MB  11.3 MB\n", "org.kde.kalk"), Some(3774873));
    }

    #[test]
    fn pacman_multiword_query_is_split_into_search_terms() {
        let query = "numpy python";
        assert_eq!(query.split_whitespace().collect::<Vec<_>>(), ["numpy", "python"]);
    }

    #[test]
    fn ranks_name_matches_ahead_of_description_matches_for_any_query() {
        let result = |name: &str, description: &str| SearchResult { name: name.into(), display_name: name.into(), description: description.into(), source: PackageSource::Pacman, package_id: name.into(), approx_size: None, remote: None };
        let ranked = rank_search_results(vec![result("libnsl", "network library"), result("numpy", "numerical Python package"), result("python-numpy", "numerical arrays"), result("unrelated", "NumPy compatibility layer")], "numpy");
        assert_eq!(ranked.into_iter().map(|item| item.package_id).collect::<Vec<_>>(), ["numpy", "python-numpy", "unrelated", "libnsl"]);
    }

    #[test]
    fn parses_flatpak_search_fixture() {
        let results = parse_flatpak_search(r#"[{"application_id":"org.gimp.GIMP","name":"GIMP","description":"Image editor","remotes":"flathub"}]"#);
        assert_eq!(results[0].remote.as_deref(), Some("flathub"));
        assert_eq!(results[0].source, PackageSource::Flatpak);
    }

    #[test]
    fn install_commands_are_non_interactive_and_explicitly_scoped() {
        let pacman = SearchResult { name: "sl".into(), display_name: "sl".into(), description: String::new(), source: PackageSource::Pacman, package_id: "sl".into(), approx_size: None, remote: None };
        let flatpak = SearchResult { name: "org.example.App".into(), display_name: "App".into(), description: String::new(), source: PackageSource::Flatpak, package_id: "org.example.App".into(), approx_size: None, remote: Some("flathub".into()) };
        assert_eq!(command_args(&pacman).unwrap(), ["pkexec", "pacman", "-S", "--noconfirm", "--", "sl"]);
        assert_eq!(command_args(&flatpak).unwrap(), ["pkexec", "flatpak", "install", "--system", "--assumeyes", "flathub", "org.example.App"]);
    }

    #[test]
    fn malformed_install_identifier_is_rejected() {
        let result = SearchResult { name: "bad".into(), display_name: "bad".into(), description: String::new(), source: PackageSource::Pacman, package_id: "sl;touch /tmp/pwned".into(), approx_size: None, remote: None };
        assert!(matches!(command_args(&result), Err(InstallError::InvalidIdentifier)));
    }

    #[test]
    fn wrong_install_confirmation_never_reaches_command_execution() {
        let result = SearchResult { name: "sl".into(), display_name: "sl".into(), description: String::new(), source: PackageSource::Pacman, package_id: "sl".into(), approx_size: None, remote: None };
        assert!(matches!(execute(&result, "INSTALL sl FROM pacman"), Err(InstallError::NotConfirmed)));
    }

    #[test]
    fn preview_returns_the_exact_backend_confirmation_text() {
        let result = SearchResult { name: "sl".into(), display_name: "sl".into(), description: String::new(), source: PackageSource::Pacman, package_id: "sl".into(), approx_size: None, remote: None };
        assert_eq!(preview(&result).unwrap().confirmation, "INSTALL sl FROM Pacman");
    }

    #[test]
    fn ranks_exact_then_prefix_then_substring_matches() {
        let result = |name: &str| SearchResult { name: name.into(), display_name: name.into(), description: String::new(), source: PackageSource::Pacman, package_id: name.into(), approx_size: None, remote: None };
        let ranked = rank_search_results(vec![result("libnsl"), result("slack"), result("sl"), result("brotli")], "sl");
        assert_eq!(ranked.into_iter().map(|item| item.package_id).collect::<Vec<_>>(), ["sl", "slack", "libnsl", "brotli"]);
    }
}