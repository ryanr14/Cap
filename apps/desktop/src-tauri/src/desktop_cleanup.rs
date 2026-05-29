use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopIconAction {
    Hide,
    Show,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopIconUpdate {
    pub visible: bool,
    pub changed: bool,
}

#[cfg(target_os = "macos")]
pub fn apply_desktop_icon_action(action: DesktopIconAction) -> Result<DesktopIconUpdate, String> {
    let visible = match action {
        DesktopIconAction::Hide => false,
        DesktopIconAction::Show => true,
        DesktopIconAction::Toggle => !desktop_icons_visible()?,
    };

    set_desktop_icons_visible(visible)
}

#[cfg(not(target_os = "macos"))]
pub fn apply_desktop_icon_action(_action: DesktopIconAction) -> Result<DesktopIconUpdate, String> {
    Err("Desktop icon visibility automation is only available on macOS".to_string())
}

#[cfg(target_os = "macos")]
fn desktop_icons_visible() -> Result<bool, String> {
    let output = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "com.apple.finder", "CreateDesktop"])
        .output()
        .map_err(|err| format!("Failed to read Finder desktop icon setting: {err}"))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return parse_defaults_bool(&stdout).ok_or(format!(
            "Unexpected Finder desktop icon setting value: {}",
            stdout.trim()
        ));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("does not exist") {
        return Ok(true);
    }

    Err(format!(
        "Failed to read Finder desktop icon setting: {}",
        stderr.trim()
    ))
}

#[cfg(target_os = "macos")]
fn set_desktop_icons_visible(visible: bool) -> Result<DesktopIconUpdate, String> {
    let current = desktop_icons_visible()?;
    if current == visible {
        return Ok(DesktopIconUpdate {
            visible,
            changed: false,
        });
    }

    let bool_arg = if visible { "true" } else { "false" };
    let output = std::process::Command::new("/usr/bin/defaults")
        .args([
            "write",
            "com.apple.finder",
            "CreateDesktop",
            "-bool",
            bool_arg,
        ])
        .output()
        .map_err(|err| format!("Failed to write Finder desktop icon setting: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Failed to write Finder desktop icon setting: {}",
            stderr.trim()
        ));
    }

    restart_finder()?;

    Ok(DesktopIconUpdate {
        visible,
        changed: true,
    })
}

#[cfg(target_os = "macos")]
fn restart_finder() -> Result<(), String> {
    let output = std::process::Command::new("/usr/bin/killall")
        .arg("Finder")
        .output()
        .map_err(|err| format!("Failed to restart Finder: {err}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("No matching processes") {
        let status = std::process::Command::new("/usr/bin/open")
            .args(["-a", "Finder"])
            .status()
            .map_err(|err| format!("Failed to open Finder: {err}"))?;

        if status.success() {
            return Ok(());
        }
    }

    Err(format!("Failed to restart Finder: {}", stderr.trim()))
}

#[cfg(target_os = "macos")]
fn parse_defaults_bool(value: &str) -> Option<bool> {
    match value.trim().trim_matches('"').to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::DesktopIconAction;

    #[test]
    fn desktop_icon_action_serializes_to_snake_case() {
        let value = serde_json::to_string(&DesktopIconAction::Toggle).unwrap();

        assert_eq!(value, "\"toggle\"");
    }
}
