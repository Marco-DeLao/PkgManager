use crate::models::{InstalledPackage, PackageSource, ScanError};
use super::PackageAdapter;
use std::path::Path;
use std::fs;
use std::process::Command;

pub struct FlatpakAdapter;

impl PackageAdapter for FlatpakAdapter {
    fn scan(&self, _scan_path: &Path) -> Result<Vec<InstalledPackage>, ScanError> {
        if !self.is_available() {
            return Ok(Vec::new());
        }

        // Try to read system Flatpak installations
        let mut packages = Vec::new();
        
        if let Ok(system_pkgs) = scan_flatpak_dir("/var/lib/flatpak/app") {
            packages.extend(system_pkgs);
        }

        Ok(packages)
    }

    fn is_available(&self) -> bool {
        Path::new("/var/lib/flatpak").exists()
    }

    fn name(&self) -> &'static str {
        "Flatpak"
    }
}

fn scan_flatpak_dir(base_path: &str) -> Result<Vec<InstalledPackage>, ScanError> {
    let mut packages = Vec::new();

    if !Path::new(base_path).exists() {
        return Ok(packages);
    }

    let entries = fs::read_dir(base_path)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if let Ok(pkg) = parse_flatpak_app(&path) {
                packages.push(pkg);
            }
        }
    }

    Ok(packages)
}

fn parse_flatpak_app(app_dir: &std::path::Path) -> Result<InstalledPackage, ScanError> {
    let app_id = app_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ScanError::ParseError("Invalid app directory".to_string()))?
        .to_string();

    // Try to find the current version directory
    let current_path = app_dir.join("current");
    let version = if let Some(version) = flatpak_version(&app_id) {
        version
    } else if current_path.exists() {
        // Read the symlink target
        fs::read_link(&current_path)
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        "unknown".to_string()
    };

    // Calculate size by reading the actual directory
    let size_bytes = calculate_dir_size(app_dir).unwrap_or(0);

    Ok(InstalledPackage {
        name: app_id.clone(),
        package_id: app_id.clone(),
        source: PackageSource::Flatpak,
        version,
        size_bytes,
        install_path: app_dir.to_string_lossy().to_string(),
    })
}

fn flatpak_version(app_id: &str) -> Option<String> {
    let output = Command::new("flatpak").args(["info", "--system", app_id]).output().ok()?;
    if !output.status.success() { return None; }
    String::from_utf8_lossy(&output.stdout).lines().find_map(|line| line.trim().strip_prefix("Version:").map(str::trim).filter(|value| !value.is_empty()).map(str::to_string))
}

fn calculate_dir_size(path: &std::path::Path) -> Result<u64, std::io::Error> {
    let mut size = 0u64;
    let entries = fs::read_dir(path)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            size += calculate_dir_size(&path)?;
        } else if let Ok(metadata) = path.metadata() {
            size += metadata.len();
        }
    }

    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flatpak_available() {
        let adapter = FlatpakAdapter;
        let _available = adapter.is_available();
    }
}

