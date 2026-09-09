use crate::models::{InstalledPackage, PackageSource, ScanError};
use super::PackageAdapter;
use std::fs;
use std::path::Path;

pub struct AppImageAdapter;

impl PackageAdapter for AppImageAdapter {
    fn scan(&self, scan_path: &Path) -> Result<Vec<InstalledPackage>, ScanError> {
        find_appimages(scan_path)
    }

    fn is_available(&self) -> bool {
        // AppImages can always exist, so we consider this adapter always "available"
        // in the sense that we can attempt to scan for them
        true
    }

    fn name(&self) -> &'static str {
        "AppImage"
    }
}

fn find_appimages(search_path: &Path) -> Result<Vec<InstalledPackage>, ScanError> {
    let mut packages = Vec::new();

    if !Path::new(search_path).exists() {
        return Ok(packages);
    }

    let entries = fs::read_dir(search_path)
        .map_err(|_| ScanError::NotFound(format!("Cannot read {}", search_path.display())))?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // Look for .AppImage files
            if filename.ends_with(".AppImage") || filename.ends_with(".appimage") {
                if let Ok(pkg) = parse_appimage_file(&path, filename) {
                    packages.push(pkg);
                }
            }
        }
    }

    Ok(packages)
}

fn parse_appimage_file(path: &Path, filename: &str) -> Result<InstalledPackage, ScanError> {
    let name = filename
        .trim_end_matches(".AppImage")
        .trim_end_matches(".appimage")
        .to_string();

    let size_bytes = path.metadata()
        .map(|m| m.len())
        .unwrap_or(0);

    // AppImages don't have built-in version info, so we'd need to extract it
    // from the filename (e.g., app-v1.0.AppImage) or query the AppImage directly
    let version = extract_version_from_filename(&name).unwrap_or_else(|| "unknown".to_string());

    Ok(InstalledPackage {
        name: name.clone(),
        package_id: name.clone(),
        source: PackageSource::AppImage,
        version,
        size_bytes,
        install_path: path.to_string_lossy().to_string(),
    })
}

fn extract_version_from_filename(filename: &str) -> Option<String> {
    // Try to extract version from patterns like:
    // app-1.0.0, app_v1.0.0, app-1.0-x86_64, etc.
    
    // Look for common version patterns
    if let Some(pos) = filename.rfind('-') {
        let potential_version = &filename[pos + 1..];
        // Check if it looks like a version (starts with digit or 'v')
        if potential_version.chars().next().map_or(false, |c| c.is_numeric() || c == 'v') {
            return Some(potential_version.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_version() {
        assert_eq!(
            extract_version_from_filename("app-1.0.0"),
            Some("1.0.0".to_string())
        );
        assert_eq!(
            extract_version_from_filename("app-v2.1"),
            Some("v2.1".to_string())
        );
        assert_eq!(
            extract_version_from_filename("app"),
            None
        );
    }
}

