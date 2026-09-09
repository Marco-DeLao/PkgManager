use crate::scanner::PackageScanner;
use crate::cleanup::{self, CleanupPreview, CleanupResult};
use crate::models::ScanResult;
use crate::install::{self, InstallOutcome, InstallPreview, SearchResult};
use crate::updates::{self, AvailableUpdate, UpdateCheck, UpdateReport};
use crate::orphans::{self, OrphanCheck, OrphanPackage, OrphanRemovalOutcome};
use crate::path_picker;
use log::info;

#[tauri::command]
pub fn ping_command() -> String {
    info!("Ping command received");
    "Pong from Tauri backend!".to_string()
}

#[tauri::command]
pub fn start_scan(scan_path: String) -> Result<ScanResult, String> {
    info!("Starting scan for path: {}", scan_path);
    
    let scanner = PackageScanner::new(scan_path.clone());
    match scanner.scan() {
        Ok(result) => {
            info!("Scan completed successfully");
            Ok(result)
        }
        Err(e) => {
            log::error!("Scan failed: {}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub fn cleanup_preview(package: crate::models::InstalledPackage) -> CleanupPreview {
    cleanup::preview(&package)
}

#[tauri::command]
pub fn cleanup_package(
    package: crate::models::InstalledPackage,
    confirmation: String,
) -> Result<CleanupResult, String> {
    cleanup::execute(&package, &confirmation).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn pick_scan_path() -> Result<String, String> {
    path_picker::pick_scan_path().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub fn search_packages(query: String) -> Result<Vec<SearchResult>, String> {
    if query.trim().is_empty() { return Ok(Vec::new()); }
    let mut results = Vec::new();
    for response in install::search_available(&query) {
        results.extend(response.map_err(|error| error.to_string())?);
    }
    Ok(install::rank_search_results(results, &query))
}

#[tauri::command]
pub fn install_preview(package: SearchResult) -> Result<InstallPreview, String> {
    install::preview(&package).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn install_package(package: SearchResult, confirmation: String) -> Result<InstallOutcome, String> {
    install::execute(&package, &confirmation).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn cancel_install() -> bool {
    install::cancel()
}

#[tauri::command]
pub fn check_updates() -> UpdateCheck {
    updates::check()
}

#[tauri::command]
pub fn update_all(packages: Vec<AvailableUpdate>, confirmation: String) -> Result<UpdateReport, String> {
    updates::update_all(&packages, &confirmation).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn check_orphans() -> OrphanCheck { orphans::check() }

#[tauri::command]
pub fn remove_orphans(packages: Vec<OrphanPackage>, confirmation: String) -> Result<Vec<OrphanRemovalOutcome>, String> {
    orphans::remove_all(&packages, &confirmation).map_err(|error| error.to_string())
}
