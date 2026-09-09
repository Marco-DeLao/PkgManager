use crate::cleanup::{append_audit_record_details, default_audit_path, format_command, run_command};
use crate::models::PackageSource;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrphanPackage {
    pub name: String,
    pub source: PackageSource,
    pub size_bytes: u64,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrphanCheck {
    pub pacman: Vec<OrphanPackage>,
    pub apt: Vec<OrphanPackage>,
    pub dnf: Vec<OrphanPackage>,
    pub flatpak_unused: Vec<OrphanPackage>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrphanRemovalOutcome {
    pub package_id: String,
    pub source: PackageSource,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub detail: String,
}

#[derive(Debug, thiserror::Error)]
pub enum OrphanError {
    #[error("orphan removal was not explicitly confirmed")]
    NotConfirmed,
    #[error("orphan command failed: {0}")]
    Command(String),
    #[error("audit log error: {0}")]
    Audit(String),
}

pub fn confirmation_text(packages: &[OrphanPackage]) -> String { format!("REMOVE {} ORPHANS", packages.len()) }

pub fn check() -> OrphanCheck {
    let mut result = OrphanCheck { pacman: Vec::new(), apt: Vec::new(), dnf: Vec::new(), flatpak_unused: Vec::new(), errors: Vec::new() };
    if command_available("pacman") { match pacman_orphans() { Ok(items) => result.pacman = items, Err(error) => result.errors.push(error) } }
    if command_available("apt-get") { match apt_orphans() { Ok(items) => result.apt = items, Err(error) => result.errors.push(error) } }
    if command_available("dnf") { match dnf_orphans() { Ok(items) => result.dnf = items, Err(error) => result.errors.push(error) } }
    result
}

fn parse_apt_orphans(output: &str) -> Vec<OrphanPackage> {
    output.lines().filter_map(|line| line.strip_prefix("Remv ").and_then(|value| value.split_whitespace().next())).map(|name| OrphanPackage { name: name.to_string(), source: PackageSource::Apt, size_bytes: 0, note: "Reported by apt autoremove dry-run.".into() }).collect()
}

fn parse_dnf_orphans(output: &str) -> Vec<OrphanPackage> {
    output.lines().filter_map(|line| {
        let value = line.trim().strip_prefix("Removing:").or_else(|| line.trim().strip_prefix("Remove "))?;
        value.split_whitespace().next()
    }).map(|name| OrphanPackage { name: name.to_string(), source: PackageSource::Rpm, size_bytes: 0, note: "Reported by dnf autoremove dry-run.".into() }).collect()
}

fn apt_orphans() -> Result<Vec<OrphanPackage>, String> {
    let output = Command::new("apt-get").args(["-s", "autoremove"]).output().map_err(|error| error.to_string())?;
    if !output.status.success() { return Err(command_detail(&output)); }
    Ok(parse_apt_orphans(&String::from_utf8_lossy(&output.stdout)))
}

fn dnf_orphans() -> Result<Vec<OrphanPackage>, String> {
    let output = Command::new("dnf").args(["autoremove", "--assumeno"]).output().map_err(|error| error.to_string())?;
    if !output.status.success() { return Err(command_detail(&output)); }
    Ok(parse_dnf_orphans(&String::from_utf8_lossy(&output.stdout)))
}

pub fn remove_all(packages: &[OrphanPackage], confirmation: &str) -> Result<Vec<OrphanRemovalOutcome>, OrphanError> {
    if confirmation != confirmation_text(packages) { return Err(OrphanError::NotConfirmed); }
    if packages.is_empty() { return Ok(Vec::new()); }
    let _mutation_guard = crate::cleanup::acquire_mutation_lock().map_err(OrphanError::Command)?;
    let audit = default_audit_path().map_err(|error| OrphanError::Audit(error.to_string()))?;
    let source = packages[0].source;
    if packages.iter().any(|package| package.source != source) {
        return Err(OrphanError::Command("bulk orphan removal requires one package source per command".into()));
    }
    if source != PackageSource::Pacman { return Err(OrphanError::Command(format!("unsupported orphan source {}", source))); }
    let args = pacman_remove_command(packages)?;
    let command = format_command(&args);
    let command_result = run_command(&args);
    let (success, stdout, stderr, detail) = match command_result {
        Ok(output) => (output.status.success(), String::from_utf8_lossy(&output.stdout).into_owned(), String::from_utf8_lossy(&output.stderr).into_owned(), format!("process exited with {}", output.status)),
        Err(error) => (false, String::new(), error.to_string(), error.to_string()),
    };
    let removed = parse_removed_package_ids(&stdout);
    log::info!("Bulk orphan removal command={} source={} success={} stdout={:?} stderr={:?}", command, source, success, stdout, stderr);
    packages.iter().map(|package| {
        let item_success = success && removed.contains(&package.name);
        let status = if item_success { "success" } else if success { "skipped" } else { "failed" };
        let item_detail = if item_success { "removed by bulk Pacman transaction".into() } else if success { "not reported by Pacman output".into() } else { detail.clone() };
        let outcome = OrphanRemovalOutcome { package_id: package.name.clone(), source: package.source, success: item_success, stdout: stdout.clone(), stderr: stderr.clone(), detail: item_detail };
        log::info!("Bulk orphan removal result command={} package={} status={} stdout={:?} stderr={:?}", command, outcome.package_id, status, outcome.stdout, outcome.stderr);
        append_audit_record_details(&audit, "orphan_remove", package.source, &package.name, &command, status, &outcome.detail, &outcome.stdout, &outcome.stderr).map_err(|error| OrphanError::Audit(error.to_string()))?;
        Ok(outcome)
    }).collect()
}

fn pacman_remove_command(packages: &[OrphanPackage]) -> Result<Vec<String>, OrphanError> {
    let mut args = vec!["pkexec", "pacman", "-Rns", "--noconfirm", "--"].into_iter().map(str::to_string).collect::<Vec<_>>();
    for package in packages {
        if package.name.is_empty() || !package.name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._+:-".contains(&byte)) {
            return Err(OrphanError::Command(format!("invalid orphan package name: {}", package.name)));
        }
        args.push(package.name.clone());
    }
    Ok(args)
}

fn parse_removed_package_ids(output: &str) -> std::collections::HashSet<String> {
    output.lines().filter_map(|line| line.trim().strip_prefix("removing ").map(|value| value.trim_end_matches('.').trim().to_string())).collect()
}

fn pacman_orphans() -> Result<Vec<OrphanPackage>, String> {
    let output = Command::new("pacman").args(["-Qtdq"]).output().map_err(|error| error.to_string())?;
    if !output.status.success() { return Ok(Vec::new()); }
    output.stdout.iter().map(|_| ()).count();
    String::from_utf8_lossy(&output.stdout).lines().filter_map(|name| {
        let name = name.trim();
        if name.is_empty() { return None; }
        let size_bytes = Command::new("pacman").args(["-Qi", name]).output().ok().and_then(|output| parse_pacman_size(&String::from_utf8_lossy(&output.stdout)));
        Some(OrphanPackage { name: name.to_string(), source: PackageSource::Pacman, size_bytes: size_bytes.unwrap_or(0), note: "Installed as a dependency and no longer required; remove only if you do not need it directly.".into() })
    }).collect::<Vec<_>>().pipe(Ok)
}

fn parse_pacman_size(output: &str) -> Option<u64> { output.lines().find_map(|line| { let value = line.strip_prefix("Installed Size")?.split(':').nth(1)?.trim(); let mut parts = value.split_whitespace(); let number = parts.next()?.parse::<f64>().ok()?; let unit = parts.next().unwrap_or("B"); Some((number * match unit { "KiB" => 1024.0, "MiB" => 1024.0 * 1024.0, "GiB" => 1024.0 * 1024.0 * 1024.0, _ => 1.0 }) as u64) }) }

fn command_available(command: &str) -> bool { Command::new("sh").args(["-c", "command -v -- \"$1\" >/dev/null 2>&1", "pkgclean", command]).status().map(|status| status.success()).unwrap_or(false) }

fn command_detail(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() { format!("process exited with {}", output.status) } else { stderr }
}

trait Pipe: Sized { fn pipe<T>(self, function: impl FnOnce(Self) -> T) -> T { function(self) } }
impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_pacman_size_fixture() { assert_eq!(parse_pacman_size("Installed Size  : 510.13 KiB"), Some(522373)); }
    #[test]
    fn orphan_confirmation_is_explicit() { assert_eq!(confirmation_text(&[OrphanPackage { name: "demo".into(), source: PackageSource::Pacman, size_bytes: 1, note: String::new() }]), "REMOVE 1 ORPHANS"); }
    #[test]
    fn parses_apt_autoremove_fixture() { assert_eq!(parse_apt_orphans("Remv orphan-a [1.0]\n" )[0].name, "orphan-a"); }
    #[test]
    fn parses_dnf_autoremove_fixture() { assert_eq!(parse_dnf_orphans("Remove orphan-b.x86_64 1.0\n")[0].name, "orphan-b.x86_64"); }
    #[test]
    fn builds_one_explicit_bulk_pacman_command() {
        let packages = vec![
            OrphanPackage { name: "recode".into(), source: PackageSource::Pacman, size_bytes: 1, note: String::new() },
            OrphanPackage { name: "fortune-mod".into(), source: PackageSource::Pacman, size_bytes: 1, note: String::new() },
        ];
        assert_eq!(pacman_remove_command(&packages).unwrap(), ["pkexec", "pacman", "-Rns", "--noconfirm", "--", "recode", "fortune-mod"]);
    }

    #[test]
    fn parses_each_removed_package_from_bulk_output() {
        let removed = parse_removed_package_ids("removing recode...\nremoving fortune-mod...\n");
        assert!(removed.contains("recode"));
        assert!(removed.contains("fortune-mod"));
    }

    #[test]
    fn wrong_orphan_confirmation_is_rejected() {
        let packages = vec![OrphanPackage { name: "demo".into(), source: PackageSource::Pacman, size_bytes: 1, note: String::new() }];
        assert!(matches!(remove_all(&packages, "REMOVE 2 ORPHANS"), Err(OrphanError::NotConfirmed)));
    }
}