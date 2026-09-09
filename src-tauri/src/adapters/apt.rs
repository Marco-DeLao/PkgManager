use crate::models::{InstalledPackage, PackageSource, ScanError};
use std::fs;
use std::path::Path;
use super::PackageAdapter;

pub struct AptAdapter;

impl PackageAdapter for AptAdapter {
    fn scan(&self, _scan_path: &Path) -> Result<Vec<InstalledPackage>, ScanError> {
        let dpkg_status_path = "/var/lib/dpkg/status";
        
        if !Path::new(dpkg_status_path).exists() {
            return Err(ScanError::NotFound(
                "dpkg status file not found - APT not available".to_string(),
            ));
        }

        let content = fs::read_to_string(dpkg_status_path)?;
        parse_dpkg_status(&content)
    }

    fn is_available(&self) -> bool {
        Path::new("/var/lib/dpkg/status").exists()
    }

    fn name(&self) -> &'static str {
        "APT/DPKG"
    }
}

fn parse_dpkg_status(content: &str) -> Result<Vec<InstalledPackage>, ScanError> {
    let mut packages = Vec::new();
    let mut current_package = DpkgPackageFields::default();
    let mut in_package = false;

    for line in content.lines() {
        if line.is_empty() {
            if in_package && !current_package.package.is_empty() {
                // Only include installed packages
                if current_package.status.contains("install") && 
                   current_package.status.contains("installed") {
                    if let Ok(pkg) = current_package.to_installed_package() {
                        packages.push(pkg);
                    }
                }
                current_package = DpkgPackageFields::default();
                in_package = false;
            }
        } else if let Some(value) = line.strip_prefix("Package: ") {
            in_package = true;
            current_package.package = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("Version: ") {
            current_package.version = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("Installed-Size: ") {
            // Installed-Size is in KiB
            if let Ok(size) = value.trim().split_whitespace().next().unwrap_or("0").parse::<u64>() {
                current_package.installed_size = size * 1024; // Convert KiB to bytes
            }
        } else if let Some(value) = line.strip_prefix("Status: ") {
            current_package.status = value.trim().to_string();
        }
    }

    // Process the last package if file doesn't end with blank line
    if in_package && !current_package.package.is_empty() {
        if current_package.status.contains("install") && 
           current_package.status.contains("installed") {
            if let Ok(pkg) = current_package.to_installed_package() {
                packages.push(pkg);
            }
        }
    }

    Ok(packages)
}

#[derive(Default, Debug)]
struct DpkgPackageFields {
    package: String,
    version: String,
    installed_size: u64,
    status: String,
}

impl DpkgPackageFields {
    fn to_installed_package(&self) -> Result<InstalledPackage, ScanError> {
        if self.package.is_empty() {
            return Err(ScanError::ParseError("Empty package name".to_string()));
        }

        // Construct a reasonable installation path
        // APT packages are typically in /usr/lib, /usr/bin, etc
        // For simplicity, we'll use a standardized path
        let install_path = format!("/usr/share/{}", self.package);

        Ok(InstalledPackage {
            name: self.package.clone(),
            package_id: self.package.clone(),
            source: PackageSource::Apt,
            version: self.version.clone(),
            size_bytes: self.installed_size,
            install_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dpkg_status_sample() {
        let sample = r#"Package: gcc
Version: 4:11.2.0-19ubuntu1
Installed-Size: 5242
Status: install ok installed

Package: gimp
Version: 2.10.30-1ubuntu1
Installed-Size: 10485
Status: install ok installed

Package: firefox
Version: 1:91.0+build2-0ubuntu1
Installed-Size: 314572
Status: install ok installed

Package: held-package
Version: 1.0.0
Installed-Size: 1000
Status: hold ok installed

Package: not-installed
Version: 1.0.0
Installed-Size: 1000
Status: deinstall ok config-files
"#;

        let packages = parse_dpkg_status(sample).unwrap();
        // Should find 4 installed packages, skip the not-installed one
        assert!(packages.len() >= 3, "Should find at least gcc, gimp, and firefox");
        assert!(packages.iter().any(|p| p.name == "gcc"));
        assert!(packages.iter().any(|p| p.name == "gimp"));
        assert!(packages.iter().any(|p| p.name == "firefox"));
        // Should not include deinstall packages
        assert!(!packages.iter().any(|p| p.name == "not-installed"));
    }

    #[test]
    fn test_installed_size_conversion() {
        let sample = r#"Package: test-pkg
Version: 1.0
Installed-Size: 1024
Status: install ok installed
"#;
        let packages = parse_dpkg_status(sample).unwrap();
        assert_eq!(packages.len(), 1);
        // 1024 KiB = 1024 * 1024 bytes = 1,048,576 bytes
        assert_eq!(packages[0].size_bytes, 1024 * 1024);
    }
}

