use crate::{launch, modrinth};
use futures_util::StreamExt;
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};
use tauri::Manager;
use tokio::{fs, io::AsyncWriteExt};
use url::Url;

const RELEASE_API: &str = "https://api.github.com/repos/lesclaquetteng-lgtm/Test/releases/latest";
const GAME_VERSION: &str = "1.21.11";
const MOD_FILE: &str = "nova-client.jar";
const MAX_JAR_BYTES: usize = 64 * 1024 * 1024;
const MAX_CHECKSUM_BYTES: usize = 4096;
static HTTP: OnceLock<Result<Client, String>> = OnceLock::new();

fn client() -> Result<&'static Client, String> {
    match HTTP.get_or_init(|| {
        Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(90))
            .user_agent(concat!("NOVA-Launcher/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| error.to_string())
    }) {
        Ok(client) => Ok(client),
        Err(error) => Err(error.clone()),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NovaClientReleaseInfo {
    pub version: String,
    pub release_url: String,
    pub jar_name: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NovaClientInstallResult {
    pub version: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    assets: Vec<Asset>,
}

#[derive(Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}

struct Assets {
    info: NovaClientReleaseInfo,
    jar: Asset,
    checksum: Asset,
}

fn trusted_asset_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|_| "Invalid GitHub asset URL".to_owned())?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("GitHub asset URL is not trusted".to_owned());
    }
    Ok(url)
}

async fn body_limited(
    response: Response,
    allowed_hosts: &[&str],
    maximum: usize,
) -> Result<Vec<u8>, String> {
    if !response.status().is_success() {
        return Err(format!(
            "GitHub returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let host = response
        .url()
        .host_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !allowed_hosts.iter().any(|allowed| host == *allowed) {
        return Err(format!("GitHub redirected to an untrusted host: {host}"));
    }
    if response
        .content_length()
        .is_some_and(|size| size > maximum as u64)
    {
        return Err("GitHub response exceeded NOVA's configured size limit".to_owned());
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        if bytes.len().saturating_add(chunk.len()) > maximum {
            return Err("GitHub response exceeded NOVA's configured size limit".to_owned());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn release_assets() -> Result<Option<Assets>, String> {
    let response = client()?
        .get(RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|error| format!("Unable to check NOVA Client releases: {error}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let body = body_limited(response, &["api.github.com"], 2 * 1024 * 1024).await?;
    let release: Release = serde_json::from_slice(&body)
        .map_err(|error| format!("Invalid GitHub release metadata: {error}"))?;
    let page = Url::parse(&release.html_url).map_err(|_| "Invalid release page URL".to_owned())?;
    if page.scheme() != "https" || page.host_str() != Some("github.com") {
        return Err("NOVA Client release page is not hosted on GitHub".to_owned());
    }

    let jars: Vec<Asset> = release
        .assets
        .iter()
        .filter(|asset| {
            let name = asset.name.to_ascii_lowercase();
            name.starts_with("nova-client-")
                && name.ends_with(".jar")
                && !name.ends_with("-sources.jar")
        })
        .cloned()
        .collect();
    if jars.len() != 1 {
        return Err(
            "The latest NOVA Client release must contain exactly one remapped JAR".to_owned(),
        );
    }
    let jar = jars.into_iter().next().expect("asset count was checked");
    if jar.size == 0 || jar.size > MAX_JAR_BYTES as u64 {
        return Err("NOVA Client JAR size is invalid".to_owned());
    }
    trusted_asset_url(&jar.browser_download_url)?;

    let checksum_name = format!("{}.sha256", jar.name);
    let checksum = release
        .assets
        .into_iter()
        .find(|asset| asset.name == checksum_name)
        .ok_or_else(|| "NOVA Client release is missing its SHA-256 checksum".to_owned())?;
    if checksum.size == 0 || checksum.size > MAX_CHECKSUM_BYTES as u64 {
        return Err("NOVA Client checksum size is invalid".to_owned());
    }
    trusted_asset_url(&checksum.browser_download_url)?;

    Ok(Some(Assets {
        info: NovaClientReleaseInfo {
            version: release.tag_name,
            release_url: release.html_url,
            jar_name: jar.name.clone(),
            size_bytes: jar.size,
        },
        jar,
        checksum,
    }))
}

#[tauri::command]
pub(crate) async fn get_nova_client_release() -> Result<Option<NovaClientReleaseInfo>, String> {
    Ok(release_assets().await?.map(|assets| assets.info))
}

async fn download_asset(asset: &Asset, maximum: usize) -> Result<Vec<u8>, String> {
    let url = trusted_asset_url(&asset.browser_download_url)?;
    let response = client()?
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Unable to download {}: {error}", asset.name))?;
    body_limited(
        response,
        &["github.com", "release-assets.githubusercontent.com"],
        maximum,
    )
    .await
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn parse_checksum(bytes: &[u8]) -> Result<String, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "Checksum file is not UTF-8".to_owned())?;
    let digest = text
        .split_whitespace()
        .next()
        .ok_or_else(|| "Checksum file is empty".to_owned())?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Release checksum is not valid SHA-256".to_owned());
    }
    Ok(digest.to_ascii_lowercase())
}

async fn canonical_instance(data_root: &Path, instance_root: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(instance_root)
        .await
        .map_err(|error| error.to_string())?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("Refusing to install into an unsafe instance directory".to_owned());
    }
    let canonical_data = fs::canonicalize(data_root)
        .await
        .map_err(|error| error.to_string())?;
    let instances = fs::canonicalize(data_root.join("instances"))
        .await
        .map_err(|error| error.to_string())?;
    let root = fs::canonicalize(instance_root)
        .await
        .map_err(|error| error.to_string())?;
    if !instances.starts_with(&canonical_data) || !root.starts_with(&instances) {
        return Err("Instance path resolves outside NOVA's managed data directory".to_owned());
    }
    Ok(root)
}

async fn create_temp(directory: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let temp = directory.join(".nova-client.jar.nova-tmp");
    match fs::symlink_metadata(&temp).await {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err("Unsafe NOVA Client temporary file".to_owned());
            }
            fs::remove_file(&temp)
                .await
                .map_err(|error| error.to_string())?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .await
        .map_err(|error| error.to_string())?;
    if let Err(error) = file.write_all(bytes).await {
        let _ = fs::remove_file(&temp).await;
        return Err(error.to_string());
    }
    if let Err(error) = file.sync_all().await {
        let _ = fs::remove_file(&temp).await;
        return Err(error.to_string());
    }
    Ok(temp)
}

async fn publish_jar(temp: &Path, destination: &Path) -> Result<(), String> {
    let backup = destination.with_extension("jar.nova-backup");
    match fs::symlink_metadata(&backup).await {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err("Unsafe NOVA Client backup file".to_owned());
            }
            match fs::symlink_metadata(destination).await {
                Ok(_) => fs::remove_file(&backup)
                    .await
                    .map_err(|error| error.to_string())?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::rename(&backup, destination)
                        .await
                        .map_err(|error| error.to_string())?;
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    match fs::symlink_metadata(destination).await {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err("NOVA Client destination is not a regular file".to_owned());
            }
            fs::rename(destination, &backup)
                .await
                .map_err(|error| error.to_string())?;
            if let Err(error) = fs::rename(temp, destination).await {
                let rollback = fs::rename(&backup, destination).await;
                let _ = fs::remove_file(temp).await;
                if let Err(rollback_error) = rollback {
                    return Err(format!(
                        "Publish failed ({error}) and rollback failed ({rollback_error})"
                    ));
                }
                return Err(format!("Unable to publish NOVA Client JAR: {error}"));
            }
            fs::remove_file(&backup).await.map_err(|error| {
                format!("NOVA Client updated, but backup cleanup failed: {error}")
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::rename(temp, destination)
                .await
                .map_err(|error| error.to_string())?;
        }
        Err(error) => return Err(error.to_string()),
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn install_nova_client(
    app: tauri::AppHandle,
    instance_id: String,
) -> Result<NovaClientInstallResult, String> {
    if launch::is_instance_running(&instance_id)? {
        return Err("Stop Minecraft before installing or updating NOVA Client.".to_owned());
    }
    let data_root = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Unable to resolve NOVA data directory: {error}"))?;
    let (instance_root, marker) = modrinth::read_instance_marker(&data_root, &instance_id).await?;
    if marker.game_version.as_deref() != Some(GAME_VERSION)
        || marker.loader.as_deref() != Some("fabric")
    {
        return Err("NOVA Client requires a Fabric 1.21.11 profile in this release.".to_owned());
    }
    let assets = release_assets()
        .await?
        .ok_or_else(|| "No official NOVA Client release has been published yet.".to_owned())?;
    let jar = download_asset(&assets.jar, MAX_JAR_BYTES).await?;
    if jar.len() as u64 != assets.jar.size {
        return Err("Downloaded NOVA Client size differs from the release metadata".to_owned());
    }
    let checksum_file = download_asset(&assets.checksum, MAX_CHECKSUM_BYTES).await?;
    let expected = parse_checksum(&checksum_file)?;
    let actual = sha256(&jar);
    if actual != expected {
        return Err(
            "NOVA Client JAR failed SHA-256 verification; instance files were not changed"
                .to_owned(),
        );
    }

    let root = canonical_instance(&data_root, &instance_root).await?;
    let mods_dir = root.join("mods");
    match fs::symlink_metadata(&mods_dir).await {
        Ok(metadata) => {
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                return Err("Refusing to install into an unsafe mods directory".to_owned());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(&mods_dir)
                .await
                .map_err(|error| error.to_string())?;
        }
        Err(error) => return Err(error.to_string()),
    }
    let mods_dir = fs::canonicalize(&mods_dir)
        .await
        .map_err(|error| error.to_string())?;
    if !mods_dir.starts_with(&root) {
        return Err("Mods directory resolves outside the managed instance".to_owned());
    }
    let disabled = mods_dir.join("nova-client.jar.disabled");
    match fs::symlink_metadata(&disabled).await {
        Ok(metadata) => {
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err("Unsafe disabled NOVA Client file".to_owned());
            }
            return Err(
                "NOVA Client is disabled in this profile. Re-enable or remove it before updating."
                    .to_owned(),
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }

    let destination = mods_dir.join(MOD_FILE);
    let temp = create_temp(&mods_dir, &jar).await?;
    if let Err(error) = publish_jar(&temp, &destination).await {
        let _ = fs::remove_file(&temp).await;
        return Err(error);
    }
    Ok(NovaClientInstallResult {
        version: assets.info.version,
        file_name: MOD_FILE.to_owned(),
        size_bytes: jar.len() as u64,
        sha256: actual,
    })
}
