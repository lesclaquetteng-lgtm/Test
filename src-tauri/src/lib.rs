use serde::Serialize;
use std::{env, path::PathBuf, process::Command};
use tauri::{Emitter, Manager};

pub(crate) mod downloader;
pub(crate) mod diagnostics;
pub(crate) mod launch;
pub(crate) mod mojang;
pub(crate) mod natives;
pub(crate) mod performance;

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
async fn get_runtime_status() -> Result<RuntimeStatus, String> {
    let _operation = performance::global().track();

    tauri::async_runtime::spawn_blocking(|| {
        let java_version = detected_java_version();
        RuntimeStatus {
            platform: env::consts::OS.to_string(),
            architecture: env::consts::ARCH.to_string(),
            java_available: java_version.is_some(),
            java_version,
            minecraft_directory: default_minecraft_directory(),
        }
    })
    .await
    .map_err(|error| format!("Runtime diagnostic task failed: {error}"))
}

#[tauri::command]
fn get_performance_snapshot() -> performance::PerformanceSnapshot {
    performance::global().snapshot()
}

#[tauri::command]
async fn get_minecraft_versions(app: tauri::AppHandle) -> Result<mojang::VersionManifest, String> {
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Unable to resolve NOVA cache directory: {error}"))?;
    mojang::fetch_manifest(&cache_root).await
}

#[tauri::command]
async fn list_installed_instances(
    app: tauri::AppHandle,
) -> Result<Vec<mojang::InstalledInstance>, String> {
    let data_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Unable to resolve NOVA data directory: {error}"))?;
    mojang::list_installed_instances(&data_root).await
}

#[tauri::command]
async fn install_vanilla_version(
    app: tauri::AppHandle,
    version_id: String,
) -> Result<mojang::InstalledInstance, String> {
    let data_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Unable to resolve NOVA data directory: {error}"))?;
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Unable to resolve NOVA cache directory: {error}"))?;

    let (progress_sender, mut progress_receiver) =
        tokio::sync::mpsc::channel::<mojang::InstallProgress>(128);
    let progress_app = app.clone();
    let progress_task = tauri::async_runtime::spawn(async move {
        while let Some(progress) = progress_receiver.recv().await {
            let _ = progress_app.emit("nova-install-progress", progress);
        }
    });

    let result = mojang::install_vanilla(
        &data_root,
        &cache_root,
        &version_id,
        Some(progress_sender.clone()),
    )
    .await;
    drop(progress_sender);
    let _ = progress_task.await;
    result
}

#[tauri::command]
async fn launch_minecraft(
    app: tauri::AppHandle,
    instance_id: String,
    player_name: String,
    memory_mb: u32,
) -> Result<launch::LaunchSession, String> {
    let data_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Unable to resolve NOVA data directory: {error}"))?;
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Unable to resolve NOVA cache directory: {error}"))?;
    let result = launch::launch_instance(
        app.clone(),
        data_root.clone(),
        cache_root.clone(),
        instance_id.clone(),
        player_name,
        memory_mb,
    )
    .await;
    if result.is_err() {
        let _ = diagnostics::run_report(
            app,
            data_root,
            cache_root,
            Some(instance_id),
            false,
            "launch_preflight_failure".to_owned(),
        )
        .await;
    }
    result
}

#[tauri::command]
async fn run_diagnostics(
    app: tauri::AppHandle,
    instance_id: Option<String>,
    deep: bool,
) -> Result<diagnostics::DiagnosticReport, String> {
    let data_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Unable to resolve NOVA data directory: {error}"))?;
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Unable to resolve NOVA cache directory: {error}"))?;
    diagnostics::run_report(
        app,
        data_root,
        cache_root,
        instance_id,
        deep,
        if deep {
            "manual_deep_scan"
        } else {
            "manual_quick_scan"
        }
        .to_owned(),
    )
    .await
}

#[tauri::command]
fn open_diagnostics_folder(app: tauri::AppHandle) -> Result<(), String> {
    let data_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Unable to resolve NOVA data directory: {error}"))?;
    diagnostics::open_report_folder(&data_root)
}

#[tauri::command]
fn list_launch_sessions() -> Result<Vec<launch::LaunchSession>, String> {
    launch::list_sessions()
}

#[tauri::command]
fn stop_minecraft(instance_id: String) -> Result<(), String> {
    launch::stop_instance(&instance_id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_runtime_status,
            get_performance_snapshot,
            get_minecraft_versions,
            list_installed_instances,
            install_vanilla_version,
            launch_minecraft,
            run_diagnostics,
            open_diagnostics_folder,
            list_launch_sessions,
            stop_minecraft
        ])
        .run(tauri::generate_context!())
        .expect("error while running NOVA Launcher");
}
