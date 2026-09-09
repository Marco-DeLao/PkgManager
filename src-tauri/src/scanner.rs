use crate::adapters;
use crate::matcher::PackageMatcher;
use crate::models::{ScanResult, ScanError};
use log::info;
use std::path::{Path, PathBuf};

pub struct PackageScanner {
    scan_path: String,
}

impl PackageScanner {
    pub fn new(scan_path: String) -> Self {
        PackageScanner { scan_path }
    }

    /// Execute a full scan of installed packages
    pub fn scan(&self) -> Result<ScanResult, ScanError> {
        info!("Starting package scan for path: {}", self.scan_path);
        let scan_path = PathBuf::from(&self.scan_path);

        let adapters = adapters::get_available_adapters();
        info!("Found {} available package managers", adapters.len());

        let mut all_packages = Vec::new();

        // Scan each available package manager
        for adapter in adapters {
            info!("Scanning {}", adapter.name());
            match adapter.scan(&scan_path) {
                Ok(packages) => {
                    let packages = packages
                        .into_iter()
                        .filter(|package| package_is_in_scan_path(&package.install_path, &scan_path))
                        .collect::<Vec<_>>();
                    info!("Found {} packages from {}", packages.len(), adapter.name());
                    all_packages.extend(packages);
                }
                Err(e) => {
                    log::warn!("Error scanning {}: {}", adapter.name(), e);
                    // Continue with next adapter
                }
            }
        }

        info!("Total packages found: {}", all_packages.len());

        // Calculate totals
        let total_size_bytes: u64 = all_packages.iter().map(|p| p.size_bytes).sum();

        // Find duplicates
        let duplicate_groups = PackageMatcher::find_duplicates(&all_packages);
        let wasted_size_bytes: u64 = duplicate_groups.iter().map(|g| g.wasted_size_bytes).sum();

        info!("Found {} duplicate groups", duplicate_groups.len());
        info!("Total wasted space: {} bytes", wasted_size_bytes);

        Ok(ScanResult {
            all_packages,
            duplicate_groups,
            total_size_bytes,
            wasted_size_bytes,
        })
    }
}

pub fn package_is_in_scan_path(package_path: &str, scan_path: &Path) -> bool {
    let scan_path = scan_path.canonicalize().unwrap_or_else(|_| scan_path.to_path_buf());
    let package_path = Path::new(package_path);
    package_path.starts_with(&scan_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scanner_creation() {
        let scanner = PackageScanner::new("/".to_string());
        assert_eq!(scanner.scan_path, "/");
    }

    #[test]
    fn only_packages_inside_selected_path_are_kept() {
        let scan_path = Path::new("/home/user/Downloads");
        assert!(package_is_in_scan_path("/home/user/Downloads/tool.AppImage", scan_path));
        assert!(!package_is_in_scan_path("/var/lib/pacman/local/tool-1", scan_path));
    }
}
