use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use tokio::fs;

const MAX_ARCHIVE_ENTRIES: usize = 20_000;
const MAX_SINGLE_FILE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_TOTAL_EXTRACTED_BYTES: u64 = 512 * 1024 * 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
enum NativeError {
    #[error("Native archive is invalid: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Archive I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Native archive contains an unsafe path")]
    UnsafePath,
    #[error("Native archive contains a symbolic link or special file")]
    UnsafeFileType,
    #[error("Native archive contains too many entries")]
    TooManyEntries,
    #[error("Native archive exceeds the extraction size limit")]
    TooLarge,
    #[error("Native archive contains duplicate output paths")]
    DuplicatePath,
    #[error("Native archive extraction produced an unexpected file length")]
    SizeMismatch,
    #[error("Unable to publish extracted natives ({publish}); rollback failed ({rollback})")]
    PublishRollback { publish: String, rollback: String },
    #[error("Native extraction worker failed: {0}")]
    Worker(String),
}

fn safe_entry_path(raw: &str) -> Result<PathBuf, NativeError> {
    let path = Path::new(raw);
    if raw.is_empty() || path.is_absolute() {
        return Err(NativeError::UnsafePath);
    }
    for component in path.components() {
        let Component::Normal(name) = component else {
            return Err(NativeError::UnsafePath);
        };
        let name = name.to_string_lossy();
        let upper = name.to_ascii_uppercase();
        let device_stem = upper.split('.').next().unwrap_or_default();
        if name.is_empty()
            || name.ends_with('.')
            || name.ends_with(' ')
            || name.contains(':')
            || matches!(
                device_stem,
                "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
            )
            || ["COM", "LPT"].iter().any(|prefix| {
                device_stem
                    .strip_prefix(prefix)
                    .is_some_and(|suffix| {
                        suffix.len() == 1
                            && suffix
                                .bytes()
                                .all(|byte| (b'1'..=b'9').contains(&byte))
                    })
            })
        {
            return Err(NativeError::UnsafePath);
        }
    }
    Ok(path.to_path_buf())
}

fn is_excluded(path: &str, exclusions: &[String]) -> bool {
    exclusions.iter().any(|excluded| {
        let normalized = excluded.trim_start_matches("./").replace('\\', "/");
        let normalized = normalized.trim_end_matches('/');
        !normalized.is_empty()
            && (path == normalized
                || path
                    .strip_prefix(normalized)
                    .is_some_and(|rest| rest.starts_with('/')))
    })
}

fn extract_sync(archives: &[(PathBuf, Vec<String>)], stage: &Path) -> Result<u64, NativeError> {
    let mut extracted_bytes = 0_u64;
    let mut written_paths = HashSet::<String>::new();
    let mut total_entries = 0_usize;
    std::fs::create_dir_all(stage)?;

    for (archive_path, exclusions) in archives {
        let file = File::open(archive_path)?;
        if file.metadata()?.len() > MAX_SINGLE_FILE_BYTES {
            return Err(NativeError::TooLarge);
        }
        let mut archive = zip::ZipArchive::new(file)?;
        if archive.len() > MAX_ARCHIVE_ENTRIES
            || total_entries.saturating_add(archive.len()) > MAX_ARCHIVE_ENTRIES
        {
            return Err(NativeError::TooManyEntries);
        }
        total_entries += archive.len();

        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            let Some(enclosed) = entry.enclosed_name() else {
                return Err(NativeError::UnsafePath);
            };
            let relative = safe_entry_path(&enclosed.to_string_lossy())?;
            let relative_text = relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if is_excluded(&relative_text, exclusions) {
                continue;
            }

            if let Some(mode) = entry.unix_mode() {
                let file_type = mode & 0o170000;
                if file_type != 0 && file_type != 0o100000 && file_type != 0o040000 {
                    return Err(NativeError::UnsafeFileType);
                }
            }

            let output_path = stage.join(&relative);
            if entry.is_dir() {
                std::fs::create_dir_all(&output_path)?;
                continue;
            }

            if !written_paths.insert(relative_text.to_ascii_lowercase()) {
                return Err(NativeError::DuplicatePath);
            }

            let declared_size = entry.size();
            if declared_size > MAX_SINGLE_FILE_BYTES
                || extracted_bytes.saturating_add(declared_size) > MAX_TOTAL_EXTRACTED_BYTES
            {
                return Err(NativeError::TooLarge);
            }

            let parent = output_path.parent().ok_or(NativeError::UnsafePath)?;
            std::fs::create_dir_all(parent)?;
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output_path)?;
            let mut buffer = [0_u8; COPY_BUFFER_BYTES];
            let mut written = 0_u64;

            loop {
                let read = entry.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                let next = written.saturating_add(read as u64);
                if next > declared_size
                    || extracted_bytes.saturating_add(next) > MAX_TOTAL_EXTRACTED_BYTES
                {
                    return Err(NativeError::TooLarge);
                }
                output.write_all(&buffer[..read])?;
                written = next;
            }

            if written != declared_size {
                return Err(NativeError::SizeMismatch);
            }
            output.flush()?;
            output.sync_all()?;
            extracted_bytes = extracted_bytes.saturating_add(written);
        }
    }

    Ok(extracted_bytes)
}

async fn publish_directory(stage: &Path, destination: &Path) -> Result<(), NativeError> {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let backup = destination.with_file_name(format!(".natives-backup-{sequence}"));
    if !fs::try_exists(destination).await? {
        fs::rename(stage, destination).await?;
        return Ok(());
    }

    fs::rename(destination, &backup).await?;
    if let Err(publish_error) = fs::rename(stage, destination).await {
        return match fs::rename(&backup, destination).await {
            Ok(()) => Err(NativeError::Io(publish_error)),
            Err(rollback_error) => Err(NativeError::PublishRollback {
                publish: publish_error.to_string(),
                rollback: rollback_error.to_string(),
            }),
        };
    }

    let _ = fs::remove_dir_all(backup).await;
    Ok(())
}

pub(crate) async fn extract_archives(
    archives: Vec<(PathBuf, Vec<String>)>,
    destination: PathBuf,
) -> Result<u64, String> {
    extract_archives_inner(archives, destination)
        .await
        .map_err(|error| error.to_string())
}

async fn extract_archives_inner(
    archives: Vec<(PathBuf, Vec<String>)>,
    destination: PathBuf,
) -> Result<u64, NativeError> {
    let parent = destination.parent().ok_or(NativeError::UnsafePath)?;
    fs::create_dir_all(parent).await?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let stage = parent.join(format!(".natives-stage-{nonce}-{sequence}"));
    let stage_for_worker = stage.clone();

    let extracted = tokio::task::spawn_blocking(move || extract_sync(&archives, &stage_for_worker))
        .await
        .map_err(|error| NativeError::Worker(error.to_string()))?;

    let extracted = match extracted {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = fs::remove_dir_all(&stage).await;
            return Err(error);
        }
    };

    if let Err(error) = publish_directory(&stage, &destination).await {
        let _ = fs::remove_dir_all(&stage).await;
        return Err(error);
    }
    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::{is_excluded, safe_entry_path};
    use std::path::Path;

    #[test]
    fn rejects_archive_path_traversal_and_windows_device_names() {
        assert!(safe_entry_path("META-INF/MANIFEST.MF").is_ok());
        assert!(safe_entry_path("../outside.dll").is_err());
        assert!(safe_entry_path("C:/Windows/system32.dll").is_err());
        assert!(safe_entry_path("CON.txt").is_err());
        assert!(safe_entry_path("folder/../outside.dll").is_err());
        assert!(!Path::new("outside.dll").is_absolute());
    }

    #[test]
    fn applies_library_exclusions_to_exact_paths_and_subtrees() {
        let exclusions = vec!["META-INF/".to_owned(), "module-info.class".to_owned()];
        assert!(is_excluded("META-INF/MANIFEST.MF", &exclusions));
        assert!(is_excluded("module-info.class", &exclusions));
        assert!(!is_excluded("com/example/Main.class", &exclusions));
    }
}
