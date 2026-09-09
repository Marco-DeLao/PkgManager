use super::{SearchAdapter, SearchError, SearchResult};
use crate::models::PackageSource;

pub struct AptSearchAdapter;

impl SearchAdapter for AptSearchAdapter {
    fn source(&self) -> PackageSource { PackageSource::Apt }
    fn is_available(&self) -> bool { std::process::Command::new("apt-cache").arg("--version").output().is_ok() }
    fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        if query.trim().is_empty() { return Err(SearchError::EmptyQuery); }
        let output = std::process::Command::new("apt-cache").args(["search", query]).output().map_err(|e| SearchError::Command(e.to_string()))?;
        if !output.status.success() { return Err(SearchError::Command(super::command_detail(&output))); }
        let mut results = parse_apt_search(&String::from_utf8_lossy(&output.stdout));
        let mut args = vec!["show".to_string()];
        args.extend(results.iter().map(|result| result.package_id.clone()));
        if let Ok(output) = std::process::Command::new("apt-cache").args(&args).output() {
            apply_apt_sizes(&mut results, &String::from_utf8_lossy(&output.stdout));
        }
        Ok(results)
    }
}

fn apply_apt_sizes(results: &mut [SearchResult], metadata: &str) {
    let mut package = None;
    let mut size = None;
    let mut installed_size = None;
    let mut apply = |package: &mut Option<String>, size: &mut Option<u64>, installed_size: &mut Option<u64>| {
        if let Some(id) = package.take() {
            if let Some(result) = results.iter_mut().find(|result| result.package_id == id) {
                result.approx_size = size.take().or_else(|| installed_size.take().map(|value| value * 1024));
            }
        }
        *size = None;
        *installed_size = None;
    };
    for line in metadata.lines().chain(std::iter::once("")) {
        if line.is_empty() { apply(&mut package, &mut size, &mut installed_size); continue; }
        if let Some(value) = line.strip_prefix("Package: ") { apply(&mut package, &mut size, &mut installed_size); package = Some(value.trim().to_string()); }
        else if let Some(value) = line.strip_prefix("Size: ") { size = super::parse_size_value(value.trim()); }
        else if let Some(value) = line.strip_prefix("Installed-Size: ") { installed_size = value.trim().parse::<u64>().ok(); }
    }
}

pub(crate) fn parse_apt_search(output: &str) -> Vec<SearchResult> {
    output.lines().filter_map(|line| {
        let (name, description) = line.split_once(" - ")?;
        Some(SearchResult { name: name.trim().to_string(), display_name: name.trim().to_string(), description: description.trim().to_string(), source: PackageSource::Apt, package_id: name.trim().to_string(), approx_size: None, remote: None })
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_apt_fixture_output() {
        let result = parse_apt_search("gimp - GNU image editor\n");
        assert_eq!(result[0].package_id, "gimp");
        assert_eq!(result[0].source, PackageSource::Apt);
    }
}