use app_lib::adapters;
use app_lib::cleanup;
use app_lib::commands;
use app_lib::install;
use app_lib::models::PackageSource;
use app_lib::matcher::PackageMatcher;
use app_lib::scanner::package_is_in_scan_path;
use std::env;
use std::path::Path;
use std::fs;

fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    println!("=== PkgManager CLI - Package Scanner ===\n");

    let args: Vec<String> = env::args().collect();
    if args.get(1).map(String::as_str) == Some("search") {
        let query = args.get(2).map(String::as_str).unwrap_or_default();
        println!("=== PkgManager package search: {} ===", query);
        match commands::search_packages(query.to_string()) {
            Ok(results) => for package in results { println!("{}\t{}\t{}", package.source, package.package_id, package.description); },
            Err(error) => eprintln!("search error: {}", error),
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("install") {
        let query = args.get(2).map(String::as_str).unwrap_or_default();
        let source = args.get(3).map(String::as_str).unwrap_or("pacman");
        let Some(expected) = parse_source(source) else { eprintln!("Unknown package source: {}", source); return; };
        let result = install::search_available(query).into_iter().filter_map(Result::ok).flatten().find(|item| item.source == expected && (item.package_id == query || (expected == PackageSource::Rpm && item.package_id.strip_suffix(".x86_64").or_else(|| item.package_id.strip_suffix(".noarch")).map(|name| name == query).unwrap_or(false))));
        let Some(result) = result else { eprintln!("No exact search result for {} from {}", query, source); return; };
        let confirmation = install::confirmation_text(&result);
        println!("Command preview: {}", commands::install_preview(result.clone()).expect("valid search result").command);
        println!("Confirmation: {}", confirmation);
        match commands::install_package(result, confirmation) {
            Ok(outcome) => {
                println!("Install succeeded: {}", outcome.package_name);
                println!("Command: {}", outcome.command);
                println!("stdout:\n{}", outcome.stdout);
                println!("stderr:\n{}", outcome.stderr);
                println!("audit: {}", outcome.audit_log);
            }
            Err(error) => eprintln!("Install failed: {}", error),
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("remove") {
        let package_id = args.get(2).map(String::as_str).unwrap_or_default();
        let source_name = args.get(3).map(String::as_str).unwrap_or("pacman");
        let Some(source) = parse_source(source_name) else { eprintln!("Unknown package source: {}", source_name); return; };
        let install_path = match source {
            PackageSource::Pacman => fs::read_dir("/var/lib/pacman/local").ok().and_then(|entries| entries.filter_map(Result::ok).map(|entry| entry.path()).find(|path| path.file_name().and_then(|name| name.to_str()).map(|name| name == package_id || name.starts_with(&format!("{}-", package_id))).unwrap_or(false))).map(|path| path.to_string_lossy().into_owned()).unwrap_or_default(),
            PackageSource::Flatpak => format!("/var/lib/flatpak/app/{}", package_id),
            PackageSource::Apt | PackageSource::Rpm => String::new(),
            _ => String::new(),
        };
        let size_bytes = match source {
            PackageSource::Pacman => std::process::Command::new("pacman").args(["-Qi", "--", package_id]).output().ok().and_then(|output| parse_pacman_size(&String::from_utf8_lossy(&output.stdout))).unwrap_or(0),
            PackageSource::Apt => std::process::Command::new("dpkg-query").args(["-W", "-f=${Installed-Size}", "--", package_id]).output().ok().and_then(|output| String::from_utf8_lossy(&output.stdout).trim().parse::<u64>().ok()).map(|size| size * 1024).unwrap_or(0),
            PackageSource::Rpm => std::process::Command::new("rpm").args(["-q", "--qf", "%{SIZE}", "--", package_id]).output().ok().and_then(|output| String::from_utf8_lossy(&output.stdout).trim().parse::<u64>().ok()).unwrap_or(0),
            _ => 0,
        };
        let package = app_lib::models::InstalledPackage { name: package_id.to_string(), package_id: package_id.to_string(), source, version: "live-test".into(), size_bytes, install_path };
        match commands::cleanup_package(package.clone(), cleanup::confirmation_text(&package)) {
            Ok(result) => println!("Removal succeeded: {}\naudit: {}", result.package_name, result.audit_log),
            Err(error) => eprintln!("Removal failed: {}", error),
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("updates") {
        let result = commands::check_updates();
        println!("=== AVAILABLE UPDATES ({}) ===", result.updates.len());
        for update in &result.updates {
            println!("{} {} -> {} ({})", update.package_id, update.current_version, update.available_version, update.source);
        }
        for error in result.errors { eprintln!("check error: {}", error); }
        return;
    }
    if args.get(1).map(String::as_str) == Some("update-one") {
        let package_id = args.get(2).map(String::as_str).unwrap_or_default();
        let result = commands::check_updates();
        let Some(update) = result.updates.into_iter().find(|item| item.package_id == package_id) else { eprintln!("No available update for {}", package_id); return; };
        let confirmation = app_lib::updates::confirmation_text(std::slice::from_ref(&update));
        println!("Command target: {} {} -> {}", update.package_id, update.current_version, update.available_version);
        println!("Confirmation: {}", confirmation);
        match commands::update_all(vec![update], confirmation) {
            Ok(report) => {
                println!("successful={} failed={} skipped={} audit={}", report.successful.len(), report.failed.len(), report.skipped.len(), report.audit_log);
                for outcome in report.successful.iter().chain(report.failed.iter()) { println!("{} {} detail={} stdout={:?} stderr={:?}", outcome.status, outcome.package_id, outcome.detail, outcome.stdout, outcome.stderr); }
            }
            Err(error) => eprintln!("Update failed: {}", error),
        }
        return;
    }
    if args.get(1).map(String::as_str) == Some("orphans") {
        let result = commands::check_orphans();
        println!("=== PACMAN ORPHANS ({}) ===", result.pacman.len());
        for package in result.pacman { println!("{} {} bytes", package.name, package.size_bytes); }
        println!("=== APT ORPHANS ({}) ===", result.apt.len());
        for package in result.apt { println!("{} {} bytes", package.name, package.size_bytes); }
        println!("=== DNF ORPHANS ({}) ===", result.dnf.len());
        for package in result.dnf { println!("{} {} bytes", package.name, package.size_bytes); }
        for error in result.errors { eprintln!("check error: {}", error); }
        return;
    }
    if args.get(1).map(String::as_str) == Some("remove-orphan") {
        let packages = args.iter().skip(2).map(|package_id| app_lib::orphans::OrphanPackage { name: package_id.clone(), source: PackageSource::Pacman, size_bytes: 0, note: String::new() }).collect::<Vec<_>>();
        let confirmation = app_lib::orphans::confirmation_text(&packages);
        println!("Confirmation: {}", confirmation);
        match commands::remove_orphans(packages, confirmation) {
            Ok(outcomes) => for outcome in outcomes { println!("{} {} stdout={:?} stderr={:?}", if outcome.success { "removed" } else { "failed" }, outcome.package_id, outcome.stdout, outcome.stderr); },
            Err(error) => eprintln!("Orphan removal failed: {}", error),
        }
        return;
    }
    let scan_path = if args.len() > 1 {
        args[1].clone()
    } else {
        "/".to_string()
    };

    println!("Scanning path: {}\n", scan_path);

    // Get available adapters
    let adapters = adapters::get_available_adapters();
    println!("Available package managers: {}\n", adapters.len());

    let mut all_packages = Vec::new();

    // Scan each adapter
    for adapter in adapters {
        println!("Scanning {}...", adapter.name());
        match adapter.scan(Path::new(&scan_path)) {
            Ok(packages) => {
                let packages = packages
                    .into_iter()
                    .filter(|package| package_is_in_scan_path(&package.install_path, Path::new(&scan_path)))
                    .collect::<Vec<_>>();
                println!("  ✓ Found {} packages", packages.len());
                all_packages.extend(packages);
            }
            Err(e) => {
                println!("  ✗ Error: {}", e);
            }
        }
    }

    println!("\n=== ALL PACKAGES ({} total) ===\n", all_packages.len());

    // Display all packages
    for (i, pkg) in all_packages.iter().enumerate() {
        let size_mb = pkg.size_bytes as f64 / (1024.0 * 1024.0);
        println!(
            "{}. {} ({}) - v{} - {:.2} MB",
            i + 1,
            pkg.name,
            pkg.source,
            pkg.version,
            size_mb
        );
        println!("   ID: {}", pkg.package_id);
        println!("   Path: {}", pkg.install_path);
    }

    // Find duplicates
    println!("\n=== DUPLICATE ANALYSIS ===\n");
    let duplicates = PackageMatcher::find_duplicates(&all_packages);

    if duplicates.is_empty() {
        println!("No duplicates found!");
    } else {
        let total_wasted: u64 = duplicates.iter().map(|g| g.wasted_size_bytes).sum();
        let total_wasted_mb = total_wasted as f64 / (1024.0 * 1024.0);
        println!("Found {} duplicate groups", duplicates.len());
        println!("Total wasted space: {:.2} MB\n", total_wasted_mb);

        for (i, group) in duplicates.iter().enumerate() {
            println!("{}. {} - {} copies", i + 1, group.app_name, group.packages.len());
            let wasted_mb = group.wasted_size_bytes as f64 / (1024.0 * 1024.0);
            println!("   Wasted space: {:.2} MB", wasted_mb);
            for (j, pkg) in group.packages.iter().enumerate() {
                let size_mb = pkg.size_bytes as f64 / (1024.0 * 1024.0);
                println!("   {}. {} ({}) v{} - {:.2} MB", j + 1, pkg.name, pkg.source, pkg.version, size_mb);
            }
            println!();
        }
    }

    // Summary
    let total_size: u64 = all_packages.iter().map(|p| p.size_bytes).sum();
    let wasted_size: u64 = duplicates.iter().map(|g| g.wasted_size_bytes).sum();
    let total_mb = total_size as f64 / (1024.0 * 1024.0);
    let wasted_mb = wasted_size as f64 / (1024.0 * 1024.0);

    println!("=== SUMMARY ===");
    println!("Total packages: {}", all_packages.len());
    println!("Total size: {:.2} MB", total_mb);
    println!("Wasted space (duplicates): {:.2} MB", wasted_mb);
    if total_size > 0 {
        let waste_pct = (wasted_size as f64 / total_size as f64) * 100.0;
        println!("Waste percentage: {:.1}%", waste_pct);
    }
}

fn parse_pacman_size(output: &str) -> Option<u64> {
    output.lines().find_map(|line| {
        let value = line.strip_prefix("Installed Size")?.split(':').nth(1)?.trim();
        let mut parts = value.split_whitespace();
        let number = parts.next()?.parse::<f64>().ok()?;
        let multiplier = match parts.next().unwrap_or("B") {
            "KiB" => 1024.0,
            "MiB" => 1024.0 * 1024.0,
            "GiB" => 1024.0 * 1024.0 * 1024.0,
            _ => 1.0,
        };
        Some((number * multiplier) as u64)
    })
}

fn parse_source(value: &str) -> Option<PackageSource> {
    match value.to_ascii_lowercase().as_str() {
        "apt" | "debian" | "ubuntu" => Some(PackageSource::Apt),
        "dnf" | "rpm" | "fedora" => Some(PackageSource::Rpm),
        "pacman" | "arch" => Some(PackageSource::Pacman),
        "flatpak" => Some(PackageSource::Flatpak),
        _ => None,
    }
}
