use serde::Serialize;
use std::{env, path::PathBuf, process::Command};

#[derive(Debug, Serialize)]
struct RuntimeStatus {
    platform: String,
    architecture: String,
    java_available: bool,
    java_version: Option<String>,
    minecraft_directory: Option<String>,
}

fn detected_java_version() -> Option<String> {
    let output = Command::new("java").arg("-version").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );

    combined
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .map(String::from)
}

fn default_minecraft_directory() -> Option<String> {
    let home = if cfg!(target_os = "windows") {
        env::var_os("APPDATA").map(PathBuf::from)
    } else {
        env::var_os("HOME").map(PathBuf::from)
    }?;

    let path = if cfg!(target_os = "windows") {
        home.join(".minecraft")
    } else if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("minecraft")
    } else {
        home.join(".minecraft")
    };

    Some(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn get_runtime_status() -> RuntimeStatus {
    let java_version = detected_java_version();

    RuntimeStatus {
        platform: env::consts::OS.to_string(),
        architecture: env::consts::ARCH.to_string(),
        java_available: java_version.is_some(),
        java_version,
        minecraft_directory: default_minecraft_directory(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_runtime_status])
        .run(tauri::generate_context!())
        .expect("error while running NOVA Launcher");
}
