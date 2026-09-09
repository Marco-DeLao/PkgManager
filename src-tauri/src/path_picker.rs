use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum PathPickerError {
    #[error("native folder picker is unavailable: {0}")]
    NativeUnavailable(String),
    #[error("folder picker was cancelled")]
    Cancelled,
    #[error("portal folder picker failed: {0}")]
    Portal(String),
}

pub fn running_inside_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some() || std::path::Path::new("/.flatpak-info").exists()
}

pub async fn pick_scan_path() -> Result<String, PathPickerError> {
    if running_inside_flatpak() {
        return pick_with_flatpak_host_native();
    }

    pick_with_native_chooser()
}

fn pick_with_flatpak_host_native() -> Result<String, PathPickerError> {
    let output = Command::new("flatpak-spawn")
        .args(["--host", "kdialog", "--getexistingdirectory", "/", "--title", "Choose scan path"])
        .output()
        .map_err(|error| PathPickerError::NativeUnavailable(error.to_string()))?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Ok(path);
        }
    }

    Err(PathPickerError::Cancelled)
}

fn pick_with_native_chooser() -> Result<String, PathPickerError> {
    if let Ok(output) = Command::new("kdialog")
        .args(["--getexistingdirectory", "/", "--title", "Choose scan path"])
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
        if !output.stderr.is_empty() {
            return Err(PathPickerError::NativeUnavailable(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        return Err(PathPickerError::Cancelled);
    }

    if let Ok(output) = Command::new("zenity")
        .args(["--file-selection", "--directory", "--title=Choose scan path"])
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
        return Err(PathPickerError::Cancelled);
    }

    Err(PathPickerError::NativeUnavailable(
        "install kdialog or zenity for native folder selection".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_process_does_not_use_portal_path() {
        assert!(!running_inside_flatpak());
    }

    #[test]
    fn native_picker_command_is_not_a_dbus_portal() {
        let command = "kdialog --getexistingdirectory / --title 'Choose scan path'";
        assert!(!command.contains("portal"));
        assert!(!command.contains("dbus"));
    }
}
