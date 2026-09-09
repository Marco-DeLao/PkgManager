use crate::models::{InstalledPackage, PackageSource, ScanError};
use super::PackageAdapter;
use std::path::Path;
use std::fs;

pub struct SnapAdapter;

impl PackageAdapter for SnapAdapter {
    fn scan(&self, _scan_path: &Path) -> Result<Vec<InstalledPackage>, ScanError> {
        if !self.is_available() {
            return Ok(Vec::new());
        }

        scan_snapd_dir("/var/lib/snapd/snaps")
    }

    fn is_available(&self) -> bool {
        Path::new("/var/lib/snapd").exists() || Path::new("/usr/bin/snap").exists()
    }

    fn name(&self) -> &'static str {
        "Snap"
    }
}

fn scan_snapd_dir(base_path: &str) -> Result<Vec<InstalledPackage>, ScanError> {
    let mut packages = Vec::new();

    if !Path::new(base_path).exists() {
        return Ok(packages);
    }

    let entries = fs::read_dir(base_path)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        // Snap files are typically named like: snap-name_version.snap
        if filename.ends_with(".snap") {
            if let Ok(pkg) = parse_snap_file(&path, filename) {
                packages.push(pkg);
            }
        }
    }

    Ok(packages)
}

fn parse_snap_file(path: &std::path::Path, filename: &str) -> Result<InstalledPackage, ScanError> {
    // Remove .snap extension
    let name_with_version = filename.trim_end_matches(".snap");
    
    // Split on last underscore to separate name from version
    let parts: Vec<&str> = name_with_version.rsplitn(2, '_').collect();
    let (name, version) = if parts.len() == 2 {
        (parts[1], parts[0])
    } else {
        (name_with_version, "unknown")
    };

    let size_bytes = path.metadata()
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(InstalledPackage {
        name: name.to_string(),
        package_id: name.to_string(),
        source: PackageSource::Snap,
        version: version.to_string(),
        size_bytes,
        install_path: path.to_string_lossy().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snap_available() {
        let adapter = SnapAdapter;
        let _available = adapter.is_available();
    }
}

