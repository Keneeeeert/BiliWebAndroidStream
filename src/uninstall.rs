//! Self-uninstall support.
//!
//! The extension can drive a full uninstall through the native messaging
//! channel: remove the host registration, the stored login state, and the
//! helper installation itself, so users never need to run a cleanup script
//! by hand.

use std::fs;
use std::path::PathBuf;

pub struct UninstallReport {
    pub manifest_removed: bool,
    pub token_removed: bool,
    pub binary_removed: bool,
}

pub fn run() -> UninstallReport {
    UninstallReport {
        manifest_removed: remove_manifest(),
        token_removed: remove_token_state(),
        binary_removed: remove_binary(),
    }
}

fn remove_manifest() -> bool {
    let mut removed = false;
    for path in manifest_paths() {
        if fs::remove_file(&path).is_ok() {
            removed = true;
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(path) = windows_manifest_path() {
            if fs::remove_file(&path).is_ok() {
                removed = true;
            }
        }
        removed |= remove_windows_registry_key();
    }
    removed
}

fn manifest_paths() -> Vec<PathBuf> {
    let name = "com.biliwebandroidstream.helper.json";
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        paths.push(home.join(".mozilla/native-messaging-hosts").join(name));
        paths.push(
            home.join("Library/Application Support/Mozilla/NativeMessagingHosts")
                .join(name),
        );
        paths.push(
            home.join(".var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts")
                .join(name),
        );
    }
    paths
}

#[cfg(target_os = "windows")]
fn windows_manifest_path() -> Option<PathBuf> {
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Mozilla\NativeMessagingHosts\com.biliwebandroidstream.helper",
            "/ve",
        ])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(index) = line.find("REG_SZ") {
            let path = line[index + "REG_SZ".len()..].trim();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn remove_windows_registry_key() -> bool {
    std::process::Command::new("reg")
        .args([
            "delete",
            r"HKCU\Software\Mozilla\NativeMessagingHosts\com.biliwebandroidstream.helper",
            "/f",
        ])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn remove_token_state() -> bool {
    let Some(dir) = token_state_dir() else {
        return false;
    };
    fs::remove_dir_all(dir).is_ok()
}

fn token_state_dir() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(path)
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".config")
    };
    Some(base.join("biliwebandroidstream"))
}

fn remove_binary() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let install_dir = exe.parent().map(PathBuf::from);

    #[cfg(unix)]
    {
        // Deleting a running binary's directory entry is allowed on Unix, so
        // the whole installation can go away in this pass.
        let mut removed = fs::remove_file(&exe).is_ok() || !exe.exists();
        if let Some(dir) = install_dir {
            removed &= fs::remove_dir_all(dir).is_ok();
        }
        removed
    }

    #[cfg(target_os = "windows")]
    {
        // A running image cannot be deleted, but renaming works. Rename and
        // schedule a detached cleanup that finishes once this process exits.
        use std::os::windows::process::CommandExt;
        let pending = exe.with_extension(format!("pending-delete-{}", std::process::id()));
        let renamed = fs::rename(&exe, &pending).is_ok();
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let dir = install_dir.unwrap_or_else(|| PathBuf::from("."));
        let script = format!(
            "timeout /t 2 /nobreak >nul & del /f /q \"{}\" & rd /s /q \"{}\"",
            pending.display(),
            dir.display()
        );
        let spawned = std::process::Command::new("cmd")
            .arg("/C")
            .arg(&script)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .is_ok();
        renamed || spawned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_paths_cover_canonical_firefox_locations() {
        unsafe {
            std::env::set_var("HOME", "/home/tester");
        }
        let paths = manifest_paths();
        assert!(paths.iter().any(|path| path
            .ends_with(".mozilla/native-messaging-hosts/com.biliwebandroidstream.helper.json")));
        assert!(paths
            .iter()
            .any(|path| path.ends_with("Library/Application Support/Mozilla/NativeMessagingHosts/com.biliwebandroidstream.helper.json")));
    }
}
