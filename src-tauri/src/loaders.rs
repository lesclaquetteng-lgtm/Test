use crate::{
    downloader::{download_verified, DownloadSpec},
    launch,
    mojang::InstalledInstance,
};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};
use tauri::Manager;
use tokio::{fs, process::Command};
use url::Url;

const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
const FORGE_PROMOTIONS: &str = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";
const MAX_METADATA_BYTES: usize = 8 * 1024 * 1024;
const MAX_INSTALLER_BYTES: u64 = 512 * 1024 * 1024;
const INSTALLER_TIMEOUT: Duration = Duration::from_secs(20 * 60);

static CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();

fn client() -> Result<&'static Client, String> {
    match CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(90))
            .user_agent(concat!(
                "NOVA-Launcher/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/lesclaquetteng-lgtm/Test)"
            ))
            .build()
            .map_err(|error| error.to_string())
    }) {
        Ok(client) => Ok(client),
        Err(error) => Err(error.clone()),
    }
}

fn valid_sha1(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'))
}

fn supported_minecraft_version(value: &str) -> bool {
    if !safe_token(value) {
        return false;
    }
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() >= 2
        && parts
            .iter()
            .take(3)
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

async fn fetch_limited(raw_url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let url = Url::parse(raw_url).map_err(|error| error.to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("Loader metadata and installers must use HTTPS without URL credentials".to_owned());
    }
    let response = client()?
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Loader service returned HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err("Loader response exceeded NOVA's size limit".to_owned());
    }
    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err("Loader response exceeded NOVA's size limit".to_owned());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

async fn fetch_text(url: &str) -> Result<String, String> {
    String::from_utf8(fetch_limited(url, MAX_METADATA_BYTES).await?)
        .map_err(|_| "Loader metadata is not valid UTF-8".to_owned())
}

fn xml_versions(document: &str) -> Vec<String> {
    let mut versions = Vec::new();
    let mut rest = document;
    while let Some(start) = rest.find("<version>") {
        rest = &rest[start + "<version>".len()..];
        let Some(end) = rest.find("</version>") else {
            break;
        };
        let version = rest[..end].trim();
        if safe_token(version) {
            versions.push(version.to_owned());
        }
        rest = &rest[end + "</version>".len()..];
    }
    versions
}

fn version_key(value: &str) -> Vec<(u8, String)> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut numeric: Option<bool> = None;
    for character in value.chars() {
        if !character.is_ascii_alphanumeric() {
            if !current.is_empty() {
                parts.push((u8::from(numeric == Some(false)), current.to_ascii_lowercase()));
                current.clear();
            }
            numeric = None;
            continue;
        }
        let is_numeric = character.is_ascii_digit();
        if numeric.is_some_and(|previous| previous != is_numeric) && !current.is_empty() {
            parts.push((u8::from(numeric == Some(false)), current.to_ascii_lowercase()));
            current.clear();
        }
        numeric = Some(is_numeric);
        current.push(character);
    }
    if !current.is_empty() {
        parts.push((u8::from(numeric == Some(false)), current.to_ascii_lowercase()));
    }
    parts
}

fn is_stable(version: &str) -> bool {
    let lower = version.to_ascii_lowercase();
    !["alpha", "beta", "rc", "snapshot", "pre", "dev"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn sort_releases(releases: &mut [LoaderRelease]) {
    releases.sort_by(|left, right| {
        right
            .stable
            .cmp(&left.stable)
            .then_with(|| version_key(&right.version).cmp(&version_key(&left.version)))
    });
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoaderRelease {
    pub loader: String,
    pub game_version: String,
    pub version: String,
    pub display_version: String,
    pub stable: bool,
}

fn forge_installer_url(game_version: &str, build: &str) -> String {
    let full = format!("{game_version}-{build}");
    format!(
        "{FORGE_MAVEN}/net/minecraftforge/forge/{full}/forge-{full}-installer.jar"
    )
}

fn neoforge_artifact(game_version: &str, version: &str) -> (String, String, String) {
    if game_version == "1.20.1" {
        let full = if version.starts_with("1.20.1-") {
            version.to_owned()
        } else {
            format!("1.20.1-{version}")
        };
        let path = format!(
            "net/neoforged/forge/{full}/forge-{full}-installer.jar"
        );
        return (NEOFORGE_MAVEN.to_owned(), path, full);
    }
    let path = format!(
        "net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar"
    );
    (
        NEOFORGE_MAVEN.to_owned(),
        path,
        version.to_owned(),
    )
}

fn neoforge_prefix(game_version: &str) -> Option<String> {
    if game_version.starts_with("1.") {
        let mut parts = game_version.split('.');
        let _one = parts.next()?;
        let major = parts.next()?;
        let minor = parts.next()?;
        if !major.bytes().all(|byte| byte.is_ascii_digit())
            || !minor.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        return Some(format!("{major}.{minor}."));
    }
    if game_version.starts_with("26.") {
        let parts = game_version.split('.').take(3).collect::<Vec<_>>();
        if parts.len() < 2 {
            return None;
        }
        return Some(format!("{}.", parts.join(".")));
    }
    None
}

async fn list_forge(game_version: &str) -> Result<Vec<LoaderRelease>, String> {
    let metadata = fetch_text(&format!("{FORGE_MAVEN}/net/minecraftforge/forge/maven-metadata.xml")).await?;
    let prefix = format!("{game_version}-");
    let mut releases = Vec::new();
    for full in xml_versions(&metadata) {
        if let Some(build) = full.strip_prefix(&prefix) {
            if safe_token(build) {
                releases.push(LoaderRelease {
                    loader: "forge".to_owned(),
                    game_version: game_version.to_owned(),
                    version: full,
                    display_version: build.to_owned(),
                    stable: is_stable(build),
                });
            }
        }
    }

    // The promotions manifest can name a supported build even if Maven metadata is briefly stale.
    let promotions: Value = serde_json::from_slice(
        &fetch_limited(FORGE_PROMOTIONS, MAX_METADATA_BYTES).await?,
    )
    .map_err(|error| format!("Forge promotions JSON is invalid: {error}"))?;
    if let Some(promos) = promotions.get("promos").and_then(Value::as_object) {
        for channel in ["recommended", "latest"] {
            if let Some(build) = promos
                .get(&format!("{game_version}-{channel}"))
                .and_then(Value::as_str)
                .filter(|value| safe_token(value))
            {
                let full = format!("{game_version}-{build}");
                if !releases.iter().any(|release| release.version == full) {
                    releases.push(LoaderRelease {
                        loader: "forge".to_owned(),
                        game_version: game_version.to_owned(),
                        version: full,
                        display_version: build.to_owned(),
                        stable: channel == "recommended" || is_stable(build),
                    });
                }
            }
        }
    }
    sort_releases(&mut releases);
    releases.truncate(40);
    Ok(releases)
}

async fn list_neoforge(game_version: &str) -> Result<Vec<LoaderRelease>, String> {
    let (artifact, prefix) = if game_version == "1.20.1" {
        (
            "forge",
            "1.20.1-".to_owned(),
        )
    } else {
        (
            "neoforge",
            neoforge_prefix(game_version).ok_or_else(|| {
                format!("NOVA does not yet map NeoForge's version scheme for Minecraft {game_version}")
            })?,
        )
    };
    let url = format!("{NEOFORGE_MAVEN}/net/neoforged/{artifact}/maven-metadata.xml");
    let metadata = fetch_text(&url).await?;
    let mut releases = Vec::new();
    for version in xml_versions(&metadata) {
        if version.starts_with(&prefix) {
            let display = if game_version == "1.20.1" {
                version.strip_prefix("1.20.1-").unwrap_or(&version).to_owned()
            } else {
                version.clone()
            };
            releases.push(LoaderRelease {
                loader: "neoforge".to_owned(),
                game_version: game_version.to_owned(),
                stable: is_stable(&version),
                version,
                display_version: display,
            });
        }
    }
    sort_releases(&mut releases);
    releases.truncate(40);
    Ok(releases)
}

#[tauri::command]
pub(crate) async fn list_loader_versions(
    game_version: String,
    loader: String,
) -> Result<Vec<LoaderRelease>, String> {
    if !supported_minecraft_version(&game_version) {
        return Err("Invalid Minecraft version".to_owned());
    }
    match loader.as_str() {
        "forge" => {
            if game_version.starts_with("1.") {
                let parts = game_version.split('.').collect::<Vec<_>>();
                let minor = parts.get(1).and_then(|value| value.parse::<u32>().ok()).unwrap_or(0);
                if minor < 13 {
                    return Err("This Forge installer pipeline supports Minecraft 1.13 and newer only.".to_owned());
                }
            }
            list_forge(&game_version).await
        }
        "neoforge" => list_neoforge(&game_version).await,
        _ => Err("Unsupported mod loader".to_owned()),
    }
}

fn installer_artifact(
    loader: &str,
    game_version: &str,
    selected_version: &str,
) -> Result<(String, String), String> {
    match loader {
        "forge" => {
            let prefix = format!("{game_version}-");
            let build = selected_version
                .strip_prefix(&prefix)
                .ok_or_else(|| "Selected Forge version does not match this Minecraft version".to_owned())?;
            if !safe_token(build) {
                return Err("Invalid Forge installer version".to_owned());
            }
            let full = format!("{game_version}-{build}");
            let file = format!("forge-{full}-installer.jar");
            Ok((
                format!("{FORGE_MAVEN}/net/minecraftforge/forge/{full}/{file}"),
                file,
            ))
        }
        "neoforge" => {
            let (base, path, full) = neoforge_artifact(game_version, selected_version);
            if !safe_token(&full) || path.contains("..") {
                return Err("Invalid NeoForge installer version".to_owned());
            }
            Ok((format!("{base}/{path}"), path.rsplit('/').next().unwrap_or_default().to_owned()))
        }
        _ => Err("Unsupported mod loader".to_owned()),
    }
}

fn profile_matches(value: &Value, game_version: &str, loader: &str) -> bool {
    let inherited = value.get("inheritsFrom").and_then(Value::as_str);
    let main = value.get("mainClass").and_then(Value::as_str).unwrap_or_default();
    let version_id = value.get("id").and_then(Value::as_str).unwrap_or_default();
    let lower = format!("{main} {version_id}").to_ascii_lowercase();
    inherited == Some(game_version)
        && !main.is_empty()
        && match loader {
            "forge" => lower.contains("forge") || lower.contains("fml"),
            "neoforge" => lower.contains("forge") || lower.contains("neoforge") || lower.contains("bootstraplauncher"),
            _ => false,
        }
}

async fn find_loader_profile(
    instance_root: &Path,
    game_version: &str,
    loader: &str,
) -> Result<(String, PathBuf, Value), String> {
    let versions_root = instance_root.join("versions");
    let mut entries = fs::read_dir(&versions_root).await.map_err(|error| error.to_string())?;
    let mut matches = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|error| error.to_string())? {
        if !entry.file_type().await.map_err(|error| error.to_string())?.is_dir() {
            continue;
        }
        let folder_name = entry.file_name().to_string_lossy().into_owned();
        if !safe_token(&folder_name) {
            continue;
        }
        let metadata_path = entry.path().join(format!("{folder_name}.json"));
        let Ok(bytes) = fs::read(&metadata_path).await else {
            continue;
        };
        if bytes.len() > 16 * 1024 * 1024 {
            continue;
        }
        let Ok(metadata) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        if profile_matches(&metadata, game_version, loader)
            && metadata.get("id").and_then(Value::as_str) == Some(folder_name.as_str())
        {
            matches.push((folder_name, metadata_path, metadata));
        }
    }
    matches.sort_by(|left, right| left.0.cmp(&right.0));
    matches.pop().ok_or_else(|| {
        format!(
            "The {loader} installer returned successfully but did not create a validated launch profile for Minecraft {game_version}"
        )
    })
}

fn merge_profile(base: &Value, loader_profile: &Value, profile_id: &str, game_version: &str) -> Result<Value, String> {
    let mut merged = base.clone();
    merged["id"] = Value::String(profile_id.to_owned());
    merged["inheritsFrom"] = Value::String(game_version.to_owned());
    if let Some(main_class) = loader_profile.get("mainClass").and_then(Value::as_str) {
        merged["mainClass"] = Value::String(main_class.to_owned());
    }
    for key in ["libraries"] {
        if !merged.get(key).is_some_and(Value::is_array) {
            merged[key] = json!([]);
        }
        let additions = loader_profile
            .get(key)
            .and_then(Value::as_array)
            .ok_or_else(|| format!("Loader profile has no {key} array"))?;
        if let Some(target) = merged[key].as_array_mut() {
            for addition in additions {
                let name = addition.get("name").and_then(Value::as_str).unwrap_or_default();
                if let Some(existing) = target.iter_mut().find(|item| {
                    item.get("name").and_then(Value::as_str) == Some(name)
                }) {
                    *existing = addition.clone();
                } else {
                    target.push(addition.clone());
                }
            }
        }
    }
    if !merged.get("arguments").is_some_and(Value::is_object) {
        merged["arguments"] = json!({});
    }
    if let Some(arguments) = loader_profile.get("arguments").and_then(Value::as_object) {
        for key in ["jvm", "game"] {
            if let Some(additions) = arguments.get(key).and_then(Value::as_array) {
                if !merged["arguments"].get(key).is_some_and(Value::is_array) {
                    merged["arguments"][key] = json!([]);
                }
                if let Some(target) = merged["arguments"][key].as_array_mut() {
                    for argument in additions {
                        if !target.contains(argument) {
                            target.push(argument.clone());
                        }
                    }
                }
            }
        }
    }
    if let Some(legacy) = loader_profile.get("minecraftArguments").and_then(Value::as_str) {
        merged["minecraftArguments"] = Value::String(legacy.to_owned());
    }
    Ok(merged)
}

async fn write_json_atomic(path: &Path, value: &Value) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "Invalid loader metadata destination".to_owned())?;
    fs::create_dir_all(parent).await.map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.nova-tmp");
    let backup = path.with_extension("json.nova-backup");
    fs::write(&temporary, bytes).await.map_err(|error| error.to_string())?;
    if fs::try_exists(&backup).await.map_err(|error| error.to_string())? {
        fs::remove_file(&backup).await.map_err(|error| error.to_string())?;
    }
    if fs::try_exists(path).await.map_err(|error| error.to_string())? {
        fs::rename(path, &backup).await.map_err(|error| error.to_string())?;
        if let Err(error) = fs::rename(&temporary, path).await {
            let rollback = fs::rename(&backup, path).await;
            let _ = fs::remove_file(&temporary).await;
            if let Err(rollback_error) = rollback {
                return Err(format!("Unable to publish loader profile ({error}); rollback failed ({rollback_error})"));
            }
            return Err(error.to_string());
        }
        let _ = fs::remove_file(backup).await;
    } else {
        fs::rename(&temporary, path).await.map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn download_installer(
    cache_root: &Path,
    url: &str,
    filename: &str,
) -> Result<PathBuf, String> {
    let parsed = Url::parse(url).map_err(|error| error.to_string())?;
    let host = parsed.host_str().unwrap_or_default();
    if parsed.scheme() != "https"
        || !matches!(host, "maven.minecraftforge.net" | "maven.neoforged.net")
        || !filename.ends_with("-installer.jar")
    {
        return Err("Refusing an installer URL outside the official Forge/NeoForge Maven hosts".to_owned());
    }
    let checksum_bytes = fetch_limited(&format!("{url}.sha1"), 512).await?;
    let checksum = String::from_utf8(checksum_bytes)
        .map_err(|_| "Installer checksum is not valid text".to_owned())?;
    let sha1 = checksum
        .split_whitespace()
        .next()
        .ok_or_else(|| "Installer checksum is empty".to_owned())?;
    if !valid_sha1(sha1) {
        return Err("Installer Maven checksum is invalid".to_owned());
    }

    let destination = cache_root
        .join("loaders")
        .join("installers")
        .join(format!("{}-{sha1}.jar", filename.trim_end_matches(".jar")));
    fs::create_dir_all(destination.parent().unwrap())
        .await
        .map_err(|error| error.to_string())?;
    download_verified(
        DownloadSpec {
            url: url.to_owned(),
            destination: destination.clone(),
            expected_sha1: sha1.to_ascii_lowercase(),
            expected_size: None,
        },
        None,
    )
    .await
    .map_err(|error| format!("Unable to download the verified loader installer: {error}"))?;
    Ok(destination)
}

async fn load_instance_marker(
    data_root: &Path,
    instance_id: &str,
) -> Result<(PathBuf, InstalledInstance), String> {
    if !safe_token(instance_id) {
        return Err("Invalid instance identifier".to_owned());
    }
    let root = data_root.join("instances").join(instance_id);
    let bytes = fs::read(root.join(".nova-install.json"))
        .await
        .map_err(|_| "This NOVA-managed instance is not installed".to_owned())?;
    let marker: InstalledInstance = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid NOVA instance marker: {error}"))?;
    if marker.id != instance_id || marker.instance_path != root.to_string_lossy().as_ref() {
        return Err("Instance marker does not match its NOVA directory".to_owned());
    }
    Ok((root, marker))
}

pub(crate) async fn install_loader_inner(
    data_root: &Path,
    cache_root: &Path,
    instance_id: &str,
    loader: &str,
    selected_version: &str,
) -> Result<InstalledInstance, String> {
    let (instance_root, mut marker) = load_instance_marker(data_root, instance_id).await?;
    if marker.loader.is_some() {
        return Err("This instance already has a mod loader. Remove or recreate the profile before switching loaders.".to_owned());
    }
    let game_version = marker.game_version.clone().unwrap_or_else(|| marker.id.clone());
    if !supported_minecraft_version(&game_version) || !safe_token(selected_version) {
        return Err("Invalid Minecraft or loader version".to_owned());
    }
    let releases = match loader {
        "forge" => list_forge(&game_version).await?,
        "neoforge" => list_neoforge(&game_version).await?,
        _ => return Err("Unsupported mod loader".to_owned()),
    };
    if !releases.iter().any(|release| release.version == selected_version) {
        return Err("The selected loader build is not published for this Minecraft version".to_owned());
    }
    let base_metadata_path = instance_root
        .join("versions")
        .join(&game_version)
        .join(format!("{game_version}.json"));
    let base_metadata_bytes = fs::read(&base_metadata_path)
        .await
        .map_err(|_| "The base Minecraft profile is missing. Repair the Vanilla instance first.".to_owned())?;
    let base_metadata: Value = serde_json::from_slice(&base_metadata_bytes)
        .map_err(|error| format!("Base Minecraft metadata is invalid: {error}"))?;
    if base_metadata.get("id").and_then(Value::as_str) != Some(game_version.as_str()) {
        return Err("Base Minecraft metadata does not match this profile".to_owned());
    }
    let required_java = base_metadata
        .get("javaVersion")
        .and_then(|value| value.get("majorVersion"))
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .unwrap_or(8);
    let (java_path, java_major) = launch::discover_java(required_java)?;

    let (installer_url, filename) = installer_artifact(loader, &game_version, selected_version)?;
    let installer = download_installer(cache_root, &installer_url, &filename).await?;
    let launcher_profiles = instance_root.join("launcher_profiles.json");
    if !fs::try_exists(&launcher_profiles).await.map_err(|error| error.to_string())? {
        fs::write(&launcher_profiles, br#"{"profiles":{},"settings":{}}"#)
            .await
            .map_err(|error| error.to_string())?;
    }

    let mut command = Command::new(&java_path);
    command
        .arg("-Djava.awt.headless=true")
        .arg("-jar")
        .arg(&installer)
        .arg("--installClient")
        .arg(&instance_root)
        .current_dir(&instance_root)
        .kill_on_drop(true);
    let output = tokio::time::timeout(INSTALLER_TIMEOUT, command.output())
        .await
        .map_err(|_| "The loader installer exceeded NOVA's 20-minute time limit".to_owned())?
        .map_err(|error| format!("Unable to execute the {loader} installer with Java {java_major}: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let diagnostic = stderr.lines().chain(stdout.lines()).rev().take(18).collect::<Vec<_>>();
        return Err(format!(
            "{loader} installer failed with exit code {:?}: {}",
            output.status.code(),
            diagnostic.into_iter().rev().collect::<Vec<_>>().join("\n")
        ));
    }

    let (profile_id, profile_path, raw_profile) =
        find_loader_profile(&instance_root, &game_version, loader).await?;
    let merged = merge_profile(&base_metadata, &raw_profile, &profile_id, &game_version)?;
    write_json_atomic(&profile_path, &merged).await?;

    marker.loader = Some(loader.to_owned());
    marker.loader_version = Some(selected_version.to_owned());
    marker.launch_version = Some(profile_id);
    write_marker(&instance_root, &marker).await?;
    Ok(marker)
}

#[tauri::command]
pub(crate) async fn install_loader(
    app: tauri::AppHandle,
    instance_id: String,
    loader: String,
    version: String,
) -> Result<InstalledInstance, String> {
    if launch::is_instance_running(&instance_id)? {
        return Err("Stop Minecraft before installing a loader".to_owned());
    }
    let data_root = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let cache_root = app.path().app_cache_dir().map_err(|error| error.to_string())?;
    install_loader_inner(&data_root, &cache_root, &instance_id, &loader, &version).await
}

#[tauri::command]
pub(crate) async fn list_loader_versions_command(
    game_version: String,
    loader: String,
) -> Result<Vec<LoaderRelease>, String> {
    list_loader_versions(game_version, loader).await
}

#[cfg(test)]
mod tests {
    use super::{is_stable, neoforge_prefix, supported_minecraft_version, xml_versions};

    #[test]
    fn parses_maven_version_entries_and_stable_channels() {
        let xml = "<metadata><versioning><versions><version>21.1.1</version><version>21.1.2-beta</version></versions></versioning></metadata>";
        assert_eq!(xml_versions(xml), vec!["21.1.1", "21.1.2-beta"]);
        assert!(is_stable("21.1.1"));
        assert!(!is_stable("21.1.2-beta"));
    }

    #[test]
    fn maps_supported_neoforge_version_families() {
        assert_eq!(neoforge_prefix("1.21.1").as_deref(), Some("21.1."));
        assert_eq!(neoforge_prefix("1.20.4").as_deref(), Some("20.4."));
        assert_eq!(neoforge_prefix("1.20.1").as_deref(), Some("20.1."));
        assert_eq!(neoforge_prefix("26.1.2").as_deref(), Some("26.1.2."));
    }

    #[test]
    fn rejects_invalid_or_unsupported_game_versions() {
        assert!(supported_minecraft_version("1.21.1"));
        assert!(!supported_minecraft_version("../outside"));
    }
}
