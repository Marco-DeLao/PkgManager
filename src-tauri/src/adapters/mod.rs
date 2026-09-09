/// Package adapter trait - all package managers must implement this
pub mod apt;
pub mod rpm;
pub mod pacman;
pub mod flatpak;
pub mod snap;
pub mod appimage;

use crate::models::{InstalledPackage, ScanError};
use std::path::Path;

/// Common trait for package manager adapters
pub trait PackageAdapter: Send + Sync {
    /// Scan for installed packages from this source
    fn scan(&self, scan_path: &Path) -> Result<Vec<InstalledPackage>, ScanError>;

    /// Check if this package manager is available on this system
    fn is_available(&self) -> bool;

    /// Human-readable name of the adapter
    fn name(&self) -> &'static str;
}

/// Get all available adapters for the current system
pub fn get_available_adapters() -> Vec<Box<dyn PackageAdapter>> {
    let mut adapters: Vec<Box<dyn PackageAdapter>> = vec![
        Box::new(apt::AptAdapter),
        Box::new(rpm::RpmAdapter),
        Box::new(pacman::PacmanAdapter),
        Box::new(flatpak::FlatpakAdapter),
        Box::new(snap::SnapAdapter),
        Box::new(appimage::AppImageAdapter),
    ];

    // Filter to only available adapters
    adapters.retain(|adapter| {
        if adapter.is_available() {
            log::info!("Package manager available: {}", adapter.name());
            true
        } else {
            log::debug!("Package manager not available: {}", adapter.name());
            false
        }
    });

    adapters
}
