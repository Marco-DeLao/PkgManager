/// Test suite for the package duplicate detector
#[cfg(test)]
mod tests {
    use app_lib::models::{InstalledPackage, PackageSource};
    use app_lib::matcher::PackageMatcher;

    #[test]
    fn test_find_duplicates_firefox() {
        let packages = vec![
            // Firefox from APT
            InstalledPackage {
                name: "firefox".to_string(),
                package_id: "firefox".to_string(),
                source: PackageSource::Apt,
                version: "91.0".to_string(),
                size_bytes: 100_000_000,
                install_path: "/usr/lib/firefox".to_string(),
            },
            // Firefox from Flatpak
            InstalledPackage {
                name: "org.mozilla.firefox".to_string(),
                package_id: "org.mozilla.firefox".to_string(),
                source: PackageSource::Flatpak,
                version: "91.0".to_string(),
                size_bytes: 200_000_000,
                install_path: "/var/lib/flatpak/app/org.mozilla.firefox".to_string(),
            },
            // Firefox from Snap
            InstalledPackage {
                name: "firefox".to_string(),
                package_id: "firefox".to_string(),
                source: PackageSource::Snap,
                version: "91.0".to_string(),
                size_bytes: 150_000_000,
                install_path: "/var/lib/snapd/snaps/firefox".to_string(),
            },
        ];

        let duplicates = PackageMatcher::find_duplicates(&packages);
        
        assert_eq!(duplicates.len(), 1, "Should find one duplicate group");
        assert_eq!(duplicates[0].packages.len(), 3, "Should have 3 copies of Firefox");
        assert_eq!(duplicates[0].total_size_bytes, 450_000_000);
        assert_eq!(duplicates[0].wasted_size_bytes, 350_000_000); // 450M - 100M (smallest)
    }

    #[test]
    fn test_find_duplicates_gimp() {
        let packages = vec![
            // GIMP from APT
            InstalledPackage {
                name: "gimp".to_string(),
                package_id: "gimp".to_string(),
                source: PackageSource::Apt,
                version: "2.10.30".to_string(),
                size_bytes: 150_000_000,
                install_path: "/usr/lib/gimp".to_string(),
            },
            // GIMP from Flatpak
            InstalledPackage {
                name: "org.gimp.GIMP".to_string(),
                package_id: "org.gimp.GIMP".to_string(),
                source: PackageSource::Flatpak,
                version: "2.10.30".to_string(),
                size_bytes: 250_000_000,
                install_path: "/var/lib/flatpak/app/org.gimp.GIMP".to_string(),
            },
        ];

        let duplicates = PackageMatcher::find_duplicates(&packages);
        
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].packages.len(), 2);
        // Total = 150M + 250M = 400M, wasted = 400M - 150M (smallest) = 250M
        assert_eq!(duplicates[0].wasted_size_bytes, 250_000_000);
    }

    #[test]
    fn test_no_duplicates_different_apps() {
        let packages = vec![
            InstalledPackage {
                name: "firefox".to_string(),
                package_id: "firefox".to_string(),
                source: PackageSource::Apt,
                version: "91.0".to_string(),
                size_bytes: 100_000_000,
                install_path: "/usr/lib/firefox".to_string(),
            },
            InstalledPackage {
                name: "chromium".to_string(),
                package_id: "chromium".to_string(),
                source: PackageSource::Apt,
                version: "92.0".to_string(),
                size_bytes: 120_000_000,
                install_path: "/usr/lib/chromium".to_string(),
            },
        ];

        let duplicates = PackageMatcher::find_duplicates(&packages);
        
        assert_eq!(duplicates.len(), 0, "Should not find duplicates for different apps");
    }

    #[test]
    fn test_fuzzy_matching_vim() {
        let packages = vec![
            // Vim from APT
            InstalledPackage {
                name: "vim".to_string(),
                package_id: "vim".to_string(),
                source: PackageSource::Apt,
                version: "8.2".to_string(),
                size_bytes: 30_000_000,
                install_path: "/usr/bin/vim".to_string(),
            },
            // Vim from Flatpak (with different naming)
            InstalledPackage {
                name: "vim-gtk".to_string(),
                package_id: "com.example.vim".to_string(),
                source: PackageSource::Flatpak,
                version: "8.2".to_string(),
                size_bytes: 40_000_000,
                install_path: "/var/lib/flatpak/app/com.example.vim".to_string(),
            },
        ];

        let duplicates = PackageMatcher::find_duplicates(&packages);
        
        assert_eq!(duplicates.len(), 1, "Should fuzzy-match vim packages");
        assert_eq!(duplicates[0].packages.len(), 2);
    }

    #[test]
    fn test_same_source_not_duplicates() {
        let packages = vec![
            InstalledPackage {
                name: "libc".to_string(),
                package_id: "glibc".to_string(),
                source: PackageSource::Apt,
                version: "2.35".to_string(),
                size_bytes: 5_000_000,
                install_path: "/lib".to_string(),
            },
            InstalledPackage {
                name: "libc++".to_string(),
                package_id: "libcxx".to_string(),
                source: PackageSource::Apt,
                version: "14.0".to_string(),
                size_bytes: 2_000_000,
                install_path: "/lib".to_string(),
            },
        ];

        let duplicates = PackageMatcher::find_duplicates(&packages);
        
        assert_eq!(duplicates.len(), 0, "Packages from same source are not duplicates");
    }
}
