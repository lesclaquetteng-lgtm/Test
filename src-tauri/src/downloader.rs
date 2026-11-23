use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, OwnedSemaphorePermit, Semaphore},
    time::sleep,
};
use url::Url;

const GLOBAL_DOWNLOAD_LIMIT: usize = 8;
const PER_HOST_DOWNLOAD_LIMIT: usize = 3;
const MAX_ATTEMPTS: usize = 3;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);
const MAX_REQUEST_DURATION: Duration = Duration::from_secs(20 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const HASH_BUFFER_SIZE: usize = 128 * 1024;

static HTTP_CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();
static GLOBAL_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
static HOST_SLOTS: OnceLock<Mutex<HashMap<String, Arc<Semaphore>>>> = OnceLock::new();
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub(crate) struct DownloadSpec {
    pub url: String,
    pub destination: PathBuf,
    pub expected_sha1: String,
    pub expected_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadProgress {
    pub transferred_bytes: u64,
    pub total_bytes: Option<u64>,
    pub attempt: usize,
}

#[derive(Debug)]
pub(crate) struct DownloadOutcome {
    pub destination: PathBuf,
    pub bytes: u64,
    pub sha1: String,
    pub reused: bool,
}

#[derive(Debug, Error)]
pub(crate) enum DownloadError {
    #[error("Only HTTPS URLs without embedded credentials or fragments are accepted")]
    InvalidUrl,
    #[error("The expected SHA-1 digest must contain exactly 40 hexadecimal characters")]
    InvalidSha1,
    #[error("The download destination must include a file name")]
    InvalidDestination,
    #[error("HTTP client initialization failed: {0}")]
    ClientInitialization(String),
    #[error("Network request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("The remote server returned HTTP {0}")]
    HttpStatus(u16),
    #[error("Artifact size mismatch: expected {expected} bytes, received {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("Artifact SHA-1 verification failed")]
    HashMismatch,
    #[error("Filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("The download scheduler is no longer available")]
    SchedulerClosed,
    #[error("Publishing the verified artifact failed ({publish}); restoring the previous file also failed ({rollback})")]
    PublishRollbackFailed { publish: String, rollback: String },
}

impl DownloadError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Request(error) => error.is_timeout() || error.is_connect() || error.is_body(),
            Self::HttpStatus(status) => *status == 408 || *status == 429 || (500..=599).contains(status),
            Self::SizeMismatch { .. } | Self::HashMismatch => true,
            _ => false,
        }
    }
}

fn http_client() -> Result<&'static Client, DownloadError> {
    match HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(MAX_REQUEST_DURATION)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(PER_HOST_DOWNLOAD_LIMIT)
            .tcp_keepalive(Duration::from_secs(30))
            .user_agent(concat!("NOVA-Launcher/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| error.to_string())
    }) {
        Ok(client) => Ok(client),
        Err(error) => Err(DownloadError::ClientInitialization(error.clone())),
    }
}

fn global_semaphore() -> Arc<Semaphore> {
    GLOBAL_SLOTS
        .get_or_init(|| Arc::new(Semaphore::new(GLOBAL_DOWNLOAD_LIMIT)))
        .clone()
}

fn host_semaphore(host: &str) -> Arc<Semaphore> {
    let slots = HOST_SLOTS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut slots = slots.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    slots
        .entry(host.to_owned())
        .or_insert_with(|| Arc::new(Semaphore::new(PER_HOST_DOWNLOAD_LIMIT)))
        .clone()
}

fn validate_url(raw: &str) -> Result<Url, DownloadError> {
    let url = Url::parse(raw).map_err(|_| DownloadError::InvalidUrl)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(DownloadError::InvalidUrl);
    }
    Ok(url)
}

fn normalize_sha1(value: &str) -> Result<String, DownloadError> {
    let value = value.trim();
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(DownloadError::InvalidSha1);
    }
    Ok(value.to_ascii_lowercase())
}

fn unique_sibling(destination: &Path, suffix: &str) -> Result<PathBuf, DownloadError> {
    let name = destination
        .file_name()
        .ok_or(DownloadError::InvalidDestination)?
        .to_string_lossy();
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(destination.with_file_name(format!(".{name}.{sequence}.{suffix}")))
}

fn digest_to_hex(digest: impl AsRef<[u8]>) -> String {
    let mut output = String::with_capacity(digest.as_ref().len() * 2);
    for byte in digest.as_ref() {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

async fn hash_file(path: &Path) -> Result<(String, u64), DownloadError> {
    let mut file = File::open(path).await?;
    let mut buffer = vec![0_u8; HASH_BUFFER_SIZE];
    let mut hasher = Sha1::new();
    let mut bytes = 0_u64;

    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes = bytes.saturating_add(read as u64);
    }

    Ok((digest_to_hex(hasher.finalize()), bytes))
}

async fn verified_cache_hit(
    destination: &Path,
    expected_sha1: &str,
    expected_size: Option<u64>,
) -> Result<Option<u64>, DownloadError> {
    let metadata = match fs::metadata(destination).await {
        Ok(metadata) if metadata.is_file() => metadata,
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Ok(None),
    };

    let length = metadata.len();
    if expected_size.is_some_and(|expected| expected != length) {
        return Ok(None);
    }

    let (actual_sha1, bytes) = match hash_file(destination).await {
        Ok(result) => result,
        Err(_) => return Ok(None),
    };

    if bytes == length && actual_sha1 == expected_sha1 {
        Ok(Some(bytes))
    } else {
        Ok(None)
    }
}

async fn publish_verified_file(temp: &Path, destination: &Path) -> Result<(), DownloadError> {
    if !fs::try_exists(destination).await? {
        fs::rename(temp, destination).await?;
        return Ok(());
    }

    // Keep the old file recoverable until the verified replacement is in place.
    let backup = unique_sibling(destination, "backup")?;
    fs::rename(destination, &backup).await?;

    if let Err(publish_error) = fs::rename(temp, destination).await {
        return match fs::rename(&backup, destination).await {
            Ok(()) => Err(DownloadError::Io(publish_error)),
            Err(rollback_error) => Err(DownloadError::PublishRollbackFailed {
                publish: publish_error.to_string(),
                rollback: rollback_error.to_string(),
            }),
        };
    }

    // A stale backup is harmless if cleanup fails; never roll back a good artifact for that.
    let _ = fs::remove_file(backup).await;
    Ok(())
}

async fn download_once(
    spec: &DownloadSpec,
    url: &Url,
    expected_sha1: &str,
    attempt: usize,
    progress: Option<&mpsc::Sender<DownloadProgress>>,
) -> Result<DownloadOutcome, DownloadError> {
    let host = url.host_str().ok_or(DownloadError::InvalidUrl)?.to_owned();
    let _global_permit: OwnedSemaphorePermit = global_semaphore()
        .acquire_owned()
        .await
        .map_err(|_| DownloadError::SchedulerClosed)?;
    let _host_permit: OwnedSemaphorePermit = host_semaphore(&host)
        .acquire_owned()
        .await
        .map_err(|_| DownloadError::SchedulerClosed)?;

    let response = http_client()?.get(url.clone()).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(DownloadError::HttpStatus(status.as_u16()));
    }

    if let (Some(expected), Some(actual)) = (spec.expected_size, response.content_length()) {
        if expected != actual {
            return Err(DownloadError::SizeMismatch { expected, actual });
        }
    }

    let total_bytes = spec.expected_size.or_else(|| response.content_length());
    let temp = unique_sibling(&spec.destination, "part")?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .await?;
    let mut stream = response.bytes_stream();
    let mut hasher = Sha1::new();
    let mut transferred_bytes = 0_u64;
    let mut last_progress = Instant::now();

    let transfer_result: Result<(), DownloadError> = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let next = transferred_bytes.saturating_add(chunk.len() as u64);

            if let Some(expected) = spec.expected_size {
                if next > expected {
                    return Err(DownloadError::SizeMismatch {
                        expected,
                        actual: next,
                    });
                }
            }

            file.write_all(&chunk).await?;
            hasher.update(&chunk);
            transferred_bytes = next;
            crate::performance::global().record_downloaded_bytes(chunk.len() as u64);

            if let Some(progress) = progress {
                if last_progress.elapsed() >= PROGRESS_INTERVAL {
                    let _ = progress.try_send(DownloadProgress {
                        transferred_bytes,
                        total_bytes,
                        attempt,
                    });
                    last_progress = Instant::now();
                }
            }
        }

        file.flush().await?;
        file.sync_all().await?;
        Ok(())
    }
    .await;

    drop(file);

    if let Err(error) = transfer_result {
        let _ = fs::remove_file(&temp).await;
        return Err(error);
    }

    if let Some(expected) = spec.expected_size {
        if expected != transferred_bytes {
            let _ = fs::remove_file(&temp).await;
            return Err(DownloadError::SizeMismatch {
                expected,
                actual: transferred_bytes,
            });
        }
    }

    let actual_sha1 = digest_to_hex(hasher.finalize());
    if actual_sha1 != expected_sha1 {
        crate::performance::global().record_verification_failure();
        let _ = fs::remove_file(&temp).await;
        return Err(DownloadError::HashMismatch);
    }

    if let Err(error) = publish_verified_file(&temp, &spec.destination).await {
        let _ = fs::remove_file(&temp).await;
        return Err(error);
    }

    if let Some(progress) = progress {
        let _ = progress.try_send(DownloadProgress {
            transferred_bytes,
            total_bytes,
            attempt,
        });
    }

    Ok(DownloadOutcome {
        destination: spec.destination.clone(),
        bytes: transferred_bytes,
        sha1: actual_sha1,
        reused: false,
    })
}

pub(crate) async fn download_verified(
    spec: DownloadSpec,
    progress: Option<mpsc::Sender<DownloadProgress>>,
) -> Result<DownloadOutcome, DownloadError> {
    let url = validate_url(&spec.url)?;
    let expected_sha1 = normalize_sha1(&spec.expected_sha1)?;
    if spec.destination.file_name().is_none() {
        return Err(DownloadError::InvalidDestination);
    }

    let _operation = crate::performance::global().track();

    if let Some(parent) = spec.destination.parent() {
        fs::create_dir_all(parent).await?;
    }

    if let Some(bytes) =
        verified_cache_hit(&spec.destination, &expected_sha1, spec.expected_size).await?
    {
        crate::performance::global().record_cache_hit();
        crate::performance::global().record_reused_bytes(bytes);
        return Ok(DownloadOutcome {
            destination: spec.destination,
            bytes,
            sha1: expected_sha1,
            reused: true,
        });
    }

    crate::performance::global().record_cache_miss();

    for attempt in 1..=MAX_ATTEMPTS {
        match download_once(&spec, &url, &expected_sha1, attempt, progress.as_ref()).await {
            Ok(outcome) => return Ok(outcome),
            Err(error) if attempt < MAX_ATTEMPTS && error.is_retryable() => {
                sleep(Duration::from_millis(200 * (1_u64 << (attempt - 1)))).await;
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("the bounded retry loop always returns on its final attempt")
}

#[cfg(test)]
mod tests {
    use super::{normalize_sha1, validate_url};

    #[test]
    fn accepts_only_https_artifact_urls() {
        assert!(validate_url("https://resources.download.minecraft.net/ab/file").is_ok());
        assert!(validate_url("http://example.com/file").is_err());
        assert!(validate_url("https://user:password@example.com/file").is_err());
        assert!(validate_url("https://example.com/file#fragment").is_err());
    }

    #[test]
    fn normalizes_and_validates_sha1_digests() {
        let digest = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
        assert_eq!(
            normalize_sha1(digest).expect("valid SHA-1"),
            "abcdef0123456789abcdef0123456789abcdef01"
        );
        assert!(normalize_sha1("not-a-hash").is_err());
    }
}
