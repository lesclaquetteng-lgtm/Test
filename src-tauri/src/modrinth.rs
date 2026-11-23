use crate::{
    downloader::{download_verified, DownloadSpec},
    mojang::{self, InstallProgress, InstalledInstance},
};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::Sha512;
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use std::io::Read;
use tokio::{fs, sync::mpsc};
use tauri::Manager;
use url::Url;

const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
const MAX_API_RESPONSE: usize = 24 * 1024 * 1024;
const MAX_FABRIC_PROFILE: usize = 12 * 1024 * 1024;
const MAX_MOD_FILE_BYTES: u64 = 1024 * 1024 * 1024;

static CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();

fn client() -> Result<&'static Client, String> {
    match CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(90))
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent(concat!(
                "lesclaquetteng/NOVA-Launcher/",
                env!("CARGO_PKG_VERSION"),
                " (https://github.com/lesclaquetteng-lgtm/Test)"
            ))
            .build()
            .map_err(|error| error.to_string())
    }) {
        Ok(value) => Ok(value),
        Err(error) => Err(error.clone()),
    }
}

fn valid_sha1(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn safe_relative_path(raw: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    if raw.is_empty()
        || raw.contains(':')
        || raw.contains('\\')
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("Unsafe Modrinth file path: {raw}"));
    }
    Ok(path.to_path_buf())
}

fn safe_mod_filename(raw: &str) -> Result<String, String> {
    let path = safe_relative_path(raw)?;
    if path.components().count() != 1
        || path.extension().and_then(|ext| ext.to_str()) != Some("jar")
    {
        return Err(format!("Modrinth returned an invalid mod JAR name: {raw}"));
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| "Modrinth file name is not valid UTF-8".to_owned())
}

async fn fetch_limited(raw_url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let url = Url::parse(raw_url).map_err(|_| "Invalid Modrinth or Fabric URL".to_owned())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("Only HTTPS metadata URLs without credentials are accepted".to_owned());
    }
    let response = client()?.get(url).send().await.map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("Remote service returned HTTP {}", response.status()));
    }
    if response.content_length().is_some_and(|length| length > limit as u64) {
        return Err("Remote metadata exceeded the configured size limit".to_owned());
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err("Remote metadata exceeded the configured size limit".to_owned());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

async fn get_json<T: for<'de> Deserialize<'de>>(url: &str, limit: usize) -> Result<T, String> {
    let bytes = fetch_limited(url, limit).await?;
    serde_json::from_slice(&bytes).map_err(|error| format!("Invalid remote JSON: {error}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModrinthProject {
    pub project_id: String,
    pub project_type: String,
    pub title: String,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub follows: u64,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub versions: Vec<String>,
    #[serde(default)]
    pub date_modified: Option<String>,
    #[serde(default)]
    pub latest_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModrinthSearchResult {
    pub hits: Vec<ModrinthProject>,
    pub offset: u32,
    pub limit: u32,
    pub total_hits: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModrinthFileHashes {
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModrinthFile {
    pub url: String,
    pub filename: String,
    pub size: u64,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub hashes: ModrinthFileHashes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModrinthDependency {
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub dependency_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModrinthVersion {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub version_number: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default)]
    pub version_type: String,
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<ModrinthDependency>,
    #[serde(default)]
    pub files: Vec<ModrinthFile>,
    #[serde(default)]
    pub date_published: Option<String>,
    #[serde(default)]
    pub downloads: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FabricLoaderVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstalledModFile {
    pub file_name: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModInstallResult {
    pub installed_files: Vec<String>,
    pub instance_id: String,
}

#[derive(Debug, Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoaderEntryDetails,
}

#[derive(Debug, Deserialize)]
struct FabricLoaderEntryDetails {
    version: String,
    #[serde(default)]
    stable: bool,
}

#[tauri::command]
pub(crate) async fn search_projects(
    query: String,
    project_type: String,
    game_version: Option<String>,
    loader: Option<String>,
    offset: Option<u32>,
) -> Result<ModrinthSearchResult, String> {
    let project_type = match project_type.as_str() {
        "mod" | "modpack" => project_type,
        _ => return Err("Unsupported Modrinth project type".to_owned()),
    };
    let offset = offset.unwrap_or(0).min(10_000);
    let mut facets: Vec<Vec<String>> = vec![vec![format!("project_type:{project_type}")]];
    if let Some(game_version) = game_version.filter(|value| safe_identifier(value)) {
        facets.push(vec![format!("versions:{game_version}")]);
    }
    if let Some(loader) = loader.filter(|value| safe_identifier(value)) {
        facets.push(vec![format!("categories:{loader}")]);
    }

    let mut url = Url::parse(&format!("{MODRINTH_API}/search"))
        .map_err(|error| error.to_string())?;
    {
        let mut pairs = url.query_pairs_mut();
        let query = query.trim();
        if !query.is_empty() {
            pairs.append_pair("query", &query.chars().take(120).collect::<String>());
        }
        pairs.append_pair("facets", &serde_json::to_string(&facets).map_err(|error| error.to_string())?);
        pairs.append_pair("index", "downloads");
        pairs.append_pair("limit", "24");
        pairs.append_pair("offset", &offset.to_string());
    }
    get_json(url.as_str(), MAX_API_RESPONSE).await
}

pub(crate) async fn get_project_versions(
    project_id: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<ModrinthVersion>, String> {
    if !safe_identifier(project_id) {
        return Err("Invalid Modrinth project identifier".to_owned());
    }
    let mut url = Url::parse(&format!("{MODRINTH_API}/project/{project_id}/version"))
        .map_err(|error| error.to_string())?;
    {
        let mut pairs = url.query_pairs_mut();
        if let Some(version) = game_version.filter(|value| safe_identifier(value)) {
            pairs.append_pair("game_versions", &json!([version]).to_string());
        }
        if let Some(loader) = loader.filter(|value| safe_identifier(value)) {
            pairs.append_pair("loaders", &json!([loader]).to_string());
        }
        pairs.append_pair("include_changelog", "false");
    }
    get_json(url.as_str(), MAX_API_RESPONSE).await
}

#[tauri::command]
pub(crate) async fn get_project_versions_command(
    project_id: String,
    game_version: Option<String>,
    loader: Option<String>,
) -> Result<Vec<ModrinthVersion>, String> {
    get_project_versions(&project_id, game_version.as_deref(), loader.as_deref()).await
}

#[tauri::command]
pub(crate) async fn get_fabric_loader_versions(
    game_version: String,
) -> Result<Vec<FabricLoaderVersion>, String> {
    if !safe_identifier(&game_version) {
        return Err("Invalid Minecraft version identifier".to_owned());
    }
    let entries: Vec<FabricLoaderEntry> = get_json(
        &format!("{FABRIC_META}/versions/loader/{game_version}"),
        MAX_API_RESPONSE,
    )
    .await?;
    Ok(entries
        .into_iter()
        .map(|entry| FabricLoaderVersion {
            version: entry.loader.version,
            stable: entry.loader.stable,
        })
        .collect())
}

#[derive(Debug)]
struct FabricArtifact {
    name: String,
    path: String,
    url: String,
    sha1: String,
    size: Option<u64>,
}

fn maven_path(coordinate: &str) -> Result<String, String> {
    let (coordinate, extension) = coordinate.split_once('@').unwrap_or((coordinate, "jar"));
    if !matches!(extension, "jar" | "pom" | "zip") {
        return Err("Fabric profile requested an unsupported library extension".to_owned());
    }
    let parts: Vec<&str> = coordinate.split(':').collect();
    if !(3..=4).contains(&parts.len())
        || parts.iter().any(|part| part.is_empty() || part.contains('/') || part.contains('\\'))
    {
        return Err("Fabric profile contains an invalid Maven coordinate".to_owned());
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).map(|value| format!("-{value}")).unwrap_or_default();
    Ok(format!(
        "{group}/{artifact}/{version}/{artifact}-{version}{classifier}.{extension}"
    ))
}

fn fabric_artifact(library: &Value) -> Result<FabricArtifact, String> {
    let name = library
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "Fabric library is missing its Maven coordinate".to_owned())?
        .to_owned();

    if let Some(artifact) = library
        .get("downloads")
        .and_then(|value| value.get("artifact"))
    {
        let path = artifact
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Fabric library {name} is missing its artifact path"))?;
        let url = artifact
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Fabric library {name} is missing its download URL"))?;
        let sha1 = artifact
            .get("sha1")
            .and_then(Value::as_str)
            .or_else(|| library.get("sha1").and_then(Value::as_str))
            .ok_or_else(|| format!("Fabric library {name} has no SHA-1 checksum"))?;
        let size = artifact
            .get("size")
            .and_then(Value::as_u64)
            .or_else(|| library.get("size").and_then(Value::as_u64));
        safe_relative_path(path)?;
        if !valid_sha1(sha1) {
            return Err(format!("Fabric library {name} has an invalid SHA-1 checksum"));
        }
        return Ok(FabricArtifact {
            name,
            path: path.to_owned(),
            url: url.to_owned(),
            sha1: sha1.to_ascii_lowercase(),
            size,
        });
    }

    let path = maven_path(&name)?;
    let sha1 = library
        .get("sha1")
        .and_then(Value::as_str)
        .or_else(|| {
            library
                .get("checksums")
                .and_then(Value::as_array)
                .and_then(|values| values.iter().find_map(Value::as_str))
        })
        .ok_or_else(|| format!("Fabric library {name} has no SHA-1 checksum"))?;
    if !valid_sha1(sha1) {
        return Err(format!("Fabric library {name} has an invalid SHA-1 checksum"));
    }
    let base = library
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("https://maven.fabricmc.net/");
    let base = Url::parse(base).map_err(|error| error.to_string())?;
    if base.scheme() != "https" || base.host_str().is_none() {
        return Err(format!("Fabric library {name} uses an unsafe Maven repository"));
    }
    let mut url = base;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "Fabric Maven URL cannot accept path segments".to_owned())?;
        segments.pop_if_empty();
        for segment in path.split('/') {
            segments.push(segment);
        }
    }
    Ok(FabricArtifact {
        name,
        path,
        url: url.to_string(),
        sha1: sha1.to_ascii_lowercase(),
        size: library.get("size").and_then(Value::as_u64),
    })
}

async fn read_instance_marker(
    data_root: &Path,
    instance_id: &str,
) -> Result<(PathBuf, InstalledInstance), String> {
    if !safe_identifier(instance_id) {
        return Err("Invalid instance identifier".to_owned());
    }
    let root = data_root.join("instances").join(instance_id);
    let marker_path = root.join(".nova-install.json");
    let bytes = fs::read(&marker_path)
        .await
        .map_err(|_| "This Minecraft instance is not installed by NOVA".to_owned())?;
    let marker: InstalledInstance = serde_json::from_slice(&bytes)
        .map_err(|error| format!("The NOVA instance marker is invalid: {error}"))?;
    if marker.id != instance_id || !valid_sha1(&marker.client_sha1) {
        return Err("The instance verification marker does not match the requested profile".to_owned());
    }
    Ok((root, marker))
}

async fn replace_file_safely(temporary: &Path, destination: &Path) -> Result<(), String> {
    let backup = destination.with_extension("nova-backup");
    if fs::try_exists(&backup).await.map_err(|error| error.to_string())? {
        fs::remove_file(&backup).await.map_err(|error| error.to_string())?;
    }
    if fs::try_exists(destination).await.map_err(|error| error.to_string())? {
        fs::rename(destination, &backup).await.map_err(|error| error.to_string())?;
        if let Err(error) = fs::rename(temporary, destination).await {
            let rollback = fs::rename(&backup, destination).await;
            let _ = fs::remove_file(temporary).await;
            if let Err(rollback_error) = rollback {
                return Err(format!("Unable to publish updated metadata ({error}); rollback also failed ({rollback_error})"));
            }
            return Err(error.to_string());
        }
        let _ = fs::remove_file(backup).await;
    } else {
        fs::rename(temporary, destination).await.map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn write_instance_marker(root: &Path, marker: &InstalledInstance) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(marker).map_err(|error| error.to_string())?;
    let temporary = root.join(".nova-install.json.new");
    let destination = root.join(".nova-install.json");
    fs::write(&temporary, bytes).await.map_err(|error| error.to_string())?;
    replace_file_safely(&temporary, &destination).await
}

async fn write_instance_metadata(path: &Path, metadata: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(metadata).map_err(|error| error.to_string())?;
    let parent = path.parent().ok_or_else(|| "Invalid Minecraft metadata path".to_owned())?;
    fs::create_dir_all(parent).await.map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.new");
    fs::write(&temporary, bytes).await.map_err(|error| error.to_string())?;
    replace_file_safely(&temporary, path).await
}

fn merge_profile_arguments(base: &mut Value, profile: &Value) {
    let Some(profile_arguments) = profile.get("arguments").and_then(Value::as_object) else {
        return;
    };
    if !base.get("arguments").is_some_and(Value::is_object) {
        base["arguments"] = json!({});
    }
    for key in ["jvm", "game"] {
        let Some(additions) = profile_arguments.get(key).and_then(Value::as_array) else {
            continue;
        };
        if !base["arguments"].get(key).is_some_and(Value::is_array) {
            base["arguments"][key] = json!([]);
        }
        if let Some(target) = base["arguments"][key].as_array_mut() {
            for item in additions {
                if !target.contains(item) {
                    target.push(item.clone());
                }
            }
        }
    }
}

async fn install_fabric_loader_inner(
    data_root: &Path,
    instance_id: &str,
    loader_version: Option<&str>,
    progress: Option<mpsc::Sender<InstallProgress>>,
) -> Result<InstalledInstance, String> {
    let (instance_root, mut marker) = read_instance_marker(data_root, instance_id).await?;
    let game_version = marker
        .game_version
        .clone()
        .unwrap_or_else(|| marker.id.clone());
    if !safe_identifier(&game_version) {
        return Err("The Minecraft base version for this instance is invalid".to_owned());
    }

    let versions = get_fabric_loader_versions(game_version.clone()).await?;
    let selected = match loader_version {
        Some(value) => {
            if !safe_identifier(value) {
                return Err("Invalid Fabric loader version".to_owned());
            }
            value.to_owned()
        }
        None => versions
            .iter()
            .find(|version| version.stable)
            .or_else(|| versions.first())
            .map(|version| version.version.clone())
            .ok_or_else(|| format!("No Fabric loader is published for Minecraft {game_version}"))?,
    };

    let profile_url = format!(
        "{FABRIC_META}/versions/loader/{game_version}/{selected}/profile/json"
    );
    let profile: Value = get_json(&profile_url, MAX_FABRIC_PROFILE).await?;
    if profile.get("inheritsFrom").and_then(Value::as_str) != Some(game_version.as_str()) {
        return Err("Fabric metadata does not inherit from the selected Minecraft version".to_owned());
    }
    let main_class = profile
        .get("mainClass")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Fabric profile does not provide a Minecraft main class".to_owned())?;

    let metadata_path = instance_root
        .join("versions")
        .join(&game_version)
        .join(format!("{game_version}.json"));
    let metadata_bytes = fs::read(&metadata_path)
        .await
        .map_err(|_| "Vanilla version metadata is missing. Repair the Minecraft instance first.".to_owned())?;
    let mut metadata: Value = serde_json::from_slice(&metadata_bytes)
        .map_err(|error| format!("Minecraft version metadata is invalid: {error}"))?;
    if metadata.get("id").and_then(Value::as_str) != Some(game_version.as_str()) {
        return Err("Minecraft version metadata does not match this instance".to_owned());
    }

    let libraries = profile
        .get("libraries")
        .and_then(Value::as_array)
        .ok_or_else(|| "Fabric profile has no library list".to_owned())?;
    let libraries_root = data_root.join("shared").join("libraries");
    fs::create_dir_all(&libraries_root)
        .await
        .map_err(|error| error.to_string())?;
    let mut installed_libraries = Vec::with_capacity(libraries.len());
    let mut seen_paths = HashSet::new();
    for library in libraries {
        let artifact = fabric_artifact(library)?;
        if !seen_paths.insert(artifact.path.clone()) {
            continue;
        }
        let relative = safe_relative_path(&artifact.path)?;
        let destination = libraries_root.join(relative);
        let outcome = download_verified(
            DownloadSpec {
                url: artifact.url.clone(),
                destination,
                expected_sha1: artifact.sha1.clone(),
                expected_size: artifact.size,
            },
            None,
        )
        .await
        .map_err(|error| format!("Unable to install Fabric library {}: {error}", artifact.name))?;

        installed_libraries.push(json!({
            "name": artifact.name,
            "downloads": {
                "artifact": {
                    "path": artifact.path,
                    "url": artifact.url,
                    "sha1": artifact.sha1,
                    "size": outcome.bytes
                }
            }
        }));
        if let Some(sender) = &progress {
            let _ = sender.send(InstallProgress {
                phase: "fabric-libraries".to_owned(),
                completed_files: installed_libraries.len(),
                total_files: libraries.len(),
                current_file: artifact.name,
                transferred_bytes: outcome.bytes,
                total_bytes: artifact.size,
                attempt: 1,
            }).await;
        }
    }

    if !metadata["libraries"].is_array() {
        metadata["libraries"] = json!([]);
    }
    if let Some(target) = metadata["libraries"].as_array_mut() {
        for library in installed_libraries {
            let name = library.get("name").and_then(Value::as_str).unwrap_or_default();
            if !target.iter().any(|existing| existing.get("name").and_then(Value::as_str) == Some(name)) {
                target.push(library);
            }
        }
    }
    metadata["mainClass"] = Value::String(main_class.to_owned());
    merge_profile_arguments(&mut metadata, &profile);
    write_instance_metadata(&metadata_path, &metadata).await?;

    marker.loader = Some("fabric".to_owned());
    marker.loader_version = Some(selected);
    write_instance_marker(&instance_root, &marker).await?;
    Ok(marker)
}

#[tauri::command]
pub(crate) async fn install_fabric_loader(
    app: tauri::AppHandle,
    instance_id: String,
    loader_version: Option<String>,
) -> Result<InstalledInstance, String> {
    let data_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Unable to resolve NOVA data directory: {error}"))?;
    install_fabric_loader_inner(&data_root, &instance_id, loader_version.as_deref(), None).await
}

async fn fetch_version(version_id: &str) -> Result<ModrinthVersion, String> {
    if !safe_identifier(version_id) {
        return Err("Invalid Modrinth version identifier".to_owned());
    }
    get_json(
        &format!("{MODRINTH_API}/version/{version_id}"),
        MAX_API_RESPONSE,
    )
    .await
}

async fn collect_required_versions(
    root_version: ModrinthVersion,
    game_version: &str,
) -> Result<Vec<ModrinthVersion>, String> {
    let mut visited = HashSet::new();
    let mut output = Vec::new();
    let mut pending = vec![(root_version, 0_usize)];

    while let Some((version, depth)) = pending.pop() {
        if depth > 16 || output.len() + pending.len() > 64 {
            return Err("This mod has an unusually deep dependency tree; installation was stopped safely".to_owned());
        }
        if !visited.insert(version.id.clone()) {
            continue;
        }
        if !version.game_versions.iter().any(|value| value == game_version)
            || !version.loaders.iter().any(|value| value == "fabric")
        {
            return Err(format!(
                "{} {} is not compatible with Minecraft {game_version} and Fabric",
                version.name, version.version_number
            ));
        }
        if matches!(
            version.environment.as_deref(),
            Some("server_only" | "dedicated_server_only")
        ) {
            return Err(format!("{} is a server-only mod and cannot be installed on a client", version.name));
        }

        for dependency in &version.dependencies {
            if dependency.dependency_type != "required" {
                continue;
            }
            let dependency_version = if let Some(id) = dependency.version_id.as_deref() {
                Some(fetch_version(id).await?)
            } else if let Some(project_id) = dependency.project_id.as_deref() {
                get_project_versions(project_id, Some(game_version), Some("fabric"))
                    .await?
                    .into_iter()
                    .find(|candidate| {
                        candidate.game_versions.iter().any(|value| value == game_version)
                            && candidate.loaders.iter().any(|value| value == "fabric")
                    })
            } else {
                None
            };
            if let Some(dependency_version) = dependency_version {
                pending.push((dependency_version, depth + 1));
            } else {
                return Err(format!(
                    "A required dependency for {} could not be resolved",
                    version.name
                ));
            }
        }
        output.push(version);
    }
    Ok(output)
}

async fn install_modrinth_file(
    instance_root: &Path,
    file: &ModrinthFile,
    progress: Option<&mpsc::Sender<InstallProgress>>,
    index: usize,
    total: usize,
) -> Result<String, String> {
    let file_name = safe_mod_filename(&file.filename)?;
    if file.size == 0 || file.size > MAX_MOD_FILE_BYTES {
        return Err(format!("Modrinth file {} has an invalid size", file.filename));
    }
    let sha1 = file.hashes.sha1.as_deref()
        .ok_or_else(|| format!("Modrinth did not provide a SHA-1 checksum for {}", file.filename))?;
    if !valid_sha1(sha1) {
        return Err(format!("Modrinth supplied an invalid checksum for {}", file.filename));
    }

    let destination = instance_root.join("mods").join(&file_name);
    if fs::try_exists(&destination).await.map_err(|error| error.to_string())? {
        let bytes = fs::read(&destination).await.map_err(|error| error.to_string())?;
        let actual = {
            let digest = Sha1::digest(&bytes);
            digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
        };
        if !actual.eq_ignore_ascii_case(sha1) {
            return Err(format!(
                "A different file named {file_name} already exists. Remove it before installing this version."
            ));
        }
        return Ok(file_name);
    }

    let outcome = download_verified(
        DownloadSpec {
            url: file.url.clone(),
            destination,
            expected_sha1: sha1.to_ascii_lowercase(),
            expected_size: Some(file.size),
        },
        None,
    )
    .await
    .map_err(|error| format!("Unable to download {}: {error}", file.filename))?;

    if let Some(sender) = progress {
        let _ = sender.send(InstallProgress {
            phase: "modrinth-mods".to_owned(),
            completed_files: index,
            total_files: total,
            current_file: file_name.clone(),
            transferred_bytes: outcome.bytes,
            total_bytes: Some(file.size),
            attempt: 1,
        }).await;
    }
    Ok(file_name)
}

async fn install_modrinth_mod_inner(
    data_root: &Path,
    instance_id: &str,
    project_id: &str,
    progress: Option<mpsc::Sender<InstallProgress>>,
) -> Result<ModInstallResult, String> {
    let (instance_root, marker) = read_instance_marker(data_root, instance_id).await?;
    if marker.loader.as_deref() != Some("fabric") {
        return Err("Install Fabric on this instance before adding Fabric mods".to_owned());
    }
    let game_version = marker.game_version.clone().unwrap_or_else(|| marker.id.clone());
    let candidates = get_project_versions(project_id, Some(&game_version), Some("fabric")).await?;
    let selected = candidates
        .into_iter()
        .find(|version| {
            version.loaders.iter().any(|loader| loader == "fabric")
                && version.game_versions.iter().any(|version| version == &game_version)
        })
        .ok_or_else(|| format!("No Fabric release of this project supports Minecraft {game_version}"))?;

    let versions = collect_required_versions(selected, &game_version).await?;

    let total = versions.iter().map(|version| {
        version.files.iter().filter(|file| file.primary).count().max(1)
    }).sum::<usize>();
    let mut installed_files = Vec::new();
    let mut counter = 0_usize;
    for version in versions {
        let file = version.files.iter()
            .find(|file| file.primary && file.filename.to_ascii_lowercase().ends_with(".jar"))
            .or_else(|| version.files.iter().find(|file| file.filename.to_ascii_lowercase().ends_with(".jar")))
            .ok_or_else(|| format!("Modrinth version {} does not contain a client JAR", version.name))?;
        counter += 1;
        let name = install_modrinth_file(
            &instance_root,
            file,
            progress.as_ref(),
            counter,
            total.max(1),
        ).await?;
        installed_files.push(name);
    }
    installed_files.sort();
    installed_files.dedup();
    Ok(ModInstallResult {
        installed_files,
        instance_id: instance_id.to_owned(),
    })
}

#[tauri::command]
pub(crate) async fn install_modrinth_mod(
    app: tauri::AppHandle,
    instance_id: String,
    project_id: String,
) -> Result<ModInstallResult, String> {
    let data_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Unable to resolve NOVA data directory: {error}"))?;
    install_modrinth_mod_inner(&data_root, &instance_id, &project_id, None).await
}

#[tauri::command]
pub(crate) async fn list_instance_mods(
    app: tauri::AppHandle,
    instance_id: String,
) -> Result<Vec<InstalledModFile>, String> {
    let (instance_root, _) = read_instance_marker(
        &app.path().app_data_dir().map_err(|error| error.to_string())?,
        &instance_id,
    ).await?;
    let mods_root = instance_root.join("mods");
    let mut entries = match fs::read_dir(&mods_root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let mut mods = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|error| error.to_string())? {
        if !entry.file_type().await.map_err(|error| error.to_string())?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.to_ascii_lowercase().ends_with(".jar") {
            continue;
        }
        let size = entry.metadata().await.map_err(|error| error.to_string())?.len();
        mods.push(InstalledModFile { file_name: name, size });
    }
    mods.sort_by(|left, right| left.file_name.to_lowercase().cmp(&right.file_name.to_lowercase()));
    Ok(mods)
}

#[tauri::command]
pub(crate) async fn remove_instance_mod(
    app: tauri::AppHandle,
    instance_id: String,
    file_name: String,
) -> Result<(), String> {
    let (instance_root, _) = read_instance_marker(
        &app.path().app_data_dir().map_err(|error| error.to_string())?,
        &instance_id,
    ).await?;
    let safe_name = safe_mod_filename(&file_name)?;
    let path = instance_root.join("mods").join(safe_name);
    let metadata = fs::symlink_metadata(&path).await.map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("Refusing to remove a non-regular mod file".to_owned());
    }
    fs::remove_file(path).await.map_err(|error| error.to_string())
}


#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MrPackIndex {
    format_version: u32,
    game: String,
    version_id: String,
    name: String,
    #[serde(default)]
    summary: Option<String>,
    files: Vec<MrPackFile>,
    dependencies: std::collections::HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MrPackFile {
    path: String,
    hashes: std::collections::HashMap<String, String>,
    #[serde(default)]
    env: Option<std::collections::HashMap<String, String>>,
    downloads: Vec<String>,
    file_size: u64,
}

fn safe_modpack_url(raw: &str) -> Result<String, String> {
    let url = Url::parse(raw).map_err(|_| "Modrinth pack contains an invalid file URL".to_owned())?;
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let trusted = host == "cdn.modrinth.com"
        || host == "github.com"
        || host == "raw.githubusercontent.com"
        || host == "gitlab.com"
        || host.ends_with(".gitlab.com");
    if url.scheme() != "https"
        || !trusted
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(format!("Modrinth pack contains a download URL from an untrusted host: {host}"));
    }
    Ok(url.to_string())
}

fn sha512_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut digest = Sha512::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        Sha1Digest::update(&mut digest, &buffer[..read]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

async fn verify_sha512(path: PathBuf, expected: String) -> Result<(), String> {
    let actual = tokio::task::spawn_blocking(move || sha512_file(&path))
        .await
        .map_err(|error| format!("SHA-512 verification worker failed: {error}"))??;
    if !actual.eq_ignore_ascii_case(&expected) {
        return Err("Modrinth file failed SHA-512 verification".to_owned());
    }
    Ok(())
}

fn read_mrpack_index(archive_path: &Path) -> Result<MrPackIndex, String> {
    let file = std::fs::File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Invalid Modrinth .mrpack archive: {error}"))?;
    let mut manifest = archive
        .by_name("modrinth.index.json")
        .map_err(|_| "Modrinth pack is missing modrinth.index.json".to_owned())?;
    if manifest.size() > 16 * 1024 * 1024 {
        return Err("Modrinth pack manifest is too large".to_owned());
    }
    let mut bytes = Vec::with_capacity(manifest.size() as usize);
    manifest
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| format!("Invalid Modrinth pack manifest: {error}"))
}

fn extract_mrpack_overrides(archive_path: &Path, instance_root: &Path) -> Result<(usize, u64), String> {
    const MAX_OVERRIDE_FILE: u64 = 256 * 1024 * 1024;
    const MAX_TOTAL_OVERRIDE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
    let file = std::fs::File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Invalid Modrinth .mrpack archive: {error}"))?;
    let mut copied_files = 0_usize;
    let mut copied_bytes = 0_u64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let archive_name = entry.name().replace('\\', "/");
        let relative_name = archive_name
            .strip_prefix("overrides/")
            .or_else(|| archive_name.strip_prefix("client-overrides/"));
        let Some(relative_name) = relative_name else {
            continue;
        };
        let relative = safe_relative_path(relative_name)?;
        let file_size = entry.size();
        if file_size > MAX_OVERRIDE_FILE
            || copied_bytes.saturating_add(file_size) > MAX_TOTAL_OVERRIDE_BYTES
        {
            return Err("Modrinth pack overrides exceed NOVA's extraction size limit".to_owned());
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("Modrinth pack contains a symbolic link; extraction was stopped".to_owned());
        }

        let destination = instance_root.join(relative);
        let parent = destination
            .parent()
            .ok_or_else(|| "Invalid Modrinth override destination".to_owned())?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let temporary = destination.with_extension("nova-tmp");
        let mut output = std::fs::File::create(&temporary).map_err(|error| error.to_string())?;
        let copied = std::io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
        if copied != file_size {
            let _ = std::fs::remove_file(&temporary);
            return Err("Modrinth override size changed during extraction".to_owned());
        }
        std::fs::rename(&temporary, &destination).map_err(|error| error.to_string())?;
        copied_files += 1;
        copied_bytes = copied_bytes.saturating_add(copied);
    }
    Ok((copied_files, copied_bytes))
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            return Err("Refusing to copy a symbolic link from a Minecraft base instance".to_owned());
        }
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

async fn ensure_pack_instance_base(
    data_root: &Path,
    cache_root: &Path,
    pack_instance_id: &str,
    game_version: &str,
    pack_name: &str,
    project_id: &str,
    pack_version_id: &str,
) -> Result<InstalledInstance, String> {
    if !safe_identifier(pack_instance_id) || !safe_identifier(game_version) {
        return Err("Invalid Minecraft or modpack instance identifier".to_owned());
    }
    let source_root = data_root.join("instances").join(game_version);
    let source_marker = source_root.join(".nova-install.json");
    if !fs::try_exists(&source_marker).await.map_err(|error| error.to_string())? {
        mojang::install_vanilla(data_root, cache_root, game_version, None).await?;
    }

    let source_bytes = fs::read(&source_marker).await.map_err(|error| error.to_string())?;
    let mut marker: InstalledInstance = serde_json::from_slice(&source_bytes)
        .map_err(|error| format!("Vanilla base instance marker is invalid: {error}"))?;
    if marker.id != game_version || !valid_sha1(&marker.client_sha1) {
        return Err("Vanilla base instance does not match this modpack's Minecraft version".to_owned());
    }

    let instance_root = data_root.join("instances").join(pack_instance_id);
    if fs::try_exists(&instance_root).await.map_err(|error| error.to_string())? {
        return Err(format!("A NOVA instance named {pack_instance_id} already exists"));
    }
    let game_version_root = instance_root.join("versions").join(game_version);
    fs::create_dir_all(&game_version_root).await.map_err(|error| error.to_string())?;

    let source_version_root = source_root.join("versions").join(game_version);
    let source_client = source_version_root.join(format!("{game_version}.jar"));
    let target_client = game_version_root.join(format!("{game_version}.jar"));
    fs::copy(&source_client, &target_client)
        .await
        .map_err(|error| format!("Unable to copy the verified Minecraft client: {error}"))?;

    let cached_metadata = cache_root
        .join("mojang")
        .join("versions")
        .join(format!("{game_version}.json"));
    let source_metadata = source_version_root.join(format!("{game_version}.json"));
    let metadata_source = if fs::try_exists(&cached_metadata).await.unwrap_or(false) {
        cached_metadata
    } else {
        source_metadata
    };
    fs::copy(metadata_source, game_version_root.join(format!("{game_version}.json")))
        .await
        .map_err(|error| format!("Unable to copy Minecraft version metadata: {error}"))?;

    let source_natives = source_root.join("natives");
    let target_natives = instance_root.join("natives");
    let source_natives_for_copy = source_natives.clone();
    let target_natives_for_copy = target_natives.clone();
    tokio::task::spawn_blocking(move || copy_directory(&source_natives_for_copy, &target_natives_for_copy))
        .await
        .map_err(|error| format!("Native library copy worker failed: {error}"))??;

    marker.id = pack_instance_id.to_owned();
    marker.game_version = Some(game_version.to_owned());
    marker.loader = None;
    marker.loader_version = None;
    marker.instance_type = Some("modpack".to_owned());
    marker.display_name = Some(pack_name.to_owned());
    marker.modrinth_project_id = Some(project_id.to_owned());
    marker.modrinth_version_id = Some(pack_version_id.to_owned());
    marker.instance_path = instance_root.to_string_lossy().into_owned();
    marker.installed_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    write_instance_marker(&instance_root, &marker).await?;
    Ok(marker)
}

async fn install_modrinth_modpack_inner(
    data_root: &Path,
    cache_root: &Path,
    project_id: &str,
    version_id: &str,
    progress: Option<mpsc::Sender<InstallProgress>>,
) -> Result<InstalledInstance, String> {
    if !safe_identifier(project_id) || !safe_identifier(version_id) {
        return Err("Invalid Modrinth project or version identifier".to_owned());
    }
    let versions = get_project_versions(project_id, None, None).await?;
    let version = versions
        .into_iter()
        .find(|candidate| candidate.id == version_id)
        .ok_or_else(|| "The selected Modrinth modpack version no longer exists".to_owned())?;
    let file = version.files
        .iter()
        .find(|file| file.primary && file.filename.to_ascii_lowercase().ends_with(".mrpack"))
        .or_else(|| version.files.iter().find(|file| file.filename.to_ascii_lowercase().ends_with(".mrpack")))
        .ok_or_else(|| "This Modrinth project version does not provide a .mrpack file".to_owned())?;

    if file.size == 0 || file.size > 300 * 1024 * 1024 {
        return Err("Modrinth pack archive exceeds NOVA's 300 MB safety limit".to_owned());
    }
    let archive_sha1 = file.hashes.sha1.as_deref()
        .filter(|value| valid_sha1(value))
        .ok_or_else(|| "Modrinth did not provide a valid SHA-1 hash for this pack archive".to_owned())?;
    let archive_sha512 = file.hashes.sha512.clone()
        .filter(|value| value.len() == 128 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| "Modrinth did not provide a valid SHA-512 hash for this pack archive".to_owned())?;
    let archive_url = safe_modpack_url(&file.url)?;

    let archive_path = cache_root
        .join("modrinth")
        .join("packs")
        .join(format!("{version_id}.mrpack"));
    fs::create_dir_all(archive_path.parent().unwrap())
        .await
        .map_err(|error| error.to_string())?;
    download_verified(
        DownloadSpec {
            url: archive_url,
            destination: archive_path.clone(),
            expected_sha1: archive_sha1.to_ascii_lowercase(),
            expected_size: Some(file.size),
        },
        None,
    )
    .await
    .map_err(|error| format!("Unable to download Modrinth pack: {error}"))?;
    verify_sha512(archive_path.clone(), archive_sha512).await?;

    let index_path = archive_path.clone();
    let index = tokio::task::spawn_blocking(move || read_mrpack_index(&index_path))
        .await
        .map_err(|error| format!("Modrinth pack parser failed: {error}"))??;
    if index.format_version != 1 || index.game != "minecraft" {
        return Err("NOVA supports Minecraft Modrinth packs using format version 1".to_owned());
    }
    if index.files.len() > 10_000 {
        return Err("Modrinth pack contains too many files".to_owned());
    }
    let game_version = index.dependencies.get("minecraft")
        .filter(|value| safe_identifier(value))
        .ok_or_else(|| "Modrinth pack does not declare a valid Minecraft version".to_owned())?
        .clone();
    for unsupported in ["forge", "neoforge", "quilt-loader"] {
        if index.dependencies.contains_key(unsupported) {
            return Err(format!("This pack requires {unsupported}; this NOVA build currently supports Fabric packs only."));
        }
    }
    let fabric_loader = index.dependencies.get("fabric-loader").cloned();
    if fabric_loader.as_deref().is_some_and(|value| !safe_identifier(value)) {
        return Err("Modrinth pack specifies an invalid Fabric loader version".to_owned());
    }
    if fabric_loader.is_none()
        && index.files.iter().any(|entry| entry.path.to_ascii_lowercase().starts_with("mods/"))
    {
        return Err("This pack contains mods but does not declare a supported loader.".to_owned());
    }

    let pack_instance_id = format!("pack_{version_id}");
    if !safe_identifier(&pack_instance_id) {
        return Err("Modrinth pack version identifier cannot be used as a NOVA profile name".to_owned());
    }
    let instance_root = data_root.join("instances").join(&pack_instance_id);
    if fs::try_exists(&instance_root).await.map_err(|error| error.to_string())? {
        return Err(format!("This modpack is already installed as {pack_instance_id}"));
    }

    let mut marker = ensure_pack_instance_base(
        data_root,
        cache_root,
        &pack_instance_id,
        &game_version,
        &index.name,
        project_id,
        version_id,
    ).await?;

    if let Some(loader_version) = fabric_loader {
        marker = install_fabric_loader_inner(
            data_root,
            &pack_instance_id,
            Some(&loader_version),
            progress.clone(),
        ).await?;
    }

    let archive_for_overrides = archive_path.clone();
    let instance_for_overrides = instance_root.clone();
    let (override_files, override_bytes) = tokio::task::spawn_blocking(move || {
        extract_mrpack_overrides(&archive_for_overrides, &instance_for_overrides)
    })
    .await
    .map_err(|error| format!("Modrinth override extraction worker failed: {error}"))??;

    let mut downloaded_files = 0_usize;
    let mut downloaded_bytes = 0_u64;
    let mut seen_paths = HashSet::new();
    for entry in index.files {
        let client_environment = entry.env
            .as_ref()
            .and_then(|environment| environment.get("client"))
            .map(String::as_str)
            .unwrap_or("required");
        if matches!(client_environment, "unsupported" | "optional") {
            continue;
        }
        let relative = safe_relative_path(&entry.path)?;
        let relative_string = relative.to_string_lossy().replace('\\', "/");
        if !seen_paths.insert(relative_string.clone()) {
            return Err(format!("Modrinth pack contains a duplicate file path: {relative_string}"));
        }
        if entry.file_size == 0 || entry.file_size > MAX_MOD_FILE_BYTES {
            return Err(format!("Modrinth pack file {relative_string} has an invalid size"));
        }
        let sha1 = entry.hashes.get("sha1")
            .filter(|value| valid_sha1(value))
            .ok_or_else(|| format!("Modrinth pack file {relative_string} has no valid SHA-1 hash"))?;
        let sha512 = entry.hashes.get("sha512")
            .filter(|value| value.len() == 128 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| format!("Modrinth pack file {relative_string} has no valid SHA-512 hash"))?;
        let raw_url = entry.downloads.iter()
            .find_map(|url| safe_modpack_url(url).ok())
            .ok_or_else(|| format!("Modrinth pack file {relative_string} has no trusted HTTPS mirror"))?;
        let destination = instance_root.join(&relative);
        let outcome = download_verified(
            DownloadSpec {
                url: raw_url,
                destination,
                expected_sha1: sha1.to_ascii_lowercase(),
                expected_size: Some(entry.file_size),
            },
            None,
        )
        .await
        .map_err(|error| format!("Unable to install pack file {relative_string}: {error}"))?;
        let verify_path = instance_root.join(&relative);
        if let Err(error) = verify_sha512(verify_path.clone(), sha512.clone()).await {
            let _ = fs::remove_file(verify_path).await;
            return Err(format!("Integrity verification failed for {relative_string}: {error}"));
        }
        downloaded_files += 1;
        downloaded_bytes = downloaded_bytes.saturating_add(outcome.bytes);
        if let Some(sender) = &progress {
            let _ = sender.send(InstallProgress {
                phase: "modrinth-pack-files".to_owned(),
                completed_files: downloaded_files,
                total_files: index.files.len(),
                current_file: relative_string,
                transferred_bytes: downloaded_bytes,
                total_bytes: None,
                attempt: 1,
            }).await;
        }
    }

    marker.files = marker.files.saturating_add(downloaded_files).saturating_add(override_files);
    marker.total_bytes = marker.total_bytes
        .saturating_add(downloaded_bytes)
        .saturating_add(override_bytes);
    marker.instance_type = Some("modpack".to_owned());
    marker.display_name = Some(index.name);
    marker.modrinth_project_id = Some(project_id.to_owned());
    marker.modrinth_version_id = Some(version_id.to_owned());
    write_instance_marker(&instance_root, &marker).await?;

    if let Some(sender) = progress {
        let _ = sender.send(InstallProgress {
            phase: "completed".to_owned(),
            completed_files: downloaded_files + override_files,
            total_files: downloaded_files + override_files,
            current_file: marker.display_name.clone().unwrap_or_else(|| pack_instance_id.clone()),
            transferred_bytes: downloaded_bytes + override_bytes,
            total_bytes: Some(downloaded_bytes + override_bytes),
            attempt: 1,
        }).await;
    }
    Ok(marker)
}

#[tauri::command]
pub(crate) async fn install_modrinth_modpack(
    app: tauri::AppHandle,
    project_id: String,
    version_id: String,
) -> Result<InstalledInstance, String> {
    let data_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Unable to resolve NOVA data directory: {error}"))?;
    let cache_root = app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("Unable to resolve NOVA cache directory: {error}"))?;

    let pack_instance_id = format!("pack_{version_id}");
    let result = install_modrinth_modpack_inner(
        &data_root,
        &cache_root,
        &project_id,
        &version_id,
        None,
    ).await;
    if result.is_err() && safe_identifier(&pack_instance_id) {
        let partial = data_root.join("instances").join(pack_instance_id);
        let _ = fs::remove_dir_all(partial).await;
    }
    result
}


#[cfg(test)]
mod tests {
    use super::{maven_path, safe_mod_filename, safe_relative_path, valid_sha1};

    #[test]
    fn validates_maven_paths_and_classifiers() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.16.10").unwrap(),
            "net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar"
        );
        assert_eq!(
            maven_path("org.example:sample:1.0:natives-windows@jar").unwrap(),
            "org/example/sample/1.0/sample-1.0-natives-windows.jar"
        );
        assert!(maven_path("../../bad:artifact:1").is_err());
    }

    #[test]
    fn rejects_unsafe_mod_paths() {
        assert!(safe_relative_path("mods/example.jar").is_ok());
        assert!(safe_relative_path("../outside.jar").is_err());
        assert!(safe_relative_path("C:/outside.jar").is_err());
        assert!(safe_mod_filename("example.jar").is_ok());
        assert!(safe_mod_filename("../example.jar").is_err());
        assert!(safe_mod_filename("config/example.jar").is_err());
        assert!(!valid_sha1("not-a-sha1"));
    }
}
