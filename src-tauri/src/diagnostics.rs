use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Seek},
    path::{Component, Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};
use tokio::process::Command as AsyncCommand;

const MAX_ASSET_OBJECTS: usize = 300_000;
const MAX_LIBRARY_COUNT: usize = 20_000;
const MAX_LOG_BYTES: u64 = 1024 * 1024;
const MAX_CRASH_BYTES: u64 = 512 * 1024;
const MAX_CAPTURED_LINES: usize = 180;
const MAX_PROCESS_LOGS: usize = 2_500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticCheck {
    pub id: String,
    pub category: String,
    pub severity: String,
    pub status: String,
    pub title: String,
    pub details: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    pub recommended_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticSummary {
    pub overall_status: String,
    pub checks_total: usize,
    pub passed: usize,
    pub warnings: usize,
    pub failures: usize,
    pub information: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GpuInformation {
    pub name: String,
    pub driver_version: Option<String>,
    pub adapter_memory_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SystemInformation {
    pub operating_system: String,
    pub architecture: String,
    pub os_version: Option<String>,
    pub os_build: Option<String>,
    pub cpu_model: Option<String>,
    pub logical_processors: usize,
    pub total_memory_bytes: Option<u64>,
    pub gpus: Vec<GpuInformation>,
    pub hardware_query_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JavaInformation {
    pub executable: String,
    pub version: Option<String>,
    pub major_version: Option<u32>,
    pub compatible: Option<bool>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstanceInformation {
    pub id: String,
    pub version_type: Option<String>,
    pub client_status: String,
    pub client_expected_sha1: Option<String>,
    pub client_actual_sha1: Option<String>,
    pub java_required_major: Option<u32>,
    pub java_candidates: Vec<JavaInformation>,
    pub library_count: usize,
    pub missing_libraries: usize,
    pub invalid_libraries: usize,
    pub asset_count: usize,
    pub missing_assets: usize,
    pub invalid_assets: usize,
    pub native_file_count: usize,
    pub latest_log_excerpt: Vec<String>,
    pub crash_report_excerpt: Vec<String>,
    pub crash_artifact_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticLog {
    pub timestamp_unix: u64,
    pub stream: String,
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NetworkEndpoint {
    pub name: String,
    pub host: String,
    pub status_code: Option<u16>,
    pub latency_ms: Option<u128>,
    pub reachable: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticReport {
    pub schema_version: u32,
    pub report_id: String,
    pub generated_at_unix: u64,
    pub trigger: String,
    pub mode: String,
    pub summary: DiagnosticSummary,
    pub system: SystemInformation,
    pub instances: Vec<InstanceInformation>,
    pub network: Vec<NetworkEndpoint>,
    pub checks: Vec<DiagnosticCheck>,
    pub recent_process_logs: Vec<DiagnosticLog>,
    pub report_path: String,
    pub privacy: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallMarker {
    id: String,
    #[serde(default)]
    version_type: Option<String>,
    #[serde(default)]
    client_sha1: Option<String>,
    #[serde(default)]
    asset_index: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetIndex {
    #[serde(default)]
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

#[derive(Debug, Clone)]
struct ProcessLogEntry {
    timestamp_unix: u64,
    instance_id: String,
    stream: String,
    line: String,
}

static PROCESS_LOGS: OnceLock<Mutex<Vec<ProcessLogEntry>>> = OnceLock::new();

fn process_logs() -> &'static Mutex<Vec<ProcessLogEntry>> {
    PROCESS_LOGS.get_or_init(|| Mutex::new(Vec::new()))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn add_check(
    checks: &mut Vec<DiagnosticCheck>,
    id: &str,
    category: &str,
    severity: &str,
    status: &str,
    title: &str,
    details: impl Into<String>,
    evidence: Vec<String>,
    recommended_action: Option<String>,
) {
    checks.push(DiagnosticCheck {
        id: id.to_owned(),
        category: category.to_owned(),
        severity: severity.to_owned(),
        status: status.to_owned(),
        title: title.to_owned(),
        details: details.into(),
        evidence,
        recommended_action,
    });
}

fn safe_file_sha1(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut digest = Sha1::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    let mut output = String::with_capacity(40);
    for byte in digest.finalize() {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    Ok(output)
}

fn safe_file_sha1_under(root: &Path, path: &Path) -> Option<String> {
    let canonical_root = fs::canonicalize(root).ok()?;
    let canonical_path = fs::canonicalize(path).ok()?;
    if !canonical_path.starts_with(&canonical_root) || !canonical_path.is_file() {
        return None;
    }
    safe_file_sha1(&canonical_path).ok()
}

fn safe_relative_path(raw: &str) -> Option<PathBuf> {
    let path = Path::new(raw);
    if raw.is_empty()
        || raw.contains(':')
        || raw.contains('\\')
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    Some(path.to_path_buf())
}

fn library_allowed(library: &Value) -> bool {
    let Some(rules) = library.get("rules").and_then(Value::as_array) else {
        return true;
    };
    if rules.is_empty() {
        return true;
    }
    let os_name = match std::env::consts::OS {
        "macos" => "osx",
        other => other,
    };
    let arch = std::env::consts::ARCH;
    let mut allowed = false;
    for rule in rules {
        let Some(object) = rule.as_object() else {
            continue;
        };
        let mut matches = true;
        if let Some(os) = object.get("os").and_then(Value::as_object) {
            if os
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name != os_name)
            {
                matches = false;
            }
            if os
                .get("arch")
                .and_then(Value::as_str)
                .is_some_and(|value| value != arch)
            {
                matches = false;
            }
        }
        if let Some(features) = object.get("features").and_then(Value::as_object) {
            if features
                .values()
                .any(|value| value.as_bool() != Some(false))
            {
                matches = false;
            }
        }
        if matches {
            allowed = object
                .get("action")
                .and_then(Value::as_str)
                .is_some_and(|action| action == "allow");
        }
    }
    allowed
}

fn count_native_files(root: &Path) -> usize {
    fn walk(path: &Path, depth: usize, total: &mut usize) {
        if depth > 12 || *total > 100_000 {
            return;
        }
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_file() {
                *total += 1;
            } else if kind.is_dir() {
                walk(&entry.path(), depth + 1, total);
            }
            if *total > 100_000 {
                return;
            }
        }
    }
    let mut total = 0;
    walk(root, 0, &mut total);
    total
}

fn redact_text(input: &str) -> String {
    let mut output = input.to_owned();
    let markers = [
        "access_token",
        "accessToken",
        "client_token",
        "clientToken",
        "refresh_token",
        "refreshToken",
        "authorization",
        "password",
        "passwd",
        "secret",
        "xsts_token",
        "user_properties",
    ];
    for marker in markers {
        let mut search_from = 0;
        loop {
            let lower = output.to_ascii_lowercase();
            let marker_lower = marker.to_ascii_lowercase();
            let Some(relative) = lower[search_from..].find(&marker_lower) else {
                break;
            };
            let start = search_from + relative;
            let after_marker = start + marker.len();
            let bytes = output.as_bytes();
            let mut value_start = after_marker;
            while value_start < bytes.len()
                && matches!(bytes[value_start], b' ' | b'\t' | b'"' | b'\'')
            {
                value_start += 1;
            }
            if value_start < bytes.len() && matches!(bytes[value_start], b':' | b'=') {
                value_start += 1;
                while value_start < bytes.len()
                    && matches!(bytes[value_start], b' ' | b'\t' | b'"' | b'\'')
                {
                    value_start += 1;
                }
                let mut value_end = value_start;
                while value_end < bytes.len()
                    && !matches!(bytes[value_end], b' ' | b'\t' | b',' | b';' | b'"' | b'\'')
                {
                    value_end += 1;
                }
                if value_end > value_start {
                    output.replace_range(value_start..value_end, "[REDACTED]");
                    search_from = value_start + "[REDACTED]".len();
                    continue;
                }
            }
            search_from = after_marker.min(output.len());
        }
    }

    let mut lines = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.to_ascii_lowercase().starts_with("authorization:") {
            lines.push("Authorization: [REDACTED]".to_owned());
        } else if trimmed.to_ascii_lowercase().contains("bearer ") {
            let index = line.to_ascii_lowercase().find("bearer ").unwrap_or(0);
            lines.push(format!("{}Bearer [REDACTED]", &line[..index]));
        } else {
            lines.push(line.to_owned());
        }
    }
    output = lines.join("\n");

    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let home = home.to_string_lossy();
        if home.len() > 4 {
            output = output.replace(home.as_ref(), "%USER_HOME%");
            output = output.replace(&home.replace('\\', "/"), "%USER_HOME%");
        }
    }
    output
}

pub(crate) fn record_process_log(instance_id: &str, stream: &str, line: &str) -> String {
    let sanitized = redact_text(line);
    let sanitized = if sanitized.len() > 16 * 1024 {
        sanitized.chars().take(16 * 1024).collect::<String>()
    } else {
        sanitized
    };
    if let Ok(mut entries) = process_logs().lock() {
        entries.push(ProcessLogEntry {
            timestamp_unix: unix_now(),
            instance_id: instance_id.to_owned(),
            stream: stream.to_owned(),
            line: sanitized.clone(),
        });
        if entries.len() > MAX_PROCESS_LOGS {
            let excess = entries.len() - MAX_PROCESS_LOGS;
            entries.drain(0..excess);
        }
    }
    sanitized
}

fn process_log_snapshot(instance_id: Option<&str>) -> Vec<DiagnosticLog> {
    let Ok(entries) = process_logs().lock() else {
        return Vec::new();
    };
    entries
        .iter()
        .filter(|entry| instance_id.is_none_or(|id| entry.instance_id == id))
        .rev()
        .take(MAX_CAPTURED_LINES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|entry| DiagnosticLog {
            timestamp_unix: entry.timestamp_unix,
            stream: entry.stream.clone(),
            line: redact_text(&entry.line),
        })
        .collect()
}

fn excerpt_from_file(
    allowed_root: &Path,
    path: &Path,
    max_bytes: u64,
    max_lines: usize,
) -> Vec<String> {
    let Ok(canonical_root) = fs::canonicalize(allowed_root) else {
        return Vec::new();
    };
    let Ok(canonical_path) = fs::canonicalize(path) else {
        return Vec::new();
    };
    if !canonical_path.starts_with(&canonical_root) || !canonical_path.is_file() {
        return Vec::new();
    }
    let Ok(metadata) = fs::metadata(&canonical_path) else {
        return Vec::new();
    };
    let start = metadata.len().saturating_sub(max_bytes);
    let Ok(mut file) = File::open(&canonical_path) else {
        return Vec::new();
    };
    if start > 0 && file.seek(std::io::SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    if file.take(max_bytes).read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines = text.lines().map(redact_text).collect::<Vec<_>>();
    lines
        .into_iter()
        .rev()
        .take(max_lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn latest_file_with_prefix(root: &Path, prefix: &str, extension: Option<&str>) -> Option<PathBuf> {
    let entries = fs::read_dir(root).ok()?;
    entries
        .flatten()
        .filter_map(|entry| {
            if !entry.file_type().ok()?.is_file() {
                return None;
            }
            let path = entry.path();
            let name = path.file_name()?.to_string_lossy();
            if !name.starts_with(prefix) {
                return None;
            }
            if let Some(extension) = extension {
                if path.extension().and_then(|value| value.to_str()) != Some(extension) {
                    return None;
                }
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((path, modified))
        })
        .max_by_key(|(_, modified)| *modified)
        .map(|(path, _)| path)
}

fn read_powershell_system(output: &str) -> Option<SystemInformation> {
    let value: Value = serde_json::from_str(output.trim()).ok()?;
    let strings = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let gpus = value
        .get("gpus")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|gpu| {
            Some(GpuInformation {
                name: gpu.get("name")?.as_str()?.to_owned(),
                driver_version: gpu
                    .get("driverVersion")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                adapter_memory_bytes: gpu.get("adapterMemoryBytes").and_then(Value::as_u64),
            })
        })
        .collect();
    Some(SystemInformation {
        operating_system: strings("osCaption").unwrap_or_else(|| std::env::consts::OS.to_owned()),
        architecture: std::env::consts::ARCH.to_owned(),
        os_version: strings("osVersion"),
        os_build: strings("osBuild"),
        cpu_model: strings("cpuName").or_else(|| std::env::var("PROCESSOR_IDENTIFIER").ok()),
        logical_processors: value
            .get("logicalProcessors")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .or_else(|| std::thread::available_parallelism().ok().map(usize::from))
            .unwrap_or(1),
        total_memory_bytes: value.get("memoryTotalBytes").and_then(Value::as_u64),
        gpus,
        hardware_query_status: "complete".to_owned(),
    })
}

async fn probe_network_endpoint(
    client: &reqwest::Client,
    name: &'static str,
    host: &'static str,
    url: &'static str,
    get: bool,
) -> NetworkEndpoint {
    let started = std::time::Instant::now();
    let request = if get {
        client.get(url)
    } else {
        client.head(url)
    };
    match tokio::time::timeout(Duration::from_secs(9), request.send()).await {
        Ok(Ok(response)) => {
            let status = response.status();
            NetworkEndpoint {
                name: name.to_owned(),
                host: host.to_owned(),
                status_code: Some(status.as_u16()),
                latency_ms: Some(started.elapsed().as_millis()),
                reachable: true,
                detail: format!(
                    "Host responded with HTTP {}. A response confirms DNS, TCP and TLS connectivity.",
                    status.as_u16()
                ),
            }
        }
        Ok(Err(error)) => NetworkEndpoint {
            name: name.to_owned(),
            host: host.to_owned(),
            status_code: None,
            latency_ms: Some(started.elapsed().as_millis()),
            reachable: false,
            detail: redact_text(&error.to_string()),
        },
        Err(_) => NetworkEndpoint {
            name: name.to_owned(),
            host: host.to_owned(),
            status_code: None,
            latency_ms: Some(started.elapsed().as_millis()),
            reachable: false,
            detail: "Connection timed out after the configured diagnostic timeout.".to_owned(),
        },
    }
}

async fn collect_network_information() -> Vec<NetworkEndpoint> {
    let client = match reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(8))
        .user_agent(concat!("NOVA-Launcher/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return vec![NetworkEndpoint {
                name: "HTTP client".to_owned(),
                host: "local runtime".to_owned(),
                status_code: None,
                latency_ms: None,
                reachable: false,
                detail: redact_text(&error.to_string()),
            }];
        }
    };

    futures_util::future::join_all([
        probe_network_endpoint(
            &client,
            "Mojang version manifest",
            "piston-meta.mojang.com",
            "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
            true,
        ),
        probe_network_endpoint(
            &client,
            "Minecraft libraries CDN",
            "libraries.minecraft.net",
            "https://libraries.minecraft.net/",
            false,
        ),
        probe_network_endpoint(
            &client,
            "Minecraft assets CDN",
            "resources.download.minecraft.net",
            "https://resources.download.minecraft.net/",
            false,
        ),
    ])
    .await
}
async fn collect_system_information() -> SystemInformation {
    if cfg!(windows) {
        let script = r#"$os=Get-CimInstance Win32_OperatingSystem; $cpu=Get-CimInstance Win32_Processor | Select-Object -First 1; $cs=Get-CimInstance Win32_ComputerSystem; $gpus=@(Get-CimInstance Win32_VideoController | ForEach-Object { [pscustomobject]@{ name=$_.Name; driverVersion=$_.DriverVersion; adapterMemoryBytes=[uint64]$_.AdapterRAM } }); [pscustomobject]@{ osCaption=$os.Caption; osVersion=$os.Version; osBuild=[string]$os.BuildNumber; cpuName=$cpu.Name; logicalProcessors=$cpu.NumberOfLogicalProcessors; memoryTotalBytes=[uint64]$cs.TotalPhysicalMemory; gpus=$gpus } | ConvertTo-Json -Compress -Depth 4"#;
        let command = AsyncCommand::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ])
            .kill_on_drop(true)
            .output();
        if let Ok(Ok(output)) = tokio::time::timeout(Duration::from_secs(8), command).await {
            if output.status.success() {
                if let Some(info) = read_powershell_system(&String::from_utf8_lossy(&output.stdout))
                {
                    return info;
                }
            }
        }
    }

    let cpu_model = std::env::var("PROCESSOR_IDENTIFIER")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            fs::read_to_string("/proc/cpuinfo")
                .ok()
                .and_then(|contents| {
                    contents.lines().find_map(|line| {
                        line.strip_prefix("model name\t:")
                            .map(str::trim)
                            .map(str::to_owned)
                    })
                })
        });
    let total_memory_bytes = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|contents| {
            contents
                .lines()
                .find_map(|line| line.strip_prefix("MemTotal:"))
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
                .map(|kilobytes| kilobytes.saturating_mul(1024))
        });
    SystemInformation {
        operating_system: std::env::consts::OS.to_owned(),
        architecture: std::env::consts::ARCH.to_owned(),
        os_version: None,
        os_build: None,
        cpu_model,
        logical_processors: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
        total_memory_bytes,
        gpus: Vec::new(),
        hardware_query_status: "partial; platform-specific hardware query unavailable".to_owned(),
    }
}

fn inspect_instance(
    data_root: &Path,
    cache_root: &Path,
    instance_id: &str,
    deep: bool,
    checks: &mut Vec<DiagnosticCheck>,
) -> InstanceInformation {
    let instance_root = data_root.join("instances").join(instance_id);
    let marker_path = instance_root.join(".nova-install.json");
    let marker: Option<InstallMarker> = fs::read(&marker_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());

    if marker
        .as_ref()
        .is_some_and(|marker| marker.id != instance_id)
    {
        add_check(
            checks,
            &format!("instance.{instance_id}.marker"),
            "instance",
            "error",
            "failed",
            "Instance marker does not match its directory",
            "The installation marker identifies a different version than the directory name.",
            vec![format!("instances/{instance_id}/.nova-install.json")],
            Some(
                "Reinstall this Minecraft version so NOVA can rebuild its installation marker."
                    .to_owned(),
            ),
        );
    } else if marker.is_none() {
        add_check(
            checks,
            &format!("instance.{instance_id}.marker"),
            "instance",
            "error",
            "failed",
            "Installation marker is missing or invalid",
            "NOVA cannot prove that this instance completed a verified installation.",
            vec![format!("instances/{instance_id}/.nova-install.json")],
            Some("Reinstall this instance from the official version catalogue.".to_owned()),
        );
    } else {
        add_check(
            checks,
            &format!("instance.{instance_id}.marker"),
            "instance",
            "info",
            "passed",
            "Installation marker is valid",
            "The marker exists, parses and matches the instance identifier.",
            vec![format!("instances/{instance_id}/.nova-install.json")],
            None,
        );
    }

    let marker = marker.unwrap_or(InstallMarker {
        id: instance_id.to_owned(),
        version_type: None,
        client_sha1: None,
        asset_index: None,
    });
    let version_root = instance_root.join("versions").join(instance_id);
    let client_path = version_root.join(format!("{instance_id}.jar"));
    let expected_client = marker.client_sha1.clone();
    let actual_client = safe_file_sha1_under(&version_root, &client_path);
    let client_status = match (&expected_client, &actual_client) {
        (Some(expected), Some(actual)) if expected.eq_ignore_ascii_case(actual) => "verified",
        (Some(_), Some(_)) => "hash_mismatch",
        (_, None) => "missing",
        _ => "unverified",
    };
    add_check(
        checks,
        &format!("instance.{instance_id}.client"),
        "integrity",
        if client_status == "verified" { "info" } else { "error" },
        if client_status == "verified" { "passed" } else { "failed" },
        "Minecraft client JAR integrity",
        match client_status {
            "verified" => "The installed client JAR SHA-1 matches the recorded official hash.",
            "hash_mismatch" => "The installed client JAR does not match the SHA-1 recorded in the installation marker.",
            "missing" => "The Minecraft client JAR is missing or unreadable.",
            _ => "The installation marker does not contain a verifiable client SHA-1.",
        },
        vec![
            format!("instances/{instance_id}/versions/{instance_id}/{instance_id}.jar"),
            format!("expected: {}", expected_client.as_deref().unwrap_or("unavailable")),
            format!("actual: {}", actual_client.as_deref().unwrap_or("unavailable")),
        ],
        if client_status == "verified" { None } else { Some("Reinstall this version; do not launch a client that failed integrity verification.".to_owned()) },
    );

    let metadata_path = cache_root
        .join("mojang")
        .join("versions")
        .join(format!("{instance_id}.json"));
    let metadata_bytes = fs::read(&metadata_path).ok();
    let metadata: Option<Value> = metadata_bytes
        .as_ref()
        .and_then(|bytes| serde_json::from_slice(bytes).ok());
    let metadata_valid = metadata
        .as_ref()
        .is_some_and(|value| value.get("id").and_then(Value::as_str) == Some(instance_id));
    add_check(
        checks,
        &format!("instance.{instance_id}.metadata"),
        "metadata",
        if metadata_valid { "info" } else { "error" },
        if metadata_valid { "passed" } else { "failed" },
        "Official version metadata",
        if metadata_valid {
            "Cached version metadata parses and its id matches this instance."
        } else {
            "Version metadata is missing, malformed or belongs to another version."
        },
        vec![format!("mojang/versions/{instance_id}.json")],
        if metadata_valid {
            None
        } else {
            Some("Refresh the official catalogue and reinstall this instance.".to_owned())
        },
    );

    let required_java = metadata
        .as_ref()
        .and_then(|value| value.get("javaVersion"))
        .and_then(|value| value.get("majorVersion"))
        .and_then(Value::as_u64)
        .map(|value| value as u32);
    let mut java_candidates = Vec::new();
    let mut java_paths = Vec::new();
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        java_paths.push((
            PathBuf::from(home)
                .join("bin")
                .join(if cfg!(windows) { "java.exe" } else { "java" }),
            "JAVA_HOME".to_owned(),
        ));
    }
    java_paths.push((
        PathBuf::from(if cfg!(windows) { "java.exe" } else { "java" }),
        "PATH".to_owned(),
    ));
    for (path, source) in java_paths {
        let output = Command::new(&path).arg("-version").output();
        match output {
            Ok(output) if output.status.success() => {
                let version_text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stderr),
                    String::from_utf8_lossy(&output.stdout)
                );
                let version = version_text
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .map(str::trim)
                    .map(str::to_owned);
                let major = version.as_deref().and_then(parse_java_major);
                let compatible =
                    required_java.map(|required| major.is_some_and(|value| value >= required));
                java_candidates.push(JavaInformation {
                    executable: sanitize_path(&path),
                    version,
                    major_version: major,
                    compatible,
                    source,
                });
            }
            Ok(output) => java_candidates.push(JavaInformation {
                executable: sanitize_path(&path),
                version: Some(format!("java -version exited with {}", output.status)),
                major_version: None,
                compatible: Some(false),
                source,
            }),
            Err(error) => java_candidates.push(JavaInformation {
                executable: sanitize_path(&path),
                version: Some(error.to_string()),
                major_version: None,
                compatible: Some(false),
                source,
            }),
        }
    }
    let java_compatible = java_candidates
        .iter()
        .any(|candidate| candidate.compatible == Some(true));
    add_check(
        checks,
        &format!("instance.{instance_id}.java"),
        "runtime",
        if java_compatible { "info" } else { "error" },
        if java_compatible { "passed" } else { "failed" },
        "Compatible Java runtime",
        if java_compatible {
            format!(
                "At least one detected Java runtime satisfies the required major version {}.",
                required_java
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "8 (default)".to_owned())
            )
        } else {
            format!(
                "No detected Java runtime satisfies the required major version {}.",
                required_java
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "8 (default)".to_owned())
            )
        },
        java_candidates
            .iter()
            .map(|candidate| {
                format!(
                    "{} · {} · {}",
                    candidate.source,
                    candidate.version.as_deref().unwrap_or("unknown"),
                    candidate.executable
                )
            })
            .collect(),
        if java_compatible {
            None
        } else {
            Some("Install the required Java major version and set JAVA_HOME to its installation directory.".to_owned())
        },
    );

    let libraries = metadata
        .as_ref()
        .and_then(|value| value.get("libraries"))
        .and_then(Value::as_array);
    let libraries_root = data_root.join("shared").join("libraries");
    let canonical_libraries_root = fs::canonicalize(&libraries_root).ok();
    let mut library_count = 0usize;
    let mut missing_libraries = 0usize;
    let mut invalid_libraries = 0usize;
    if let Some(libraries) = libraries {
        if libraries.len() > MAX_LIBRARY_COUNT {
            invalid_libraries += 1;
        }
        for library in libraries
            .iter()
            .take(MAX_LIBRARY_COUNT)
            .filter(|library| library_allowed(library))
        {
            let Some(artifact) = library
                .get("downloads")
                .and_then(|value| value.get("artifact"))
            else {
                continue;
            };
            let Some(relative_raw) = artifact.get("path").and_then(Value::as_str) else {
                invalid_libraries += 1;
                continue;
            };
            let Some(relative) = safe_relative_path(relative_raw) else {
                invalid_libraries += 1;
                continue;
            };
            library_count += 1;
            let path = libraries_root.join(relative);
            let expected_size = artifact.get("size").and_then(Value::as_u64);
            let expected_hash = artifact.get("sha1").and_then(Value::as_str);
            match fs::canonicalize(&path) {
                Ok(canonical)
                    if canonical_libraries_root
                        .as_ref()
                        .is_some_and(|root| canonical.starts_with(root)) =>
                {
                    match fs::metadata(&canonical) {
                        Ok(metadata) if metadata.is_file() => {
                            if expected_size.is_some_and(|size| size != metadata.len()) {
                                invalid_libraries += 1;
                            } else if deep {
                                if let Some(expected) = expected_hash {
                                    if safe_file_sha1_under(&libraries_root, &canonical)
                                        .is_none_or(|actual| !actual.eq_ignore_ascii_case(expected))
                                    {
                                        invalid_libraries += 1;
                                    }
                                }
                            }
                        }
                        _ => missing_libraries += 1,
                    }
                }
                Ok(_) => invalid_libraries += 1,
                Err(_) => missing_libraries += 1,
            }
        }
    }
    add_check(
        checks,
        &format!("instance.{instance_id}.libraries"),
        "integrity",
        if missing_libraries == 0 && invalid_libraries == 0 { "info" } else { "error" },
        if missing_libraries == 0 && invalid_libraries == 0 { "passed" } else { "failed" },
        "Minecraft libraries",
        format!("{library_count} applicable libraries scanned; {missing_libraries} missing; {invalid_libraries} invalid. {}", if deep { "Deep mode verified SHA-1 where official hashes were available." } else { "Quick mode checked presence and declared sizes." }),
        vec![format!("shared libraries scanned: {library_count}")],
        if missing_libraries == 0 && invalid_libraries == 0 { None } else { Some("Repair the instance by reinstalling the affected Minecraft version.".to_owned()) },
    );

    let asset_index_id = marker.asset_index.clone().unwrap_or_default();
    let asset_index_path = data_root
        .join("shared")
        .join("assets")
        .join("indexes")
        .join(format!("{asset_index_id}.json"));
    let asset_index: Option<AssetIndex> = fs::read(&asset_index_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let assets_objects_root = data_root
        .join("shared")
        .join("assets")
        .join("objects");
    let canonical_assets_root = fs::canonicalize(&assets_objects_root).ok();
    let mut asset_count = 0usize;
    let mut missing_assets = 0usize;
    let mut invalid_assets = 0usize;
    if let Some(index) = asset_index.as_ref() {
        asset_count = index.objects.len();
        if asset_count > MAX_ASSET_OBJECTS {
            invalid_assets += 1;
        } else {
            for object in index.objects.values() {
                if object.hash.len() != 40
                    || !object.hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    invalid_assets += 1;
                    continue;
                }
                let hash = object.hash.to_ascii_lowercase();
                let path = data_root
                    .join("shared")
                    .join("assets")
                    .join("objects")
                    .join(&hash[..2])
                    .join(&hash);
                match fs::canonicalize(&path) {
                    Ok(canonical)
                        if canonical_assets_root
                            .as_ref()
                            .is_some_and(|root| canonical.starts_with(root)) =>
                    {
                        match fs::metadata(&canonical) {
                            Ok(metadata) if metadata.is_file() => {
                                if metadata.len() != object.size {
                                    invalid_assets += 1;
                                } else if deep
                                    && safe_file_sha1_under(&assets_objects_root, &canonical)
                                        .is_none_or(|actual| !actual.eq_ignore_ascii_case(&hash))
                                {
                                    invalid_assets += 1;
                                }
                            }
                            _ => missing_assets += 1,
                        }
                    }
                    Ok(_) => invalid_assets += 1,
                    Err(_) => missing_assets += 1,
                }
            }
        }
    } else {
        invalid_assets += 1;
    }
    add_check(
        checks,
        &format!("instance.{instance_id}.assets"),
        "integrity",
        if missing_assets == 0 && invalid_assets == 0 {
            "info"
        } else {
            "error"
        },
        if missing_assets == 0 && invalid_assets == 0 {
            "passed"
        } else {
            "failed"
        },
        "Minecraft assets",
        if asset_index.is_some() {
            format!("{asset_count} asset objects scanned; {missing_assets} missing; {invalid_assets} invalid. {}", if deep { "Every indexed object was SHA-1 verified." } else { "Quick mode checked object presence and file size." })
        } else {
            "The asset index is missing or invalid.".to_owned()
        },
        vec![format!("assets/indexes/{asset_index_id}.json")],
        if missing_assets == 0 && invalid_assets == 0 {
            None
        } else {
            Some(
                "Run the installer again to restore missing or corrupted asset objects.".to_owned(),
            )
        },
    );

    let natives_root = instance_root.join("natives");
    let native_file_count = count_native_files(&natives_root);
    add_check(
        checks,
        &format!("instance.{instance_id}.natives"),
        "runtime",
        if native_file_count > 0 {
            "info"
        } else {
            "error"
        },
        if native_file_count > 0 {
            "passed"
        } else {
            "failed"
        },
        "Extracted native libraries",
        format!("{native_file_count} native files found in the isolated instance directory."),
        vec![format!("instances/{instance_id}/natives")],
        if native_file_count > 0 {
            None
        } else {
            Some(
                "Reinstall the version to download and extract its platform-native libraries."
                    .to_owned(),
            )
        },
    );

    let logs_root = instance_root.join("logs");
    let latest_log_path = logs_root.join("latest.log");
    let latest_log_excerpt = excerpt_from_file(
        &instance_root,
        &latest_log_path,
        MAX_LOG_BYTES,
        MAX_CAPTURED_LINES,
    );
    if latest_log_excerpt.is_empty() {
        add_check(
            checks,
            &format!("instance.{instance_id}.latest-log"),
            "logs",
            "info",
            "skipped",
            "Minecraft latest.log",
            "No latest.log was found yet. It is created after Minecraft starts.",
            vec!["logs/latest.log".to_owned()],
            None,
        );
    } else {
        add_check(
            checks,
            &format!("instance.{instance_id}.latest-log"),
            "logs",
            "info",
            "passed",
            "Minecraft latest.log captured",
            format!(
                "Captured the last {} lines (bounded to {} KiB).",
                latest_log_excerpt.len(),
                MAX_LOG_BYTES / 1024
            ),
            vec!["logs/latest.log".to_owned()],
            None,
        );
    }

    let crash_root = instance_root.join("crash-reports");
    let latest_crash = latest_file_with_prefix(&crash_root, "crash-", Some("txt"));
    let hs_err = latest_file_with_prefix(&instance_root, "hs_err_pid", Some("log"));
    let mut crash_artifact_names = Vec::new();
    let mut crash_report_excerpt = Vec::new();
    if let Some(path) = latest_crash {
        crash_artifact_names.push(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        );
        crash_report_excerpt.extend(excerpt_from_file(
            &crash_root,
            &path,
            MAX_CRASH_BYTES,
            MAX_CAPTURED_LINES,
        ));
    }
    if let Some(path) = hs_err {
        crash_artifact_names.push(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        );
        crash_report_excerpt.extend(excerpt_from_file(
            &instance_root,
            &path,
            MAX_CRASH_BYTES,
            MAX_CAPTURED_LINES,
        ));
    }
    add_check(
        checks,
        &format!("instance.{instance_id}.crash-artifacts"),
        "logs",
        if crash_artifact_names.is_empty() {
            "info"
        } else {
            "warning"
        },
        if crash_artifact_names.is_empty() {
            "passed"
        } else {
            "warning"
        },
        "Crash artifacts",
        if crash_artifact_names.is_empty() {
            "No Minecraft crash report or JVM hs_err file was found.".to_owned()
        } else {
            format!(
                "{} crash artifact(s) found; sanitized excerpts are attached to this report.",
                crash_artifact_names.len()
            )
        },
        crash_artifact_names.clone(),
        None,
    );

    InstanceInformation {
        id: instance_id.to_owned(),
        version_type: marker.version_type,
        client_status: client_status.to_owned(),
        client_expected_sha1: expected_client,
        client_actual_sha1: actual_client,
        java_required_major: required_java,
        java_candidates,
        library_count,
        missing_libraries,
        invalid_libraries,
        asset_count,
        missing_assets,
        invalid_assets,
        native_file_count,
        latest_log_excerpt,
        crash_report_excerpt,
        crash_artifact_names,
    }
}

fn parse_java_major(output: &str) -> Option<u32> {
    let version = output.split('"').nth(1)?;
    let mut parts = version.split('.');
    let first = parts.next()?.parse::<u32>().ok()?;
    if first == 1 {
        parts.next()?.parse::<u32>().ok()
    } else {
        Some(first)
    }
}

fn sanitize_path(path: &Path) -> String {
    redact_text(&path.to_string_lossy())
}

fn summarize(checks: &[DiagnosticCheck]) -> DiagnosticSummary {
    let passed = checks
        .iter()
        .filter(|check| check.status == "passed")
        .count();
    let warnings = checks
        .iter()
        .filter(|check| check.status == "warning")
        .count();
    let failures = checks
        .iter()
        .filter(|check| check.status == "failed")
        .count();
    let information = checks
        .iter()
        .filter(|check| check.status == "skipped")
        .count();
    DiagnosticSummary {
        overall_status: if failures > 0 {
            "critical".to_owned()
        } else if warnings > 0 {
            "warning".to_owned()
        } else {
            "healthy".to_owned()
        },
        checks_total: checks.len(),
        passed,
        warnings,
        failures,
        information,
    }
}

fn report_id() -> String {
    format!("nova-{}-{}", unix_now(), std::process::id())
}

pub(crate) async fn run_report(
    app: AppHandle,
    data_root: PathBuf,
    cache_root: PathBuf,
    target_instance: Option<String>,
    deep: bool,
    trigger: String,
) -> Result<DiagnosticReport, String> {
    let generated_at = unix_now();
    let id = report_id();
    let (system, network) = tokio::join!(
        collect_system_information(),
        collect_network_information()
    );
    let logs = process_log_snapshot(target_instance.as_deref());
    let data_for_worker = data_root.clone();
    let cache_for_worker = cache_root.clone();
    let target_for_worker = target_instance.clone();
    let mut checks = Vec::new();

    add_check(
        &mut checks,
        "system.platform",
        "system",
        "info",
        "passed",
        "Operating system and architecture",
        format!("{} · {}", system.operating_system, system.architecture),
        vec![
            format!(
                "version: {}",
                system.os_version.as_deref().unwrap_or("not exposed")
            ),
            format!(
                "build: {}",
                system.os_build.as_deref().unwrap_or("not exposed")
            ),
        ],
        None,
    );
    for endpoint in &network {
        add_check(
            &mut checks,
            &format!("network.{}", endpoint.host),
            "network",
            if endpoint.reachable { "info" } else { "warning" },
            if endpoint.reachable { "passed" } else { "warning" },
            &endpoint.name,
            &endpoint.detail,
            vec![
                format!("host: {}", endpoint.host),
                format!("HTTP status: {}", endpoint.status_code.map(|status| status.to_string()).unwrap_or_else(|| "no response".to_owned())),
                format!("latency: {}", endpoint.latency_ms.map(|value| format!("{value} ms")).unwrap_or_else(|| "unavailable".to_owned())),
            ],
            if endpoint.reachable { None } else { Some("Check DNS, firewall, proxy, VPN and TLS inspection settings, then retry the diagnostic.".to_owned()) },
        );
    }

    add_check(
        &mut checks,
        "system.hardware",
        "system",
        if system.hardware_query_status == "complete" {
            "info"
        } else {
            "warning"
        },
        if system.hardware_query_status == "complete" {
            "passed"
        } else {
            "warning"
        },
        "Hardware inventory",
        system.hardware_query_status.clone(),
        vec![
            format!(
                "CPU: {}",
                system.cpu_model.as_deref().unwrap_or("not exposed")
            ),
            format!("logical processors: {}", system.logical_processors),
            format!(
                "RAM: {}",
                system
                    .total_memory_bytes
                    .map(|value| format!("{:.1} GiB", value as f64 / 1073741824.0))
                    .unwrap_or_else(|| "not exposed".to_owned())
            ),
            format!("GPU devices: {}", system.gpus.len()),
        ],
        None,
    );

    let data_root_for_scan = data_for_worker.clone();
    let scan = tokio::task::spawn_blocking(move || {
        let instances_root = data_root_for_scan.join("instances");
        let mut ids = Vec::new();
        if let Some(id) = target_for_worker {
            if safe_id(&id) {
                ids.push(id);
            }
        } else if let Ok(entries) = fs::read_dir(&instances_root) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    if let Some(id) = entry.file_name().to_str().map(str::to_owned) {
                        if safe_id(&id) {
                            ids.push(id);
                        }
                    }
                }
            }
        }
        ids.sort();
        let mut scan_checks = Vec::new();
        let mut instances = Vec::new();
        for id in ids {
            instances.push(inspect_instance(
                &data_root_for_scan,
                &cache_for_worker,
                &id,
                deep,
                &mut scan_checks,
            ));
        }
        (instances, scan_checks)
    })
    .await
    .map_err(|error| format!("Diagnostic scan worker failed: {error}"))?;
    let (instances, instance_checks) = scan;
    checks.extend(instance_checks);

    if instances.is_empty() {
        add_check(
            &mut checks,
            "instances.none",
            "instance",
            "info",
            "skipped",
            "Installed Minecraft instances",
            "No NOVA-managed instance was found in the application data directory.",
            vec!["instances/".to_owned()],
            Some("Install an official Minecraft version from the Instances page.".to_owned()),
        );
    }

    let summary = summarize(&checks);
    let privacy = vec![
        "Reports are generated locally and are never uploaded automatically.".to_owned(),
        "Authentication tokens, passwords, authorization headers and secret fields are redacted.".to_owned(),
        "User home paths are replaced with %USER_HOME% in captured text.".to_owned(),
        "No full environment-variable dump, local IP inventory, MAC address, hardware serial or process list is collected.".to_owned(),
        "Connectivity probes contact only the public Minecraft metadata, libraries and assets endpoints; those services necessarily see the public IP address of the network connection.".to_owned(),
        "Crash excerpts and game logs may still contain server names, chat text or player identifiers; review the report before sharing it.".to_owned(),
    ];
    let report = DiagnosticReport {
        schema_version: 1,
        report_id: id.clone(),
        generated_at_unix: generated_at,
        trigger,
        mode: if deep { "deep" } else { "quick" }.to_owned(),
        summary,
        system,
        instances,
        network,
        checks,
        recent_process_logs: logs,
        report_path: format!("%NOVA_DATA%/diagnostics/reports/{id}.json"),
        privacy,
    };

    let reports_root = data_root.join("diagnostics").join("reports");
    tokio::fs::create_dir_all(&reports_root)
        .await
        .map_err(|error| format!("Unable to create diagnostic report directory: {error}"))?;
    let report_path = reports_root.join(format!("{id}.json"));
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("Unable to serialize diagnostic report: {error}"))?;
    tokio::fs::write(&report_path, bytes)
        .await
        .map_err(|error| format!("Unable to save diagnostic report: {error}"))?;

    let _ = app.emit("nova-diagnostic-report-ready", &report);
    Ok(report)
}

pub(crate) fn open_report_folder(data_root: &Path) -> Result<(), String> {
    let path = data_root.join("diagnostics").join("reports");
    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    #[cfg(windows)]
    let result = Command::new("explorer.exe").arg(&path).spawn();
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(&path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(&path).spawn();
    result
        .map(|_| ())
        .map_err(|error| format!("Unable to open diagnostic reports folder: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{parse_java_major, redact_text, safe_id, safe_relative_path};

    #[test]
    fn redacts_sensitive_fields_and_bearer_tokens() {
        let value = r#"accessToken="abc123" Authorization: Bearer xyz789 password=hello"#;
        let redacted = redact_text(value);
        assert!(!redacted.contains("abc123"));
        assert!(!redacted.contains("xyz789"));
        assert!(!redacted.contains("hello"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn validates_instance_ids_and_relative_paths() {
        assert!(safe_id("1.21.11"));
        assert!(!safe_id("../outside"));
        assert!(safe_relative_path("versions/1.21.11/client.jar").is_some());
        assert!(safe_relative_path("../../outside.jar").is_none());
        assert!(safe_relative_path("C:/outside.jar").is_none());
        assert!(safe_relative_path("folder\\outside.jar").is_none());
    }

    #[test]
    fn parses_java_eight_and_modern_versions() {
        assert_eq!(parse_java_major("java version \"1.8.0_402\""), Some(8));
        assert_eq!(parse_java_major("openjdk version \"21.0.2\""), Some(21));
    }
}
