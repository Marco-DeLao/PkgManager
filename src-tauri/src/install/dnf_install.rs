use super::{SearchAdapter, SearchError, SearchResult};
use crate::models::PackageSource;

pub struct DnfSearchAdapter;

impl SearchAdapter for DnfSearchAdapter {
    fn source(&self) -> PackageSource { PackageSource::Rpm }
    fn is_available(&self) -> bool { std::process::Command::new("dnf").arg("--version").output().is_ok() }
    fn search(&self, query: &str) -> Result<Vec<SearchResult>, SearchError> {
        if query.trim().is_empty() { return Err(SearchError::EmptyQuery); }
        let output = std::process::Command::new("dnf").args(["search", query]).output().map_err(|e| SearchError::Command(e.to_string()))?;
        if !output.status.success() { return Err(SearchError::Command(super::command_detail(&output))); }
        let mut results = parse_dnf_search(&String::from_utf8_lossy(&output.stdout));
        let mut args = vec!["info".to_string(), "--".to_string()];
        args.extend(results.iter().map(|result| result.package_id.clone()));
        if let Ok(output) = std::process::Command::new("dnf").args(&args).output() {
            for result in &mut results {
                result.approx_size = find_dnf_size(&String::from_utf8_lossy(&output.stdout), &result.package_id);
            }
        }
        Ok(results)
    }
}

fn find_dnf_size(output: &str, package_id: &str) -> Option<u64> {
    let mut in_package = false;
    let requested_name = [".x86_64", ".noarch", ".aarch64", ".i686"].iter().find_map(|suffix| package_id.strip_suffix(suffix)).unwrap_or(package_id);
    for line in output.lines() {
        let line = line.trim_start();
        if let Some((field, value)) = line.split_once(':') {
            if field.trim() == "Name" { in_package = value.trim() == requested_name || value.trim() == package_id; }
            if in_package && (field.trim() == "Size" || field.trim() == "Download size") { return super::parse_size_value(value.trim()); }
        }
    }
    None
}

pub(crate) fn parse_dnf_search(output: &str) -> Vec<SearchResult> {
    output.lines().filter_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Matched fields:") { return None; }
        let (name, description) = if let Some((name, description)) = line.split_once(" : ") {
            (name.trim(), description.trim())
        } else {
            let separator = line.as_bytes().iter().position(|byte| byte.is_ascii_whitespace())?;
            (line[..separator].trim(), line[separator..].trim())
        };
        if name.is_empty() || description.is_empty() { return None; }
        Some(SearchResult { name: name.to_string(), display_name: name.to_string(), description: description.to_string(), source: PackageSource::Rpm, package_id: name.to_string(), approx_size: None, remote: None })
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_dnf_fixture_output() {
        let result = parse_dnf_search("gimp.x86_64 : GNU image editor\n");
        assert_eq!(result[0].package_id, "gimp.x86_64");
        assert_eq!(result[0].source, PackageSource::Rpm);
    }

    #[test]
    fn parses_current_dnf_aligned_search_output() {
        let results = parse_dnf_search("Matched fields: name (exact)\n bash.x86_64\tThe GNU Bourne Again shell\n");
        assert_eq!(results[0].package_id, "bash.x86_64");
        assert_eq!(results[0].description, "The GNU Bourne Again shell");
    }
}