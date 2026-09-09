use crate::models::{InstalledPackage, PackageSource, ScanError};
use super::PackageAdapter;
use std::path::Path;
use std::fs;
use std::io::BufRead;

pub struct PacmanAdapter;

impl PackageAdapter for PacmanAdapter {
    fn scan(&self, _scan_path: &Path) -> Result<Vec<InstalledPackage>, ScanError> {
        let pacman_db = "/var/lib/pacman/local";

        if !Path::new(pacman_db).exists() {
            return Err(ScanError::NotFound(
                "Pacman database not found".to_string(),
            ));
        }

        scan_pacman_db(pacman_db)
    }

    fn is_available(&self) -> bool {
        Path::new("/var/lib/pacman/local").exists()
    }

    fn name(&self) -> &'static str {
        "Pacman"
    }
}

fn scan_pacman_db(db_path: &str) -> Result<Vec<InstalledPackage>, ScanError> {
    let mut packages = Vec::new();
    let entries = fs::read_dir(db_path)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Each directory is a package-version directory
            if let Ok(pkg) = parse_pacman_package(&path) {
                packages.push(pkg);
            }
        }
    }

    Ok(packages)
}

fn parse_pacman_package(pkg_dir: &std::path::Path) -> Result<InstalledPackage, ScanError> {
    let dir_name = pkg_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ScanError::ParseError("Invalid directory name".to_string()))?;

    // Try to read desc file for additional info
    let desc_path = pkg_dir.join("desc");
    let desc = fs::read_to_string(&desc_path)?;
    let name = extract_desc_field(&desc, "%NAME%").unwrap_or_else(|| dir_name.to_string());
    let version = extract_desc_field(&desc, "%VERSION%").unwrap_or_else(|| "unknown".to_string());
    let size_bytes = if desc_path.exists() {
        extract_size_from_desc(&desc_path).unwrap_or(0)
    } else {
        0
    };

    Ok(InstalledPackage {
        name: name.clone(),
        package_id: name.clone(),
        source: PackageSource::Pacman,
        version,
        size_bytes,
        install_path: format!("/var/lib/pacman/local/{}", dir_name),
    })
}

fn extract_desc_field(content: &str, field: &str) -> Option<String> {
    let mut lines = content.lines();
    while let Some(line) = lines.next() {
        if line == field {
            return lines.next().map(str::trim).filter(|value| !value.is_empty()).map(str::to_string);
        }
    }
    None
}

fn extract_size_from_desc(desc_path: &std::path::Path) -> Result<u64, ScanError> {
    let file = fs::File::open(desc_path)?;
    let reader = std::io::BufReader::new(file);
    let mut lines = reader.lines();

    while let Some(Ok(line)) = lines.next() {
        if line == "%SIZE%" {
            if let Some(Ok(size_line)) = lines.next() {
                if let Ok(size) = size_line.parse::<u64>() {
                    return Ok(size);
                }
            }
        }
    }

    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pacman_available() {
        let adapter = PacmanAdapter;
        // This will be true or false depending on the system
        let _available = adapter.is_available();
    }

    #[test]
    fn test_parse_desc_fields() {
        let desc = "%NAME%\ngimp\n\n%VERSION%\n3.2.4-2\n";
        assert_eq!(extract_desc_field(desc, "%NAME%"), Some("gimp".to_string()));
        assert_eq!(extract_desc_field(desc, "%VERSION%"), Some("3.2.4-2".to_string()));
    }
}

