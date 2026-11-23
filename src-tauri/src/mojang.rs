use crate::downloader::{download_verified, DownloadOutcome, DownloadProgress, DownloadSpec};
use futures_util::{stream, StreamExt, TryStreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{fs, sync::mpsc};
use url::Url;

const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const MAX_MANIFEST_BYTES: usize = 32 * 1024 * 1024;
const MAX_VERSION_METADATA_BYTES: usize = 16 * 1024 * 1024;
const MAX_ASSET_INDEX_BYTES: usize = 64 * 1024 * 1024;
const MAX_ASSET_OBJECTS: usize = 250_000;
const CACHE_FRESH_FOR: Duration = Duration::from_secs(15 * 60);
const CACHE_STALE_LIMIT: Duration = Duration::from_secs(14 * 24 * 60 * 60);
const MAX_PARALLEL_INSTALL_TASKS: usize = 8;
const ASSET_BATCH_SIZE: usize = 256;

static METADATA_CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();

#[derive(Debug, thiserror::Error)]
enum MojangError {
    #[error("Metadata URL must use HTTPS")]
    InvalidUrl,
    #[error("Unable to initialize Mojang HTTP client: {0}")]
    ClientInitialization(String),
    #[error("Mojang metadata request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Mojang metadata returned HTTP {0}")]
    HttpStatus(u16),
    #[error("Remote metadata exceeded the configured size limit")]
    ResponseTooLarge,
    #[error("Metadata JSON is invalid: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("The official version manifest is invalid")]
    InvalidManifest,
    #[error("Metadata integrity check failed")]
    HashMismatch,
    #[error("The requested Minecraft version was not found in the official manifest")]
    VersionNotFound,
    #[error("The version metadata does not match the requested version")]
    VersionMismatch,
    #[error("The version does not publish a required client download or asset index")]
    MissingRequiredArtifact,
    #[error("Unsafe artifact path in official metadata")]
    UnsafeArtifactPath,
    #[error("Filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Artifact installation failed: {0}")]
    Download(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: String,
    pub url: String,
    pub time: String,
    pub release_time: String,
    pub sha1: String,
    #[serde(default)]
    pub compliance_level: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstallProgress {
    pub phase: String,
    pub completed_files: usize,
    pub total_files: usize,
    pub current_file: String,
    pub transferred_bytes: u64,
    pub total_bytes: Option<u64>,
    pub attempt: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstalledInstance {
    pub id: String,
    pub version_type: String,
    pub installed_at_unix: u64,
    pub total_bytes: u64,
    pub files: usize,
    pub instance_path: String,
    pub client_sha1: String,
    pub asset_index: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionDetails {
    id: String,
    #[serde(default)]
    main_class: Option<String>,
    #[serde(default)]
    downloads: HashMap<String, VersionDownload>,
    #[serde(default)]
    asset_index: Option<AssetIndexDownload>,
    #[serde(default)]
    libraries: Vec<Library>,
}


#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionDownload {
    sha1: String,
    url: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Artifact {
    path: String,
    sha1: String,
    url: String,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetIndexDownload {
    id: String,
    sha1: String,
    url: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    total_size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Library {
    #[serde(default)]
    downloads: Option<LibraryDownloads>,
    #[serde(default)]
    rules: Vec<LibraryRule>,
    #[serde(default)]
    natives: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LibraryDownloads {
    #[serde(default)]
    artifact: Option<Artifact>,
    #[serde(default)]
    classifiers: HashMap<String, Artifact>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LibraryRule {
    action: String,
    #[serde(default)]
    os: Option<RuleOs>,
    #[serde(default)]
    features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RuleOs {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arch: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AssetIndex {
    #[serde(default)]
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

#[derive(Debug, Clone)]
struct PlannedDownload {
    spec: DownloadSpec,
    display_name: String,
}

fn metadata_client() -> Result<&'static Client, MojangError> {
    match METADATA_CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45))
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent(concat!("NOVA-Launcher/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| error.to_string())
    }) {
        Ok(client) => Ok(client),
        Err(error) => Err(MojangError::ClientInitialization(error.clone())),
    }
}

fn validate_https_url(raw: &str) -> Result<Url, MojangError> {
    let url = Url::parse(raw).map_err(|_| MojangError::InvalidUrl)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(MojangError::InvalidUrl);
    }
    Ok(url)
}

async fn fetch_limited(raw_url: &str, limit: usize) -> Result<Vec<u8>, MojangError> {
    let url = validate_https_url(raw_url)?;
    let response = metadata_client()?.get(url).send().await?;
    if !response.status().is_success() {
        return Err(MojangError::HttpStatus(response.status().as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(MojangError::ResponseTooLarge);
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(MojangError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn sha1_hex(bytes: &[u8]) -> String {
    let digest = Sha1::digest(bytes);
    let mut output = String::with_capacity(40);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn valid_sha1(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_version_id(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn safe_relative_path(raw: &str) -> Result<PathBuf, MojangError> {
    let path = Path::new(raw);
    if raw.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(MojangError::UnsafeArtifactPath);
    }
    Ok(path.to_path_buf())
}

async fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), MojangError> {
    let parent = path.parent().ok_or(MojangError::UnsafeArtifactPath)?;
    fs::create_dir_all(parent).await?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(MojangError::UnsafeArtifactPath)?;
    let temp = parent.join(format!(".{name}.{nonce}.tmp"));
    let backup = parent.join(format!(".{name}.{nonce}.backup"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .await?;
    use tokio::io::AsyncWriteExt;
    let write_result = async {
        file.write_all(bytes).await?;
        file.flush().await?;
        file.sync_all().await?;
        Ok::<(), std::io::Error>(())
    }
    .await;
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp).await;
        return Err(MojangError::Io(error));
    }

    if fs::try_exists(path).await? {
        fs::rename(path, &backup).await?;
        if let Err(error) = fs::rename(&temp, path).await {
            let rollback = fs::rename(&backup, path).await;
            let _ = fs::remove_file(&temp).await;
            if let Err(rollback_error) = rollback {
                return Err(MojangError::Download(format!(
                    "atomic replace failed ({error}); restoring previous file failed ({rollback_error})"
                )));
            }
            return Err(MojangError::Io(error));
        }
        let _ = fs::remove_file(backup).await;
    } else if let Err(error) = fs::rename(&temp, path).await {
        let _ = fs::remove_file(&temp).await;
        return Err(MojangError::Io(error));
    }
    Ok(())
}

fn valid_manifest(manifest: &VersionManifest) -> bool {
    if manifest.versions.is_empty()
        || !safe_version_id(&manifest.latest.release)
        || !safe_version_id(&manifest.latest.snapshot)
    {
        return false;
    }
    let mut seen = HashSet::with_capacity(manifest.versions.len());
    manifest.versions.iter().all(|version| {
        safe_version_id(&version.id)
            && valid_sha1(&version.sha1)
            && validate_https_url(&version.url).is_ok()
            && seen.insert(version.id.as_str())
    })
}

async fn cache_age(path: &Path) -> Option<Duration> {
    let metadata = fs::metadata(path).await.ok()?;
    let modified = metadata.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

pub(crate) async fn fetch_manifest(cache_root: &Path) -> Result<VersionManifest, String> {
    fetch_manifest_inner(cache_root)
        .await
        .map_err(|error| error.to_string())
}

async fn fetch_manifest_inner(cache_root: &Path) -> Result<VersionManifest, MojangError> {
    let path = cache_root.join("mojang").join("version_manifest_v2.json");
    let cached = fs::read(&path).await.ok();
    if let Some(bytes) = cached.as_ref() {
        if cache_age(&path)
            .await
            .is_some_and(|age| age < CACHE_FRESH_FOR)
        {
            if let Ok(manifest) = serde_json::from_slice::<VersionManifest>(bytes) {
                if valid_manifest(&manifest) {
                    return Ok(manifest);
                }
            }
        }
    }

    match fetch_limited(VERSION_MANIFEST_URL, MAX_MANIFEST_BYTES).await {
        Ok(bytes) => {
            let manifest: VersionManifest = serde_json::from_slice(&bytes)?;
            if !valid_manifest(&manifest) {
                return Err(MojangError::InvalidManifest);
            }
            write_atomic(&path, &bytes).await?;
            Ok(manifest)
        }
        Err(network_error) => {
            if let (Some(bytes), Some(age)) = (cached, cache_age(&path).await) {
                if age <= CACHE_STALE_LIMIT {
                    if let Ok(manifest) = serde_json::from_slice::<VersionManifest>(&bytes) {
                        if valid_manifest(&manifest) {
                            return Ok(manifest);
                        }
                    }
                }
            }
            Err(network_error)
        }
    }
}

async fn fetch_version_details(
    cache_root: &Path,
    entry: &VersionEntry,
) -> Result<VersionDetails, MojangError> {
    if !safe_version_id(&entry.id) || !valid_sha1(&entry.sha1) {
        return Err(MojangError::VersionNotFound);
    }
    let cache_path = cache_root
        .join("mojang")
        .join("versions")
        .join(format!("{}.json", entry.id));
    if let Ok(bytes) = fs::read(&cache_path).await {
        if sha1_hex(&bytes).eq_ignore_ascii_case(&entry.sha1) {
            if let Ok(details) = serde_json::from_slice::<VersionDetails>(&bytes) {
                if details.id == entry.id {
                    return Ok(details);
                }
            }
        }
    }

    let bytes = fetch_limited(&entry.url, MAX_VERSION_METADATA_BYTES).await?;
    if !sha1_hex(&bytes).eq_ignore_ascii_case(&entry.sha1) {
        return Err(MojangError::HashMismatch);
    }
    let details: VersionDetails = serde_json::from_slice(&bytes)?;
    if details.id != entry.id {
        return Err(MojangError::VersionMismatch);
    }
    write_atomic(&cache_path, &bytes).await?;
    Ok(details)
}

fn current_os_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "osx",
        "linux" => "linux",
        other => other,
    }
}

fn current_os_arch() -> &'static str {
    std::env::consts::ARCH
}

fn rule_matches(rule: &LibraryRule) -> bool {
    if let Some(os) = &rule.os {
        if os
            .name
            .as_deref()
            .is_some_and(|name| name != current_os_name())
        {
            return false;
        }
        if os
            .arch
            .as_deref()
            .is_some_and(|arch| arch != current_os_arch())
        {
            return false;
        }
    }
    if let Some(features) = &rule.features {
        if features.values().any(|enabled| *enabled) {
            return false;
        }
    }
    true
}

fn library_allowed(rules: &[LibraryRule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule) {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn native_classifier(library: &Library) -> Option<String> {
    let template = library.natives.get(current_os_name())?;
    let arch = if cfg!(target_pointer_width = "64") {
        "64"
    } else {
        "32"
    };
    let placeholder = ["$", "{arch}"].concat();
    Some(template.replace(placeholder.as_str(), arch))
}

fn artifact_spec(artifact: &Artifact, base: &Path) -> Result<DownloadSpec, MojangError> {
    if !valid_sha1(&artifact.sha1) {
        return Err(MojangError::HashMismatch);
    }
    let relative = safe_relative_path(&artifact.path)?;
    Ok(DownloadSpec {
        url: artifact.url.clone(),
        destination: base.join(relative),
        expected_sha1: artifact.sha1.to_ascii_lowercase(),
        expected_size: artifact.size,
    })
}

async fn download_tracked(
    planned: PlannedDownload,
    phase: String,
    completed: Arc<AtomicUsize>,
    total: usize,
    progress: Option<mpsc::Sender<InstallProgress>>,
) -> Result<DownloadOutcome, MojangError> {
    if let Some(progress_sender) = progress {
        let final_sender = progress_sender.clone();
        let (download_sender, mut download_receiver) = mpsc::channel::<DownloadProgress>(8);
        let phase_for_events = phase.clone();
        let display_for_events = planned.display_name.clone();
        let completed_for_events = completed.clone();
        let progress_task = tokio::spawn(async move {
            while let Some(update) = download_receiver.recv().await {
                let _ = progress_sender
                    .send(InstallProgress {
                        phase: phase_for_events.clone(),
                        completed_files: completed_for_events.load(Ordering::Relaxed),
                        total_files: total,
                        current_file: display_for_events.clone(),
                        transferred_bytes: update.transferred_bytes,
                        total_bytes: update.total_bytes,
                        attempt: update.attempt,
                    })
                    .await;
            }
        });
        let result = download_verified(planned.spec, Some(download_sender.clone())).await;
        drop(download_sender);
        let _ = progress_task.await;
        let outcome = result.map_err(|error| MojangError::Download(error.to_string()))?;
        let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = final_sender
            .send(InstallProgress {
                phase,
                completed_files: done,
                total_files: total,
                current_file: planned.display_name,
                transferred_bytes: outcome.bytes,
                total_bytes: Some(outcome.bytes),
                attempt: 1,
            })
            .await;
        Ok(outcome)
    } else {
        download_verified(planned.spec, None)
            .await
            .map_err(|error| MojangError::Download(error.to_string()))
    }
}

async fn download_group(
    planned: Vec<PlannedDownload>,
    phase: &str,
    progress: Option<mpsc::Sender<InstallProgress>>,
) -> Result<Vec<DownloadOutcome>, MojangError> {
    if planned.is_empty() {
        return Ok(Vec::new());
    }
    let total = planned.len();
    let completed = Arc::new(AtomicUsize::new(0));
    stream::iter(planned.into_iter().map(|item| {
        let phase = phase.to_owned();
        let completed = completed.clone();
        let progress = progress.clone();
        async move { download_tracked(item, phase, completed, total, progress).await }
    }))
    .buffer_unordered(MAX_PARALLEL_INSTALL_TASKS)
    .try_collect::<Vec<_>>()
    .await
}

fn summarize_downloads(outcomes: &[DownloadOutcome]) -> (u64, usize, usize) {
    let mut bytes = 0_u64;
    let mut downloaded = 0_usize;
    let mut reused = 0_usize;
    for outcome in outcomes {
        bytes = bytes.saturating_add(outcome.bytes);
        if outcome.reused {
            reused += 1;
        } else {
            downloaded += 1;
        }
    }
    (bytes, downloaded, reused)
}

pub(crate) async fn install_vanilla(
    data_root: &Path,
    cache_root: &Path,
    version_id: &str,
    progress: Option<mpsc::Sender<InstallProgress>>,
) -> Result<InstalledInstance, String> {
    install_vanilla_inner(data_root, cache_root, version_id, progress)
        .await
        .map_err(|error| error.to_string())
}

async fn install_vanilla_inner(
    data_root: &Path,
    cache_root: &Path,
    version_id: &str,
    progress: Option<mpsc::Sender<InstallProgress>>,
) -> Result<InstalledInstance, MojangError> {
    if !safe_version_id(version_id) {
        return Err(MojangError::VersionNotFound);
    }
    let _operation = crate::performance::global().track();
    let manifest = fetch_manifest_inner(cache_root).await?;
    let entry = manifest
        .versions
        .iter()
        .find(|version| version.id == version_id)
        .ok_or(MojangError::VersionNotFound)?;
    let details = fetch_version_details(cache_root, entry).await?;
    let client = details
        .downloads
        .get("client")
        .ok_or(MojangError::MissingRequiredArtifact)?;
    let asset_index = details
        .asset_index
        .as_ref()
        .ok_or(MojangError::MissingRequiredArtifact)?;
    if !safe_version_id(&asset_index.id) {
        return Err(MojangError::UnsafeArtifactPath);
    }

    let instance_root = data_root.join("instances").join(version_id);
    let version_root = instance_root.join("versions").join(version_id);
    let shared_root = data_root.join("shared");
    let libraries_root = shared_root.join("libraries");
    let assets_root = shared_root.join("assets");
    fs::create_dir_all(&version_root).await?;
    fs::create_dir_all(&libraries_root).await?;
    fs::create_dir_all(assets_root.join("indexes")).await?;
    fs::create_dir_all(assets_root.join("objects")).await?;

    let mut total_bytes = 0_u64;
    let mut total_files = 0_usize;

    let client_outcomes = download_group(
        vec![PlannedDownload {
            spec: DownloadSpec {
                url: client.url.clone(),
                destination: version_root.join(format!("{version_id}.jar")),
                expected_sha1: client.sha1.to_ascii_lowercase(),
                expected_size: client.size,
            },
            display_name: format!("{version_id}.jar"),
        }],
        "client",
        progress.clone(),
    )
    .await?;
    let (bytes, _, _) = summarize_downloads(&client_outcomes);
    total_bytes = total_bytes.saturating_add(bytes);
    total_files += client_outcomes.len();

    let asset_index_path = assets_root
        .join("indexes")
        .join(format!("{}.json", asset_index.id));
    let index_outcomes = download_group(
        vec![PlannedDownload {
            spec: DownloadSpec {
                url: asset_index.url.clone(),
                destination: asset_index_path.clone(),
                expected_sha1: asset_index.sha1.to_ascii_lowercase(),
                expected_size: asset_index.size,
            },
            display_name: format!("asset-index/{}.json", asset_index.id),
        }],
        "asset-index",
        progress.clone(),
    )
    .await?;
    let (bytes, _, _) = summarize_downloads(&index_outcomes);
    total_bytes = total_bytes.saturating_add(bytes);
    total_files += index_outcomes.len();

    let index_bytes = fs::read(&asset_index_path).await?;
    if index_bytes.len() > MAX_ASSET_INDEX_BYTES {
        return Err(MojangError::ResponseTooLarge);
    }
    let asset_index_data: AssetIndex = serde_json::from_slice(&index_bytes)?;
    if asset_index_data.objects.len() > MAX_ASSET_OBJECTS {
        return Err(MojangError::ResponseTooLarge);
    }

    let mut library_downloads = Vec::new();
    for library in &details.libraries {
        if !library_allowed(&library.rules) {
            continue;
        }
        let Some(downloads) = &library.downloads else {
            continue;
        };
        if let Some(artifact) = &downloads.artifact {
            library_downloads.push(PlannedDownload {
                spec: artifact_spec(artifact, &libraries_root)?,
                display_name: artifact.path.clone(),
            });
        }
        if let Some(classifier) = native_classifier(library) {
            if let Some(native) = downloads.classifiers.get(&classifier) {
                library_downloads.push(PlannedDownload {
                    spec: artifact_spec(native, &libraries_root)?,
                    display_name: native.path.clone(),
                });
            }
        }
    }
    library_downloads.sort_by(|left, right| left.spec.destination.cmp(&right.spec.destination));
    library_downloads.dedup_by(|left, right| left.spec.destination == right.spec.destination);
    let library_outcomes = download_group(library_downloads, "libraries", progress.clone()).await?;
    let (bytes, _, _) = summarize_downloads(&library_outcomes);
    total_bytes = total_bytes.saturating_add(bytes);
    total_files += library_outcomes.len();

    let mut unique_assets = HashMap::<String, u64>::new();
    for object in asset_index_data.objects.values() {
        if !valid_sha1(&object.hash) {
            return Err(MojangError::HashMismatch);
        }
        unique_assets
            .entry(object.hash.to_ascii_lowercase())
            .and_modify(|size| *size = (*size).max(object.size))
            .or_insert(object.size);
    }
    if unique_assets.len() > MAX_ASSET_OBJECTS {
        return Err(MojangError::ResponseTooLarge);
    }

    let mut asset_hashes: Vec<(String, u64)> = unique_assets.into_iter().collect();
    asset_hashes.sort_by(|left, right| left.0.cmp(&right.0));
    for batch in asset_hashes.chunks(ASSET_BATCH_SIZE) {
        let planned: Result<Vec<_>, MojangError> = batch
            .iter()
            .map(|(hash, size)| {
                let prefix = &hash[..2];
                let relative = safe_relative_path(&format!("{prefix}/{hash}"))?;
                Ok(PlannedDownload {
                    spec: DownloadSpec {
                        url: format!("https://resources.download.minecraft.net/{prefix}/{hash}"),
                        destination: assets_root.join("objects").join(relative),
                        expected_sha1: hash.clone(),
                        expected_size: Some(*size),
                    },
                    display_name: format!("assets/{prefix}/{hash}"),
                })
            })
            .collect();
        let outcomes = download_group(planned?, "assets", progress.clone()).await?;
        let (bytes, _, _) = summarize_downloads(&outcomes);
        total_bytes = total_bytes.saturating_add(bytes);
        total_files += outcomes.len();
    }

    let version_json = serde_json::to_vec_pretty(&details)?;
    write_atomic(
        &version_root.join(format!("{version_id}.json")),
        &version_json,
    )
    .await?;

    let installed = InstalledInstance {
        id: version_id.to_owned(),
        version_type: entry.version_type.clone(),
        installed_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        total_bytes,
        files: total_files,
        instance_path: instance_root.to_string_lossy().into_owned(),
        client_sha1: client.sha1.to_ascii_lowercase(),
        asset_index: asset_index.id.clone(),
    };
    let marker = serde_json::to_vec_pretty(&installed)?;
    write_atomic(&instance_root.join(".nova-install.json"), &marker).await?;

    if let Some(sender) = progress {
        let _ = sender
            .send(InstallProgress {
                phase: "completed".to_owned(),
                completed_files: total_files,
                total_files,
                current_file: version_id.to_owned(),
                transferred_bytes: total_bytes,
                total_bytes: Some(total_bytes),
                attempt: 1,
            })
            .await;
    }
    Ok(installed)
}

pub(crate) async fn list_installed_instances(
    data_root: &Path,
) -> Result<Vec<InstalledInstance>, String> {
    let instances_root = data_root.join("instances");
    let mut entries = match fs::read_dir(&instances_root).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let mut installed = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| error.to_string())?
    {
        if !entry
            .file_type()
            .await
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            continue;
        }
        let marker = entry.path().join(".nova-install.json");
        let Ok(bytes) = fs::read(marker).await else {
            continue;
        };
        let Ok(instance) = serde_json::from_slice::<InstalledInstance>(&bytes) else {
            continue;
        };
        if safe_version_id(&instance.id) && instance.id == entry.file_name().to_string_lossy() {
            installed.push(instance);
        }
    }
    installed.sort_by(|left, right| right.installed_at_unix.cmp(&left.installed_at_unix));
    Ok(installed)
}

#[cfg(test)]
mod tests {
    use super::{
        library_allowed, safe_relative_path, safe_version_id, AssetIndex, LibraryRule, RuleOs,
        VersionManifest,
    };

    #[test]
    fn parses_official_manifest_shape() {
        let raw = r#"{
          "latest":{"release":"1.21.1","snapshot":"24w33a"},
          "versions":[{
            "id":"1.21.1",
            "type":"release",
            "url":"https://piston-meta.mojang.com/v1/packages/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/1.21.1.json",
            "time":"2024-08-08T12:00:00+00:00",
            "releaseTime":"2024-08-08T12:00:00+00:00",
            "sha1":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          }]
        }"#;
        let manifest: VersionManifest = serde_json::from_str(raw).expect("valid manifest");
        assert_eq!(manifest.latest.release, "1.21.1");
        assert_eq!(manifest.versions[0].version_type, "release");
    }

    #[test]
    fn rejects_unsafe_instance_and_artifact_paths() {
        assert!(safe_version_id("1.21.1"));
        assert!(!safe_version_id("../other"));
        assert!(!safe_version_id(".."));
        assert!(safe_relative_path("org/example/library/1.0/library-1.0.jar").is_ok());
        assert!(safe_relative_path("../outside.jar").is_err());
        assert!(safe_relative_path("/absolute.jar").is_err());
    }

    #[test]
    fn applies_library_rules_in_metadata_order() {
        let allowed = vec![
            LibraryRule {
                action: "allow".to_owned(),
                os: None,
                features: None,
            },
            LibraryRule {
                action: "disallow".to_owned(),
                os: Some(RuleOs {
                    name: Some(super::current_os_name().to_owned()),
                    arch: None,
                }),
                features: None,
            },
        ];
        assert!(!library_allowed(&allowed));
        let no_rules: Vec<LibraryRule> = Vec::new();
        assert!(library_allowed(&no_rules));
    }

    #[test]
    fn parses_real_version_download_and_library_artifact_shapes() {
        let raw = r#"{
          "id":"1.21.1",
          "mainClass":"net.minecraft.client.main.Main",
          "downloads":{
            "client":{
              "sha1":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
              "size":100,
              "url":"https://launcher.mojang.com/v1/objects/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/client.jar"
            }
          },
          "assetIndex":{
            "id":"17",
            "sha1":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "size":20,
            "totalSize":100,
            "url":"https://piston-meta.mojang.com/v1/packages/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/17.json"
          },
          "libraries":[{
            "name":"org.example:sample:1.0",
            "downloads":{"artifact":{
              "path":"org/example/sample/1.0/sample-1.0.jar",
              "sha1":"cccccccccccccccccccccccccccccccccccccccc",
              "size":30,
              "url":"https://libraries.minecraft.net/org/example/sample/1.0/sample-1.0.jar"
            }}
          }]
        }"#;
        let details: super::VersionDetails =
            serde_json::from_str(raw).expect("valid official version metadata");
        assert_eq!(details.downloads["client"].size, Some(100));
        assert_eq!(
            details.libraries[0]
                .downloads
                .as_ref()
                .unwrap()
                .artifact
                .as_ref()
                .unwrap()
                .path,
            "org/example/sample/1.0/sample-1.0.jar"
        );
    }

    #[test]
    fn deserializes_asset_indexes() {
        let raw = r#"{"objects":{"minecraft/lang/en_us.json":{"hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":123}}}"#;
        let parsed: AssetIndex = serde_json::from_str(raw).expect("valid asset index");
        assert_eq!(parsed.objects.len(), 1);
    }
}
