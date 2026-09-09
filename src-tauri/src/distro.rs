use std::fs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistroInfo {
    pub id: String,
    pub id_like: Vec<String>,
}

impl DistroInfo {
    pub fn detect() -> Self {
        let content = fs::read_to_string("/etc/os-release").unwrap_or_default();
        let values = content
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key, value.trim_matches('"').to_string()))
            .collect::<std::collections::HashMap<_, _>>();
        let id = values.get("ID").cloned().unwrap_or_default();
        let id_like = values
            .get("ID_LIKE")
            .map(|value| value.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default();
        Self { id, id_like }
    }

    pub fn native_source(&self) -> Option<crate::models::PackageSource> {
        let matches = |name: &str| self.id == name || self.id_like.iter().any(|item| item == name);
        if matches("arch") { Some(crate::models::PackageSource::Pacman) }
        else if matches("debian") || matches("ubuntu") { Some(crate::models::PackageSource::Apt) }
        else if matches("fedora") || matches("rhel") { Some(crate::models::PackageSource::Rpm) }
        else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch_family_selects_pacman() {
        let distro = DistroInfo { id: "endeavouros".into(), id_like: vec!["arch".into()] };
        assert_eq!(distro.native_source(), Some(crate::models::PackageSource::Pacman));
    }
}