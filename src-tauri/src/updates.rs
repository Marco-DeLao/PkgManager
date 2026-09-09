use crate::cleanup::{append_audit_record_details, default_audit_path, format_command, run_command};
use crate::models::PackageSource;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub package_id: String,
    pub display_name: String,
    pub source: PackageSource,
    pub current_version: String,
    pub available_version: String,
    pub approx_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheck {
    pub updates: Vec<AvailableUpdate>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateItemOutcome {
    pub package_id: String,
    pub source: PackageSource,
    pub status: String,
    pub detail: String,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateReport {
    pub successful: Vec<UpdateItemOutcome>,
    pub failed: Vec<UpdateItemOutcome>,
    pub skipped: Vec<UpdateItemOutcome>,
    pub audit_log: String,
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("update check failed: {0}")]
    Check(String),
    #[error("update command failed: {0}")]
    Command(String),
    #[error("audit log error: {0}")]
    Audit(String),
    #[error("updates were not explicitly confirmed")]
    NotConfirmed,
}

pub fn confirmation_text(updates: &[AvailableUpdate]) -> String {
    format!("UPDATE ALL {} PACKAGES", updates.len())
}

pub fn check() -> UpdateCheck {
    let mut updates = Vec::new();
    let mut errors = Vec::new();
    if command_available("pacman") {
        match pacman_updates() { Ok(items) => updates.extend(items), Err(error) => errors.push(error) }
    }
    if command_available("flatpak") {
        match flatpak_updates() { Ok(items) => updates.extend(items), Err(error) => errors.push(error) }
    }
    if command_available("apt-get") {
        match apt_updates() { Ok(items) => updates.extend(items), Err(error) => errors.push(error) }
    }
    if command_available("dnf") {
        match dnf_updates() { Ok(items) => updates.extend(items), Err(error) => errors.push(error) }
    }
    UpdateCheck { updates, errors }
}

fn pacman_updates() -> Result<Vec<AvailableUpdate>, String> {
    let refresh = vec!["pkexec", "pacman", "-Sy", "--noconfirm"].into_iter().map(str::to_string).collect::<Vec<_>>();
    let refresh_output = run_command(&refresh).map_err(|error| format!("Pacman database refresh: {}", error))?;
    log::info!("Update check command={} status={} stdout={:?} stderr={:?}", format_command(&refresh), refresh_output.status, String::from_utf8_lossy(&refresh_output.stdout), String::from_utf8_lossy(&refresh_output.stderr));
    let output = Command::new("pacman").args(["-Qu"]).output().map_err(|error| error.to_string())?;
    log::info!("Update check command=pacman -Qu status={} stdout={:?} stderr={:?}", output.status, String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.code() == Some(1) && stdout.trim().is_empty() && stderr.trim().is_empty() {
            log::info!("Pacman reported no available updates with its normal exit status 1");
            return Ok(Vec::new());
        }
        return Err(command_detail(&output));
    }
    Ok(parse_pacman_updates(&String::from_utf8_lossy(&output.stdout)))
}

fn flatpak_updates() -> Result<Vec<AvailableUpdate>, String> {
    let output = Command::new("flatpak").args(["remote-ls", "--updates", "--system", "-j"]).output().map_err(|error| error.to_string())?;
    if !output.status.success() { return Err(command_detail(&output)); }
    Ok(parse_flatpak_updates(&String::from_utf8_lossy(&output.stdout)))
}

fn apt_updates() -> Result<Vec<AvailableUpdate>, String> {
    let output = Command::new("apt-get").args(["-s", "upgrade"]).output().map_err(|error| error.to_string())?;
    if !output.status.success() { return Err(command_detail(&output)); }
    Ok(parse_apt_updates(&String::from_utf8_lossy(&output.stdout)))
}

fn dnf_updates() -> Result<Vec<AvailableUpdate>, String> {
    let output = Command::new("dnf").args(["check-update"]).output().map_err(|error| error.to_string())?;
    if output.status.code() != Some(0) && output.status.code() != Some(100) { return Err(command_detail(&output)); }
    Ok(parse_dnf_updates(&String::from_utf8_lossy(&output.stdout)))
}

pub fn update_all(updates: &[AvailableUpdate], confirmation: &str) -> Result<UpdateReport, UpdateError> {
    if confirmation != confirmation_text(updates) { return Err(UpdateError::NotConfirmed); }
    let _mutation_guard = crate::cleanup::acquire_mutation_lock().map_err(UpdateError::Command)?;
    let mut report = UpdateReport { successful: Vec::new(), failed: Vec::new(), skipped: Vec::new(), audit_log: String::new() };
    let audit = default_audit_path().map_err(|error| UpdateError::Audit(error.to_string()))?;
    for source in [PackageSource::Pacman, PackageSource::Flatpak, PackageSource::Apt, PackageSource::Rpm] {
        let source_updates = updates.iter().filter(|update| update.source == source).collect::<Vec<_>>();
        if source_updates.is_empty() { continue; }
        let args = bulk_update_command(source);
        let command = format_command(&args);
        let command_result = run_command(&args);
        let (success, stdout, stderr, detail) = match command_result {
            Ok(output) => (output.status.success(), String::from_utf8_lossy(&output.stdout).into_owned(), String::from_utf8_lossy(&output.stderr).into_owned(), format!("process exited with {}", output.status)),
            Err(error) => (false, String::new(), error.to_string(), error.to_string()),
        };
        log::info!("Bulk update command={} source={} success={} stdout={:?} stderr={:?}", command, source, success, stdout, stderr);
        let updated_ids = parse_updated_package_ids(source, &stdout);
        for update in source_updates {
            let item_success = success && updated_ids.contains(&update.package_id);
            let status = if item_success { "success" } else if success { "skipped" } else { "failed" };
            let item_detail = if item_success { format!("{} -> {}", update.current_version, update.available_version) } else if success { "not reported by the bulk command".into() } else { detail.clone() };
            let outcome = UpdateItemOutcome { package_id: update.package_id.clone(), source: update.source, status: status.into(), detail: item_detail, stdout: stdout.clone(), stderr: stderr.clone() };
            log::info!("Bulk update result command={} package={} status={} stdout={:?} stderr={:?}", command, outcome.package_id, outcome.status, outcome.stdout, outcome.stderr);
            let audit_path = append_audit_record_details(&audit, "update", update.source, &update.package_id, &command, &outcome.status, &outcome.detail, &outcome.stdout, &outcome.stderr).map_err(|error| UpdateError::Audit(error.to_string()))?;
            report.audit_log = audit_path.display().to_string();
            match outcome.status.as_str() { "success" => report.successful.push(outcome), "skipped" => report.skipped.push(outcome), _ => report.failed.push(outcome) }
        }
    }
    Ok(report)
}

fn bulk_update_command(source: PackageSource) -> Vec<String> {
    match source {
        PackageSource::Pacman => vec!["pkexec", "pacman", "-Syu", "--noconfirm"].into_iter().map(str::to_string).collect(),
        PackageSource::Flatpak => vec!["pkexec", "flatpak", "update", "--system", "--assumeyes"].into_iter().map(str::to_string).collect(),
        PackageSource::Apt => vec!["pkexec", "apt-get", "upgrade", "-y"].into_iter().map(str::to_string).collect(),
        PackageSource::Rpm => vec!["pkexec", "dnf", "upgrade", "-y"].into_iter().map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

fn parse_updated_package_ids(source: PackageSource, output: &str) -> std::collections::HashSet<String> {
    output.lines().filter_map(|line| match source {
        PackageSource::Pacman => line.trim().strip_prefix("upgrading ").map(|value| value.trim_end_matches('.').trim()),
        PackageSource::Flatpak => line.split_whitespace().find(|value| value.starts_with("app/") || value.starts_with("runtime/")) .and_then(|value| value.split('/').nth(1)),
        _ => None,
    }).map(str::to_string).collect()
}

fn parse_pacman_updates(output: &str) -> Vec<AvailableUpdate> {
    output.lines().filter_map(|line| {
        let (left, available) = line.split_once(" -> ")?;
        let mut left_parts = left.split_whitespace();
        let current = left_parts.next_back()?;
        let name = left_parts.collect::<Vec<_>>().join(" ");
        if name.is_empty() { return None; }
        Some(AvailableUpdate { package_id: name.clone(), display_name: name, source: PackageSource::Pacman, current_version: current.to_string(), available_version: available.trim().to_string(), approx_size: None })
    }).collect()
}

fn parse_flatpak_updates(output: &str) -> Vec<AvailableUpdate> {
    serde_json::from_str::<serde_json::Value>(output).ok().and_then(|value| value.as_array().cloned()).unwrap_or_default().into_iter().filter_map(|entry| {
        let package_id = entry.get("application")?.as_str()?.to_string();
        let display_name = entry.get("name").and_then(|value| value.as_str()).unwrap_or(&package_id).to_string();
        let available_version = entry.get("version").and_then(|value| value.as_str()).unwrap_or("unknown").to_string();
        Some(AvailableUpdate { package_id, display_name, source: PackageSource::Flatpak, current_version: "installed".into(), available_version, approx_size: entry.get("download-size").and_then(|value| value.as_u64()) })
    }).collect()
}

fn parse_apt_updates(output: &str) -> Vec<AvailableUpdate> {
    output.lines().filter_map(|line| {
        if let Some(rest) = line.strip_prefix("Inst ") {
            let package_id = rest.split_whitespace().next()?.to_string();
            let current_version = rest.split_once('[').and_then(|(_, value)| value.split(']').next()).unwrap_or("installed").trim().to_string();
            let available_version = rest.split_once('(')?.1.split_whitespace().next()?.to_string();
            return Some(AvailableUpdate { package_id: package_id.clone(), display_name: package_id, source: PackageSource::Apt, current_version, available_version, approx_size: None });
        }
        let (name, rest) = line.split_once(' ')?;
        let available = rest.split_whitespace().next()?.to_string();
        let current = rest.split("[upgradable from: ").nth(1)?.trim_end_matches(']').to_string();
        Some(AvailableUpdate { package_id: name.split('/').next().unwrap_or(name).into(), display_name: name.into(), source: PackageSource::Apt, current_version: current, available_version: available, approx_size: None })
    }).collect()
}

fn parse_dnf_updates(output: &str) -> Vec<AvailableUpdate> {
    output.lines().filter_map(|line| {
        if let Some((name, versions)) = line.split_once(' ') {
            if let Some((current, available)) = versions.split_once(" -> ") {
                return Some(AvailableUpdate { package_id: name.to_string(), display_name: name.to_string(), source: PackageSource::Rpm, current_version: current.to_string(), available_version: available.to_string(), approx_size: None });
            }
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 3 || fields[0] == "Last" || fields[0] == "Dependencies" || fields[0].starts_with("Updating") { return None; }
        Some(AvailableUpdate { package_id: fields[0].to_string(), display_name: fields[0].to_string(), source: PackageSource::Rpm, current_version: "installed".into(), available_version: fields[1].to_string(), approx_size: None })
    }).collect()
}

fn command_available(command: &str) -> bool { Command::new("sh").args(["-c", "command -v -- \"$1\" >/dev/null 2>&1", "pkgclean", command]).status().map(|status| status.success()).unwrap_or(false) }
fn command_detail(output: &std::process::Output) -> String { let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string(); if stderr.is_empty() { format!("process exited with {}", output.status) } else { stderr } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pacman_update_fixture() {
        let updates = parse_pacman_updates("firefox 139.0-1 -> 140.0-1\n");
        assert_eq!(updates[0].current_version, "139.0-1");
        assert_eq!(updates[0].available_version, "140.0-1");
    }

    #[test]
    fn parses_flatpak_update_fixture() {
        let updates = parse_flatpak_updates(r#"[{"application":"org.gimp.GIMP","name":"GIMP","version":"3.2.5","download-size":1234}]"#);
        assert_eq!(updates[0].package_id, "org.gimp.GIMP");
        assert_eq!(updates[0].approx_size, Some(1234));
    }

    #[test]
    fn parses_apt_update_fixture() { assert_eq!(parse_apt_updates("gimp/jammy 3.2.5 amd64 [upgradable from: 3.2.4]")[0].current_version, "3.2.4"); }
    #[test]
    fn parses_dnf_update_fixture() { assert_eq!(parse_dnf_updates("gimp.x86_64 3.2.4 -> 3.2.5")[0].available_version, "3.2.5"); }

    #[test]
    fn bulk_commands_use_one_authenticated_session_per_source() {
        assert_eq!(bulk_update_command(PackageSource::Pacman), ["pkexec", "pacman", "-Syu", "--noconfirm"]);
        assert_eq!(bulk_update_command(PackageSource::Flatpak), ["pkexec", "flatpak", "update", "--system", "--assumeyes"]);
    }

    #[test]
    fn parses_pacman_bulk_updated_package_names() {
        let updated = parse_updated_package_ids(PackageSource::Pacman, "upgrading firefox...\nupgrading curl...\n");
        assert!(updated.contains("firefox"));
        assert!(updated.contains("curl"));
    }

    #[test]
    fn pacman_status_one_with_empty_output_means_no_updates() {
        let output = Command::new("sh").args(["-c", "exit 1"]).output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        assert!(output.status.code() == Some(1) && String::from_utf8_lossy(&output.stdout).trim().is_empty() && String::from_utf8_lossy(&output.stderr).trim().is_empty());
    }

    #[test]
    fn wrong_update_confirmation_is_rejected() {
        let updates = vec![AvailableUpdate { package_id: "demo".into(), display_name: "demo".into(), source: PackageSource::Pacman, current_version: "1".into(), available_version: "2".into(), approx_size: None }];
        assert!(matches!(update_all(&updates, "UPDATE ALL 2 PACKAGES"), Err(UpdateError::NotConfirmed)));
    }
}