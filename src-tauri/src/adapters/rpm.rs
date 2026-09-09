use crate::models::{InstalledPackage, PackageSource, ScanError};
use super::PackageAdapter;
use std::path::Path;
use std::process::Command;

pub struct RpmAdapter;

impl PackageAdapter for RpmAdapter {
    fn scan(&self, _scan_path: &Path) -> Result<Vec<InstalledPackage>, ScanError> {
        if !self.is_available() {
            return Ok(Vec::new());
        }

        scan_rpm_packages()
    }

    fn is_available(&self) -> bool {
        Path::new("/var/lib/rpm").exists() || Path::new("/usr/bin/rpm").exists()
    }

    fn name(&self) -> &'static str {
        "RPM/DNF"
    }
}

fn scan_rpm_packages() -> Result<Vec<InstalledPackage>, ScanError> {
    // Try to use rpm command to query installed packages
    // Format: rpm -qa --qf='%{NAME}|%{VERSION}|%{SIZE}\n'
    
    let output = Command::new("rpm")
        .args(&["-qa", "--queryformat", "%{NAME}|%{VERSION}|%{SIZE}\\n"])
        .output()
        .map_err(|e| ScanError::CommandError(format!("Failed to run rpm command: {}", e)))?;

    if !output.status.success() {
        return Err(ScanError::CommandError(
            "rpm command failed".to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_rpm_output(&stdout)
}

fn parse_rpm_output(output: &str) -> Result<Vec<InstalledPackage>, ScanError> {
    let mut packages = Vec::new();

    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 3 {
            let name = parts[0].trim().to_string();
            let version = parts[1].trim().to_string();
            let size_bytes = parts[2]
                .trim()
                .parse::<u64>()
                .unwrap_or(0);

            packages.push(InstalledPackage {
                name: name.clone(),
                package_id: name.clone(),
                source: PackageSource::Rpm,
                version,
                size_bytes,
                install_path: format!("/usr/share/{}", name),
            });
        }
    }

    Ok(packages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rpm_output() {
        let sample = r#"gcc|11.2.0|5242880
gimp|2.10.30|10485760
firefox|91.0|314572800
"#;
        let packages = parse_rpm_output(sample).unwrap();
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].name, "gcc");
        assert_eq!(packages[0].version, "11.2.0");
        assert_eq!(packages[0].size_bytes, 5242880);
    }

    #[test]
    fn test_rpm_available() {
        let adapter = RpmAdapter;
        let _available = adapter.is_available();
    }
}


