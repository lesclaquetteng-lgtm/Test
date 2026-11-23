use crate::{launch, modrinth, mojang};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
use tokio::sync::Mutex;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CopySummary {
    files: usize,
    bytes: u64,
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn validate_display_name(value: &str) -> Result<String, String> {
    let name = value.trim();
    if name.is_empty()
        || name.chars().count() > 48
        || name.chars().any(char::is_control)
    {
        return Err("Instance names must contain 1–48 visible characters".to_owned());
    }
    Ok(name.to_owned())
}

fn validate_jvm_arguments(arguments: &[String]) -> Result<Vec<String>, String> {
    if arguments.len() > 32 {
        return Err("A maximum of 32 custom JVM arguments is allowed".to_owned());
    }
    let mut validated = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let value = argument.trim();
        if value.is_empty()
            || value.len() > 512
            || value.chars().any(char::is_control)
            || value.starts_with('@')
            || value == "-cp"
            || value == "-classpath"
            || value.starts_with("-Xmx")
            || value.starts_with("-Xms")
            || value.starts_with("-Djava.library.path")
            || value.starts_with("-Duser.dir")
            || value.starts_with("-Djava.system.class.loader")
            || value.starts_with("-javaagent")
            || value.starts_with("-agentlib")
            || value.starts_with("-agentpath")
        {
            return Err(format!("Unsafe or reserved JVM argument: {value}"));
        }
        validated.push(value.to_owned());
    }
    Ok(validated)
}

fn instance_root(data_root: &Path, instance_id: &str) -> Result<PathBuf, String> {
    if !safe_id(instance_id) {
        return Err("Invalid instance identifier".to_owned());
    }
    Ok(data_root.join("instances").join(instance_id))
}

async fn read_marker(
    data_root: &Path,
    instance_id: &str,
) -> Result<(PathBuf, mojang::InstalledInstance), String> {
    let root = instance_root(data_root, instance_id)?;
    let marker_path = root.join(".nova-install.json");
    let metadata = fs::symlink_metadata(&marker_path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("Instance marker is not a regular file".to_owned());
    }
    let bytes = tokio::fs::read(&marker_path)
        .await
        .map_err(|error| error.to_string())?;
    let marker: mojang::InstalledInstance =
        serde_json::from_slice(&bytes).map_err(|error| format!("Invalid instance marker: {error}"))?;
    if marker.id != instance_id || marker.instance_path != root.to_string_lossy() {
        return Err("Instance marker does not match its NOVA-managed directory".to_owned());
    }
    if marker.client_sha1.len() != 40
        || !marker.client_sha1.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Instance marker has an invalid Minecraft client checksum".to_owned());
    }
    Ok((root, marker))
}

fn write_marker(root: &Path, marker: &mojang::InstalledInstance) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(marker).map_err(|error| error.to_string())?;
    let destination = root.join(".nova-install.json");
    let temporary = root.join(".nova-install.json.new");
    let backup = root.join(".nova-install.json.backup");
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;

    if backup.exists() {
        fs::remove_file(&backup).map_err(|error| error.to_string())?;
    }
    if destination.exists() {
        fs::rename(&destination, &backup).map_err(|error| error.to_string())?;
        if let Err(error) = fs::rename(&temporary, &destination) {
            let rollback = fs::rename(&backup, &destination);
            let _ = fs::remove_file(&temporary);
            if let Err(rollback_error) = rollback {
                return Err(format!(
                    "Unable to save instance metadata ({error}); rollback also failed ({rollback_error})"
                ));
            }
            return Err(error.to_string());
        }
        let _ = fs::remove_file(&backup);
    } else {
        fs::rename(&temporary, &destination).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path, canonical_root: &Path) -> Result<CopySummary, String> {
    let metadata = fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!("Refusing to copy a non-directory or linked path: {}", source.display()));
    }
    let canonical_source = fs::canonicalize(source).map_err(|error| error.to_string())?;
    if !canonical_source.starts_with(canonical_root) {
        return Err("A source path resolves outside the selected instance".to_owned());
    }
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    let mut summary = CopySummary::default();

    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let entry_type = entry.file_type().map_err(|error| error.to_string())?;
        if entry_type.is_symlink() {
            return Err(format!("Instance contains a symbolic link: {}", source_path.display()));
        }
        let canonical_entry = fs::canonicalize(&source_path).map_err(|error| error.to_string())?;
        if !canonical_entry.starts_with(canonical_root) {
            return Err("A copied path resolves outside the selected instance".to_owned());
        }

        if entry_type.is_dir() {
            let child = copy_tree(&source_path, &destination_path, canonical_root)?;
            summary.files = summary.files.saturating_add(child.files);
            summary.bytes = summary.bytes.saturating_add(child.bytes);
        } else if entry_type.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let copied = fs::copy(&source_path, &destination_path).map_err(|error| error.to_string())?;
            summary.files = summary.files.saturating_add(1);
            summary.bytes = summary.bytes.saturating_add(copied);
        } else {
            return Err(format!("Unsupported file type in instance: {}", source_path.display()));
        }
    }
    Ok(summary)
}

fn copy_folder_contents(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.exists() {
        return Err(format!("Required source folder is missing: {}", source.display()));
    }
    let metadata = fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("A required Minecraft data directory is not a regular directory".to_owned());
    }
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    let canonical_root = fs::canonicalize(source).map_err(|error| error.to_string())?;
    copy_tree(source, destination, &canonical_root)?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn rename_instance(
    app: tauri::AppHandle,
    instance_id: String,
    display_name: String,
) -> Result<mojang::InstalledInstance, String> {
    let data_root = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let (root, mut marker) = read_marker(&data_root, &instance_id).await?;
    marker.display_name = Some(validate_display_name(&display_name)?);
    write_marker(&root, &marker)?;
    Ok(marker)
}

#[tauri::command]
pub(crate) async fn duplicate_instance(
    app: tauri::AppHandle,
    instance_id: String,
    new_instance_id: String,
    display_name: String,
) -> Result<mojang::InstalledInstance, String> {
    let data_root = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let (source, source_marker) = read_marker(&data_root, &instance_id).await?;
    let destination = instance_root(&data_root, &new_instance_id)?;
    let display_name = validate_display_name(&display_name)?;
    if instance_id == new_instance_id {
        return Err("The duplicated profile must have a different identifier".to_owned());
    }
    if tokio::fs::try_exists(&destination).await.map_err(|error| error.to_string())? {
        return Err(format!("An instance named {new_instance_id} already exists"));
    }

    let instances_root = data_root.join("instances");
    tokio::fs::create_dir_all(&instances_root).await.map_err(|error| error.to_string())?;
    let temporary = instances_root.join(format!(".nova-copy-{new_instance_id}-{}", unix_now()));
    let source_for_copy = source.clone();
    let temporary_for_copy = temporary.clone();
    let summary = tokio::task::spawn_blocking(move || {
        let canonical = fs::canonicalize(&source_for_copy).map_err(|error| error.to_string())?;
        copy_tree(&source_for_copy, &temporary_for_copy, &canonical)
    })
    .await
    .map_err(|error| format!("Instance copy worker failed: {error}"))??;

    let mut marker = source_marker;
    marker.id = new_instance_id.clone();
    marker.display_name = Some(display_name);
    marker.instance_path = destination.to_string_lossy().into_owned();
    marker.installed_at_unix = unix_now();
    marker.total_bytes = summary.bytes.max(marker.total_bytes);
    marker.files = marker.files.max(summary.files);
    if let Err(error) = write_marker(&temporary, &marker) {
        let _ = tokio::fs::remove_dir_all(&temporary).await;
        return Err(error);
    }
    if let Err(error) = tokio::fs::rename(&temporary, &destination).await {
        let _ = tokio::fs::remove_dir_all(&temporary).await;
        return Err(format!("Unable to publish duplicated instance: {error}"));
    }
    marker.instance_path = destination.to_string_lossy().into_owned();
    // The path is already written to the temporary marker before the atomic rename.
    Ok(marker)
}

#[tauri::command]
pub(crate) async fn delete_instance(
    app: tauri::AppHandle,
    instance_id: String,
) -> Result<(), String> {
    if launch::is_instance_running(&instance_id)? {
        return Err("Stop Minecraft before deleting this instance".to_owned());
    }
    let data_root = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let (root, _) = read_marker(&data_root, &instance_id).await?;
    let instances_root = data_root.join("instances");
    let canonical_root = fs::canonicalize(&instances_root).map_err(|error| error.to_string())?;
    let canonical_instance = fs::canonicalize(&root).map_err(|error| error.to_string())?;
    if !canonical_instance.starts_with(&canonical_root) || canonical_instance == canonical_root {
        return Err("Refusing to delete a path outside NOVA's instance directory".to_owned());
    }
    let metadata = fs::symlink_metadata(&root).map_err(|error| error.to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("Refusing to delete a linked or non-directory instance".to_owned());
    }
    tokio::fs::remove_dir_all(root).await.map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn update_instance_settings(
    app: tauri::AppHandle,
    instance_id: String,
    memory_mb: Option<u32>,
    jvm_arguments: Option<Vec<String>>,
) -> Result<mojang::InstalledInstance, String> {
    let data_root = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let (root, mut marker) = read_marker(&data_root, &instance_id).await?;
    if let Some(memory) = memory_mb {
        if !(1024..=32768).contains(&memory) {
            return Err("Instance memory must be between 1024 MB and 32768 MB".to_owned());
        }
        marker.memory_mb = Some(memory);
    } else {
        marker.memory_mb = None;
    }
    if let Some(arguments) = jvm_arguments {
        marker.jvm_arguments = Some(validate_jvm_arguments(&arguments)?);
    } else {
        marker.jvm_arguments = None;
    }
    write_marker(&root, &marker)?;
    Ok(marker)
}

#[tauri::command]
pub(crate) async fn repair_instance(
    app: tauri::AppHandle,
    instance_id: String,
) -> Result<mojang::InstalledInstance, String> {
    if launch::is_instance_running(&instance_id)? {
        return Err("Stop Minecraft before repairing this instance".to_owned());
    }
    let data_root = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let cache_root = app.path().app_cache_dir().map_err(|error| error.to_string())?;
    let (target_root, original) = read_marker(&data_root, &instance_id).await?;
    let game_version = original.game_version.clone().unwrap_or_else(|| original.id.clone());
    if !safe_id(&game_version) {
        return Err("This profile has an invalid base Minecraft version".to_owned());
    }

    // Re-run the official Vanilla installer to backfill missing client, asset, library and native files.
    let repaired_base = mojang::install_vanilla(&data_root, &cache_root, &game_version, None).await?;
    if instance_id != game_version {
        let source_root = data_root.join("instances").join(&game_version);
        let source_version = source_root.join("versions").join(&game_version);
        let target_version = target_root.join("versions").join(&game_version);
        let source_natives = source_root.join("natives");
        let target_natives = target_root.join("natives");

        let copy_source = source_version.clone();
        let copy_target = target_version.clone();
        tokio::task::spawn_blocking(move || copy_folder_contents(&copy_source, &copy_target))
            .await
            .map_err(|error| format!("Minecraft version repair worker failed: {error}"))??;
        let native_source = source_natives.clone();
        let native_target = target_natives.clone();
        tokio::task::spawn_blocking(move || copy_folder_contents(&native_source, &native_target))
            .await
            .map_err(|error| format!("Native library repair worker failed: {error}"))??;
    }

    let mut marker = original.clone();
    marker.client_sha1 = repaired_base.client_sha1;
    marker.asset_index = repaired_base.asset_index;
    marker.version_type = repaired_base.version_type;
    if marker.loader.as_deref() == Some("fabric") {
        marker = modrinth::repair_fabric_loader(&data_root, &instance_id, marker).await?;
    } else {
        write_marker(&target_root, &marker)?;
    }
    Ok(marker)
}

#[tauri::command]
pub(crate) fn open_instance_folder(
    app: tauri::AppHandle,
    instance_id: String,
) -> Result<(), String> {
    let data_root = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let root = instance_root(&data_root, &instance_id)?;
    let marker = fs::read(root.join(".nova-install.json")).map_err(|error| error.to_string())?;
    let parsed: mojang::InstalledInstance =
        serde_json::from_slice(&marker).map_err(|error| error.to_string())?;
    if parsed.id != instance_id || parsed.instance_path != root.to_string_lossy() {
        return Err("Instance marker does not match its directory".to_owned());
    }
    let mut command = if cfg!(windows) {
        let mut command = Command::new("explorer.exe");
        command.arg(&root);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(&root);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(&root);
        command
    };
    command.spawn().map(|_| ()).map_err(|error| format!("Unable to open instance folder: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{safe_id, validate_display_name, validate_jvm_arguments};

    #[test]
    fn validates_instance_identifiers_and_names() {
        assert!(safe_id("copy-1.21.1"));
        assert!(!safe_id("../outside"));
        assert!(validate_display_name("My Modpack").is_ok());
        assert!(validate_display_name("  ").is_err());
        assert!(validate_display_name(&"x".repeat(49)).is_err());
    }

    #[test]
    fn rejects_reserved_or_dangerous_jvm_arguments() {
        assert!(validate_jvm_arguments(&["-XX:+UseG1GC".to_owned()]).is_ok());
        assert!(validate_jvm_arguments(&["-Xmx32G".to_owned()]).is_err());
        assert!(validate_jvm_arguments(&["@args.txt".to_owned()]).is_err());
        assert!(validate_jvm_arguments(&["-javaagent:agent.jar".to_owned()]).is_err());
    }
}
