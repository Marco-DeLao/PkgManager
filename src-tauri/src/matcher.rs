use crate::models::{InstalledPackage, DuplicateGroup, VersionComparison};
use strsim::jaro_winkler;

const SIMILARITY_THRESHOLD: f64 = 0.80;

/// Matches packages across different sources to identify duplicates
pub struct PackageMatcher;

impl PackageMatcher {
    /// Find duplicate groups among packages from different sources
    pub fn find_duplicates(packages: &[InstalledPackage]) -> Vec<DuplicateGroup> {
        if packages.is_empty() {
            return Vec::new();
        }

        let mut visited = vec![false; packages.len()];
        let mut groups = Vec::new();

        for i in 0..packages.len() {
            if visited[i] {
                continue;
            }

            let mut group = vec![packages[i].clone()];
            visited[i] = true;

            // Find all packages that match with packages[i]
            for j in (i + 1)..packages.len() {
                if visited[j] {
                    continue;
                }

                // Check if packages[j] matches any package in the current group
                if group.iter().any(|p| Self::packages_match(p, &packages[j])) {
                    group.push(packages[j].clone());
                    visited[j] = true;
                }
            }

            // Only keep groups with more than one package (actual duplicates)
            if group.len() > 1 {
                let total_size: u64 = group.iter().map(|p| p.size_bytes).sum();
                let wasted_size = if total_size > 0 {
                    total_size - group.iter().map(|p| p.size_bytes).min().unwrap_or(0)
                } else {
                    0
                };

                let version_comparison = Self::version_comparison(&group);
                groups.push(DuplicateGroup {
                    app_name: Self::normalize_name(&group[0].name),
                    packages: group,
                    total_size_bytes: total_size,
                    wasted_size_bytes: wasted_size,
                    version_comparison,
                });
            }
        }

        // Sort by wasted space (largest first)
        groups.sort_by(|a, b| b.wasted_size_bytes.cmp(&a.wasted_size_bytes));

        groups
    }

    /// Check if two packages represent the same application
    fn packages_match(pkg1: &InstalledPackage, pkg2: &InstalledPackage) -> bool {
        // If from the same source, they're probably not duplicates
        if pkg1.source == pkg2.source {
            return false;
        }

        let norm1 = Self::normalize_name_for_matching(pkg1);
        let norm2 = Self::normalize_name_for_matching(pkg2);

        // Exact match after normalization
        if norm1 == norm2 {
            return true;
        }

        // Fuzzy matching is only useful for a base name and its explicit variant
        // (for example, vim and vim-gtk), not unrelated short names such as gzip/gimp.
        if !norm1.starts_with(&norm2) && !norm2.starts_with(&norm1) {
            return false;
        }

        // Fuzzy match
        let similarity = jaro_winkler(&norm1, &norm2);
        similarity >= SIMILARITY_THRESHOLD
    }

    fn normalize_name_for_matching(package: &InstalledPackage) -> String {
        Self::normalize_name(&package.name)
    }

    fn version_comparison(packages: &[InstalledPackage]) -> Option<VersionComparison> {
        let parsed = packages.iter().map(|package| parse_version(&package.version).map(|version| (package, version))).collect::<Option<Vec<_>>>()?;
        let newest = parsed.iter().max_by(|left, right| left.1.cmp(&right.1))?;
        let oldest = parsed.iter().min_by(|left, right| left.1.cmp(&right.1))?;
        if newest.1 == oldest.1 || parsed.iter().filter(|(_, version)| *version == newest.1).count() != 1 { return None; }
        Some(VersionComparison {
            newer_package_id: newest.0.package_id.clone(), newer_source: newest.0.source, newer_version: newest.0.version.clone(),
            older_package_id: oldest.0.package_id.clone(), older_source: oldest.0.source, older_version: oldest.0.version.clone(),
            recommendation: format!("Recommendation: keep the {} copy ({}) and remove the {} copy ({}) - {} has the newer version.", newest.0.source, newest.0.version, oldest.0.source, oldest.0.version, newest.0.source),
        })
    }

    /// Normalize package names for comparison
    fn normalize_name(name: &str) -> String {
        let mut normalized = name.to_lowercase();

        // Remove reverse DNS prefixes (e.g., org.mozilla.firefox -> firefox)
        if normalized.contains(".") {
            if let Some(last_part) = normalized.split('.').last() {
                normalized = last_part.to_string();
            }
        }

        for prefix in &["gnome-", "kde-"] {
            if let Some(stripped) = normalized.strip_prefix(prefix) {
                normalized = stripped.to_string();
                break;
            }
        }

        // Remove common suffixes
        for suffix in &["-bin", "-git", "-devel", "_stable", "_snapshot"] {
            if normalized.ends_with(suffix) {
                normalized = normalized.trim_end_matches(suffix).to_string();
            }
        }

        // Remove whitespace
        normalized.split_whitespace().collect::<Vec<_>>().join("")
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct ComparableVersion { epoch: u64, parts: Vec<u64>, release: u64 }

fn parse_version(value: &str) -> Option<ComparableVersion> {
    if value.is_empty() || value.eq_ignore_ascii_case("stable") || value.eq_ignore_ascii_case("latest") { return None; }
    let (epoch, rest) = if let Some((epoch, rest)) = value.split_once(':') { (epoch.parse().ok()?, rest) } else { (0, value) };
    let (core, release) = if let Some((core, release)) = rest.rsplit_once('-') { (core, release.parse().unwrap_or(0)) } else { (rest, 0) };
    let parts = core.split('.').map(|part| part.parse::<u64>().ok()).collect::<Option<Vec<_>>>()?;
    if parts.is_empty() { None } else { Some(ComparableVersion { epoch, parts, release }) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PackageSource;

    #[test]
    fn test_normalize_name_reverse_dns() {
        assert_eq!(
            PackageMatcher::normalize_name("org.mozilla.firefox"),
            "firefox"
        );
        assert_eq!(
            PackageMatcher::normalize_name("org.gimp.GIMP"),
            "gimp"
        );
    }

    #[test]
    fn test_normalize_name_suffixes() {
        assert_eq!(
            PackageMatcher::normalize_name("firefox-bin"),
            "firefox"
        );
        assert_eq!(
            PackageMatcher::normalize_name("vim-git"),
            "vim"
        );
        assert_eq!(
            PackageMatcher::normalize_name("gnome-calculator"),
            "calculator"
        );
    }

    #[test]
    fn test_find_duplicates() {
        let packages = vec![
            InstalledPackage {
                name: "firefox".to_string(),
                package_id: "firefox".to_string(),
                source: PackageSource::Apt,
                version: "91.0".to_string(),
                size_bytes: 1000,
                install_path: "/usr/lib/firefox".to_string(),
            },
            InstalledPackage {
                name: "org.mozilla.firefox".to_string(),
                package_id: "org.mozilla.firefox".to_string(),
                source: PackageSource::Flatpak,
                version: "91.0".to_string(),
                size_bytes: 2000,
                install_path: "/var/lib/flatpak/app/org.mozilla.firefox".to_string(),
            },
            InstalledPackage {
                name: "vim".to_string(),
                package_id: "vim".to_string(),
                source: PackageSource::Apt,
                version: "8.2.0".to_string(),
                size_bytes: 500,
                install_path: "/usr/bin/vim".to_string(),
            },
        ];

        let duplicates = PackageMatcher::find_duplicates(&packages);
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].packages.len(), 2);
        // Total = 1000 + 2000 = 3000, wasted = 3000 - 1000 (smallest) = 2000
        assert_eq!(duplicates[0].wasted_size_bytes, 2000);
    }

    #[test]
    fn test_unrelated_similar_names_do_not_create_false_duplicates() {
        let packages = vec![
            InstalledPackage {
                name: "gzip".to_string(),
                package_id: "gzip".to_string(),
                source: PackageSource::Pacman,
                version: "1.14".to_string(),
                size_bytes: 1000,
                install_path: "/var/lib/pacman/local/gzip".to_string(),
            },
            InstalledPackage {
                name: "org.gimp.GIMP".to_string(),
                package_id: "org.gimp.GIMP".to_string(),
                source: PackageSource::Flatpak,
                version: "stable".to_string(),
                size_bytes: 2000,
                install_path: "/var/lib/flatpak/app/org.gimp.GIMP".to_string(),
            },
        ];

        assert!(PackageMatcher::find_duplicates(&packages).is_empty());
    }

    #[test]
    fn version_comparison_recommends_unique_newer_copy() {
        let packages = vec![
            InstalledPackage { name: "gimp".into(), package_id: "gimp".into(), source: PackageSource::Pacman, version: "3.2.4-2".into(), size_bytes: 1, install_path: String::new() },
            InstalledPackage { name: "org.gimp.GIMP".into(), package_id: "org.gimp.GIMP".into(), source: PackageSource::Flatpak, version: "3.2.5".into(), size_bytes: 1, install_path: String::new() },
        ];
        assert_eq!(PackageMatcher::find_duplicates(&packages)[0].version_comparison.as_ref().unwrap().newer_source, PackageSource::Flatpak);
    }

    #[test]
    fn ambiguous_versions_do_not_get_a_recommendation() {
        let packages = vec![
            InstalledPackage { name: "gimp".into(), package_id: "gimp".into(), source: PackageSource::Pacman, version: "3.2.4".into(), size_bytes: 1, install_path: String::new() },
            InstalledPackage { name: "org.gimp.GIMP".into(), package_id: "org.gimp.GIMP".into(), source: PackageSource::Flatpak, version: "stable".into(), size_bytes: 1, install_path: String::new() },
        ];
        assert!(PackageMatcher::find_duplicates(&packages)[0].version_comparison.is_none());
    }
}
