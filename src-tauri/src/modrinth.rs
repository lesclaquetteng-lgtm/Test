use crate::{
    downloader::{download_verified, DownloadSpec},
    mojang::{self, InstallProgress, InstalledInstance},
};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest as Sha1Digest, Sha1};
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};
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

async fn write_instance_marker(root: &Path, marker: &InstalledInstance) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(marker).map_err(|error| error.to_string())?;
    let temporary = root.join(".nova-install.json.new");
    fs::write(&temporary, bytes)
        .await
        .map_err(|error| error.to_string())?;
    fs::rename(&temporary, root.join(".nova-install.json"))
        .await
        .map_err(|error| error.to_string())
}

async fn write_instance_metadata(path: &Path, metadata: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(metadata).map_err(|error| error.to_string())?;
    let parent = path.parent().ok_or_else(|| "Invalid Minecraft metadata path".to_owned())?;
    fs::create_dir_all(parent).await.map_err(|error| error.to_string())?;
    let temporary = path.with_extension("json.new");
    fs::write(&temporary, bytes).await.map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).await.map_err(|error| error.to_string())
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
