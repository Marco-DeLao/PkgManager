use serde::{Deserialize, Serialize};

/// Represents the source of a package installation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum PackageSource {
    #[serde(rename = "apt")]
    Apt,
    #[serde(rename = "rpm")]
    Rpm,
    #[serde(rename = "pacman")]
    Pacman,
    #[serde(rename = "flatpak")]
    Flatpak,
    #[serde(rename = "snap")]
    Snap,
    #[serde(rename = "appimage")]
    AppImage,
}

impl std::fmt::Display for PackageSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageSource::Apt => write!(f, "APT"),
            PackageSource::Rpm => write!(f, "RPM"),
            PackageSource::Pacman => write!(f, "Pacman"),
            PackageSource::Flatpak => write!(f, "Flatpak"),
            PackageSource::Snap => write!(f, "Snap"),
            PackageSource::AppImage => write!(f, "AppImage"),
        }
    }
}

/// Represents a single installed package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    /// Display name of the package
    pub name: String,
    /// Original package ID (e.g., "gimp" for apt, "org.gimp.GIMP" for flatpak)
    pub package_id: String,
    /// Source package manager
    pub source: PackageSource,
    /// Version string
    pub version: String,
    /// Installed size in bytes
    pub size_bytes: u64,
    /// Installation path on filesystem
    pub install_path: String,
}

/// Result of a scan operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    /// All packages found across all sources
    pub all_packages: Vec<InstalledPackage>,
    /// Duplicate groups (packages that are likely the same application)
    pub duplicate_groups: Vec<DuplicateGroup>,
    /// Total disk space used by all packages
    pub total_size_bytes: u64,
    /// Total disk space wasted by duplicates
    pub wasted_size_bytes: u64,
}

/// A group of packages that appear to be duplicates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    /// Normalized name of the application (e.g., "Firefox")
    pub app_name: String,
    /// All packages in this group
    pub packages: Vec<InstalledPackage>,
    /// Total size used by all copies
    pub total_size_bytes: u64,
    /// Size that would be freed if keeping only one copy
    pub wasted_size_bytes: u64,
    pub version_comparison: Option<VersionComparison>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionComparison {
    pub newer_package_id: String,
    pub newer_source: PackageSource,
    pub newer_version: String,
    pub older_package_id: String,
    pub older_source: PackageSource,
    pub older_version: String,
    pub recommendation: String,
}

/// Error type for package scanning operations
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Command error: {0}")]
    CommandError(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}
