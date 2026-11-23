use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::watch,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchSession {
    pub instance_id: String,
    pub pid: u32,
    pub started_at_unix: u64,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessStateEvent {
    instance_id: String,
    state: String,
    pid: Option<u32>,
    exit_code: Option<i32>,
    message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessLogEvent {
    instance_id: String,
    stream: String,
    line: String,
}

struct ProcessSession {
    snapshot: LaunchSession,
    stop: watch::Sender<bool>,
}

static SESSIONS: OnceLock<Mutex<HashMap<String, ProcessSession>>> = OnceLock::new();

fn sessions() -> &'static Mutex<HashMap<String, ProcessSession>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
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

fn safe_relative_path(raw: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    if raw.is_empty()
        || raw.contains(':')
        || raw.contains('\\')
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("Unsafe path in Minecraft metadata: {raw}"));
    }
    Ok(path.to_path_buf())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallMarker {
    id: String,
    client_sha1: String,
    asset_index: String,
    #[serde(default)]
    game_version: Option<String>,
    #[serde(default)]
    loader: Option<String>,
    #[serde(default)]
    loader_version: Option<String>,
    #[serde(default)]
    instance_type: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    modrinth_project_id: Option<String>,
    #[serde(default)]
    modrinth_version_id: Option<String>,
}

fn sha1_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("Unable to open Minecraft client: {error}"))?;
    let mut digest = Sha1::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Unable to verify Minecraft client: {error}"))?;
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

fn parse_java_major(output: &str) -> Option<u32> {
    let version = output.split('"').nth(1).or_else(|| {
        output
            .lines()
            .find(|line| line.to_ascii_lowercase().contains("version"))
    })?;
    let version = version.trim();
    let mut parts = version.split('.');
    let first = parts.next()?.parse::<u32>().ok()?;
    if first == 1 {
        parts.next()?.parse::<u32>().ok()
    } else {
        Some(first)
    }
}

fn java_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        let binary = if cfg!(windows) { "java.exe" } else { "java" };
        candidates.push(PathBuf::from(home).join("bin").join(binary));
    }
    candidates.push(PathBuf::from(if cfg!(windows) {
        "java.exe"
    } else {
        "java"
    }));
    candidates
}

fn discover_java(required_major: u32) -> Result<(PathBuf, u32), String> {
    let mut compatible = Vec::new();
    let mut diagnostic = Vec::new();

    for candidate in java_candidates() {
        let output = match std::process::Command::new(&candidate)
            .arg("-version")
            .output()
        {
            Ok(output) if output.status.success() => output,
            Ok(_) => {
                diagnostic.push(format!("{} returned an error", candidate.display()));
                continue;
            }
            Err(error) => {
                diagnostic.push(format!("{}: {error}", candidate.display()));
                continue;
            }
        };
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        let Some(major) = parse_java_major(&text) else {
            diagnostic.push(format!(
                "{} returned an unrecognized version",
                candidate.display()
            ));
            continue;
        };
        if major < required_major {
            diagnostic.push(format!(
                "{} is Java {major}; Minecraft requires Java {required_major} or newer",
                candidate.display()
            ));
            continue;
        }
        compatible.push((candidate, major));
    }

    compatible.sort_by_key(|(_, major)| (*major != required_major, *major));
    compatible.into_iter().next().ok_or_else(|| {
        if diagnostic.is_empty() {
            format!("No Java runtime was found. Install Java {required_major} or newer and set JAVA_HOME.")
        } else {
            format!(
                "No compatible Java runtime was found (required: {required_major}+). {}",
                diagnostic.join("; ")
            )
        }
    })
}

fn os_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "osx",
        "linux" => "linux",
        other => other,
    }
}

fn os_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "x86" => "x86",
        "aarch64" => "aarch64",
        other => other,
    }
}

fn feature_flags() -> HashMap<&'static str, bool> {
    HashMap::from([
        ("has_custom_resolution", false),
        ("is_demo_user", false),
        ("is_quick_play_singleplayer", false),
        ("is_quick_play_multiplayer", false),
        ("is_quick_play_realms", false),
    ])
}

fn rule_matches(rule: &Value) -> bool {
    let Some(object) = rule.as_object() else {
        return false;
    };

    if let Some(os) = object.get("os").and_then(Value::as_object) {
        if let Some(name) = os.get("name").and_then(Value::as_str) {
            if name != os_name() {
                return false;
            }
        }
        if let Some(arch) = os.get("arch").and_then(Value::as_str) {
            if arch != os_arch() {
                return false;
            }
        }
        // os.version rules are uncommon in current metadata and are not used
        // to relax path or file integrity checks.
    }

    if let Some(features) = object.get("features").and_then(Value::as_object) {
        let flags = feature_flags();
        for (name, expected) in features {
            let actual = flags.get(name.as_str()).copied().unwrap_or(false);
            if expected.as_bool().unwrap_or(false) != actual {
                return false;
            }
        }
    }
    true
}

fn rules_allow(rules: Option<&Value>) -> bool {
    let Some(rules) = rules.and_then(Value::as_array) else {
        return true;
    };
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule) {
            allowed = rule
                .get("action")
                .and_then(Value::as_str)
                .is_some_and(|action| action == "allow");
        }
    }
    allowed
}

fn offline_uuid(username: &str) -> String {
    let mut bytes = md5::compute(format!("OfflinePlayer:{username}")).0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let mut output = String::with_capacity(32);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn placeholders(
    instance_id: &str,
    username: &str,
    uuid: &str,
    memory_mb: u32,
    instance_root: &Path,
    libraries_root: &Path,
    assets_root: &Path,
    natives_root: &Path,
    classpath: &str,
    asset_index: &str,
    version_type: &str,
) -> HashMap<String, String> {
    let path = |value: &Path| value.to_string_lossy().into_owned();
    HashMap::from([
        ("auth_player_name".to_owned(), username.to_owned()),
        ("version_name".to_owned(), instance_id.to_owned()),
        ("game_directory".to_owned(), path(instance_root)),
        ("assets_root".to_owned(), path(assets_root)),
        ("assets_index_name".to_owned(), asset_index.to_owned()),
        ("auth_uuid".to_owned(), uuid.to_owned()),
        ("auth_access_token".to_owned(), "0".to_owned()),
        ("clientid".to_owned(), "0".to_owned()),
        ("auth_xuid".to_owned(), "0".to_owned()),
        ("user_type".to_owned(), "legacy".to_owned()),
        ("version_type".to_owned(), version_type.to_owned()),
        ("user_properties".to_owned(), "{}".to_owned()),
        ("natives_directory".to_owned(), path(natives_root)),
        ("launcher_name".to_owned(), "NOVA".to_owned()),
        (
            "launcher_version".to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
        ),
        ("classpath".to_owned(), classpath.to_owned()),
        (
            "classpath_separator".to_owned(),
            if cfg!(windows) {
                ";".to_owned()
            } else {
                ":".to_owned()
            },
        ),
        ("library_directory".to_owned(), path(libraries_root)),
        ("resolution_width".to_owned(), "1280".to_owned()),
        ("resolution_height".to_owned(), "720".to_owned()),
        ("memory_mb".to_owned(), memory_mb.to_string()),
    ])
}

fn expand_argument(raw: &str, values: &HashMap<String, String>) -> Result<String, String> {
    let mut expanded = raw.to_owned();
    for (key, value) in values {
        expanded = expanded.replace(&format!("${{{key}}}"), value);
    }
    if expanded.contains("${") {
        return Err(format!(
            "Unsupported unresolved launch variable: {expanded}"
        ));
    }
    Ok(expanded)
}

fn append_argument_value(
    value: &Value,
    output: &mut Vec<String>,
    values: &HashMap<String, String>,
) -> Result<(), String> {
    match value {
        Value::String(raw) => output.push(expand_argument(raw, values)?),
        Value::Array(items) => {
            for item in items {
                append_argument_value(item, output, values)?;
            }
        }
        _ => return Err("Minecraft launch metadata contains an invalid argument value".to_owned()),
    }
    Ok(())
}

fn resolve_argument_list(
    raw: Option<&Value>,
    values: &HashMap<String, String>,
) -> Result<Vec<String>, String> {
    let Some(items) = raw.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut output = Vec::new();
    for item in items {
        if let Some(object) = item.as_object() {
            if !rules_allow(object.get("rules")) {
                continue;
            }
            let value = object
                .get("value")
                .ok_or_else(|| "Launch argument entry is missing its value".to_owned())?;
            append_argument_value(value, &mut output, values)?;
        } else {
            append_argument_value(item, &mut output, values)?;
        }
    }
    Ok(output)
}

fn build_launch_arguments(
    metadata: &Value,
    instance_id: &str,
    username: &str,
    memory_mb: u32,
    install: &InstallMarker,
    instance_root: &Path,
    data_root: &Path,
    game_version: &str,
) -> Result<Vec<String>, String> {
    let main_class = metadata
        .get("mainClass")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Minecraft version metadata has no mainClass".to_owned())?;

    let version_type = metadata
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("release");
    let libraries_root = data_root.join("shared").join("libraries");
    let assets_root = data_root.join("shared").join("assets");
    let natives_root = instance_root.join("natives");
    let version_root = instance_root.join("versions").join(game_version);
    let client_jar = version_root.join(format!("{game_version}.jar"));

    let mut classpath_entries = Vec::new();
    classpath_entries.push(client_jar);
    let libraries = metadata
        .get("libraries")
        .and_then(Value::as_array)
        .ok_or_else(|| "Minecraft version metadata has no libraries array".to_owned())?;

    for library in libraries {
        if !rules_allow(library.get("rules")) {
            continue;
        }
        let Some(path) = library
            .get("downloads")
            .and_then(|downloads| downloads.get("artifact"))
            .and_then(|artifact| artifact.get("path"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let relative = safe_relative_path(path)?;
        let artifact = libraries_root.join(relative);
        let metadata = std::fs::metadata(&artifact)
            .map_err(|_| format!("A required library is missing: {}", artifact.display()))?;
        if !metadata.is_file() {
            return Err(format!(
                "A required library is not a file: {}",
                artifact.display()
            ));
        }
        let canonical_library_root = std::fs::canonicalize(&libraries_root)
            .map_err(|error| format!("Unable to resolve the library directory: {error}"))?;
        let canonical_artifact = std::fs::canonicalize(&artifact).map_err(|error| {
            format!("Unable to resolve library {}: {error}", artifact.display())
        })?;
        if !canonical_artifact.starts_with(&canonical_library_root) {
            return Err("A library path resolves outside NOVA's shared library store".to_owned());
        }
        classpath_entries.push(canonical_artifact);
    }

    let separator = if cfg!(windows) { ";" } else { ":" };
    let classpath = classpath_entries
        .iter()
        .map(|path| path.to_string_lossy())
        .collect::<Vec<_>>()
        .join(separator);
    let username_uuid = offline_uuid(username);
    let values = placeholders(
        instance_id,
        username,
        &username_uuid,
        memory_mb,
        instance_root,
        &libraries_root,
        &assets_root,
        &natives_root,
        &classpath,
        &install.asset_index,
        version_type,
    );

    let arguments = metadata.get("arguments");
    let mut jvm = resolve_argument_list(arguments.and_then(|value| value.get("jvm")), &values)?;
    if !jvm.iter().any(|argument| argument.starts_with("-Xmx")) {
        jvm.push(format!("-Xmx{memory_mb}M"));
    }
    if !jvm.iter().any(|argument| argument.starts_with("-Xms")) {
        jvm.push("-Xms512M".to_owned());
    }
    if !jvm
        .iter()
        .any(|argument| argument.starts_with("-Djava.library.path="))
    {
        jvm.push(format!(
            "-Djava.library.path={}",
            natives_root.to_string_lossy()
        ));
    }
    if !jvm
        .iter()
        .any(|argument| argument == "-cp" || argument == "-classpath")
    {
        jvm.push("-cp".to_owned());
        jvm.push(classpath);
    }

    let mut game = if arguments
        .and_then(|value| value.get("game"))
        .and_then(Value::as_array)
        .is_some()
    {
        resolve_argument_list(arguments.and_then(|value| value.get("game")), &values)?
    } else if let Some(legacy) = metadata.get("minecraftArguments").and_then(Value::as_str) {
        legacy
            .split_whitespace()
            .map(|argument| expand_argument(argument, &values))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };

    if !game.iter().any(|argument| argument == "--username") {
        game.extend(["--username".to_owned(), username.to_owned()]);
    }
    if !game.iter().any(|argument| argument == "--version") {
        game.extend(["--version".to_owned(), instance_id.to_owned()]);
    }
    if !game.iter().any(|argument| argument == "--gameDir") {
        game.extend([
            "--gameDir".to_owned(),
            instance_root.to_string_lossy().into_owned(),
        ]);
    }
    if !game.iter().any(|argument| argument == "--assetsDir") {
        game.extend([
            "--assetsDir".to_owned(),
            assets_root.to_string_lossy().into_owned(),
        ]);
    }
    if !game.iter().any(|argument| argument == "--assetIndex") {
        game.extend(["--assetIndex".to_owned(), install.asset_index.clone()]);
    }
    if !game.iter().any(|argument| argument == "--uuid") {
        game.extend(["--uuid".to_owned(), username_uuid.clone()]);
    }
    if !game.iter().any(|argument| argument == "--accessToken") {
        game.extend(["--accessToken".to_owned(), "0".to_owned()]);
    }
    if !game.iter().any(|argument| argument == "--userType") {
        game.extend(["--userType".to_owned(), "legacy".to_owned()]);
    }
    if !game.iter().any(|argument| argument == "--versionType") {
        game.extend(["--versionType".to_owned(), version_type.to_owned()]);
    }
    if !game.iter().any(|argument| argument == "--userProperties") {
        game.extend(["--userProperties".to_owned(), "{}".to_owned()]);
    }

    jvm.push(main_class.to_owned());
    jvm.extend(game);
    Ok(jvm)
}

async fn forward_logs<R>(reader: R, app: AppHandle, instance_id: String, stream_name: &'static str)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let bounded = if line.len() > 16 * 1024 {
            let prefix = line.chars().take(16 * 1024).collect::<String>();
            format!("{prefix}… [line truncated]")
        } else {
            line
        };
        let sanitized = crate::diagnostics::record_process_log(&instance_id, stream_name, &bounded);
        let _ = app.emit(
            "nova-process-log",
            ProcessLogEvent {
                instance_id: instance_id.clone(),
                stream: stream_name.to_owned(),
                line: sanitized,
            },
        );
    }
}

pub(crate) async fn launch_instance(
    app: AppHandle,
    data_root: PathBuf,
    cache_root: PathBuf,
    instance_id: String,
    player_name: String,
    memory_mb: u32,
) -> Result<LaunchSession, String> {
    if !safe_id(&instance_id) {
        return Err("Invalid instance identifier".to_owned());
    }
    if !(3..=16).contains(&player_name.len())
        || !player_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(
            "Offline player names must contain 3–16 letters, digits or underscores".to_owned(),
        );
    }
    if !(1024..=32768).contains(&memory_mb) {
        return Err("Memory allocation must be between 1024 MB and 32768 MB".to_owned());
    }
    {
        let active = sessions()
            .lock()
            .map_err(|_| "Launch session registry is unavailable".to_owned())?;
        if !active.is_empty() {
            return Err("A Minecraft process is already running. Stop it before launching another instance.".to_owned());
        }
    }

    let instance_root = data_root.join("instances").join(&instance_id);
    let marker_path = instance_root.join(".nova-install.json");
    let marker_bytes = tokio::fs::read(&marker_path).await.map_err(|_| {
        "This instance is not installed or its verification marker is missing".to_owned()
    })?;
    let marker: InstallMarker = serde_json::from_slice(&marker_bytes)
        .map_err(|_| "The instance verification marker is invalid".to_owned())?;
    if marker.id != instance_id || !safe_id(&marker.asset_index) || marker.client_sha1.len() != 40 {
        return Err("The instance verification marker does not match this installation".to_owned());
    }

    let game_version = marker.game_version.as_deref().unwrap_or(&instance_id);
    if !safe_id(game_version) {
        return Err("The Minecraft base version for this instance is invalid".to_owned());
    }
    let version_root = instance_root.join("versions").join(game_version);
    let client_jar = version_root.join(format!("{game_version}.jar"));
    let expected_hash = marker.client_sha1.to_ascii_lowercase();
    let hash_path = client_jar.clone();
    let actual_hash = tokio::task::spawn_blocking(move || sha1_file(&hash_path))
        .await
        .map_err(|error| format!("Client verification worker failed: {error}"))??;
    if actual_hash != expected_hash {
        return Err("The Minecraft client JAR failed SHA-1 verification. Reinstall this instance before launching.".to_owned());
    }

    let metadata_path = version_root.join(format!("{game_version}.json"));
    let metadata_bytes = tokio::fs::read(&metadata_path)
        .await
        .or_else(|_| {
            std::fs::read(
                cache_root
                    .join("mojang")
                    .join("versions")
                    .join(format!("{game_version}.json")),
            )
        })
        .map_err(|_| "Minecraft version metadata is missing. Repair this instance before launching.".to_owned())?;
    let metadata: Value = serde_json::from_slice(&metadata_bytes)
        .map_err(|error| format!("Official version metadata is invalid: {error}"))?;
    if metadata.get("id").and_then(Value::as_str) != Some(game_version) {
        return Err("Minecraft version metadata does not match this instance's base game version".to_owned());
    }
    let required_java = metadata
        .get("javaVersion")
        .and_then(|value| value.get("majorVersion"))
        .and_then(Value::as_u64)
        .map(|major| major as u32)
        .unwrap_or(8);
    let (java_path, java_major) = discover_java(required_java)?;
    let arguments = build_launch_arguments(
        &metadata,
        &instance_id,
        &player_name,
        memory_mb,
        &marker,
        &instance_root,
        &data_root,
        game_version,
    )?;

    if !tokio::fs::try_exists(&instance_root.join("natives"))
        .await
        .map_err(|error| error.to_string())?
    {
        return Err("Extracted native libraries are missing. Reinstall this instance.".to_owned());
    }

    let mut command = Command::new(&java_path);
    command
        .args(arguments)
        .current_dir(&instance_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child: Child = command
        .spawn()
        .map_err(|error| format!("Unable to start Java {}: {error}", java_path.display()))?;
    let pid = child
        .id()
        .ok_or_else(|| "The operating system did not return a process identifier".to_owned())?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (stop_sender, mut stop_receiver) = watch::channel(false);
    let snapshot = LaunchSession {
        instance_id: instance_id.clone(),
        pid,
        started_at_unix: unix_now(),
        state: "running".to_owned(),
    };

    let inserted = {
        let mut active = sessions()
            .lock()
            .map_err(|_| "Launch session registry is unavailable".to_owned())?;
        if active.is_empty() {
            active.insert(
                instance_id.clone(),
                ProcessSession {
                    snapshot: snapshot.clone(),
                    stop: stop_sender,
                },
            );
            true
        } else {
            false
        }
    };
    if !inserted {
        let _ = child.kill().await;
        return Err("Another Minecraft process started concurrently".to_owned());
    }

    let _ = app.emit(
        "nova-process-state",
        ProcessStateEvent {
            instance_id: instance_id.clone(),
            state: "running".to_owned(),
            pid: Some(pid),
            exit_code: None,
            message: Some(format!("Minecraft started with Java {java_major}")),
        },
    );

    if let Some(stdout) = stdout {
        tokio::spawn(forward_logs(
            stdout,
            app.clone(),
            instance_id.clone(),
            "stdout",
        ));
    }
    if let Some(stderr) = stderr {
        tokio::spawn(forward_logs(
            stderr,
            app.clone(),
            instance_id.clone(),
            "stderr",
        ));
    }

    let monitor_app = app.clone();
    let monitor_id = instance_id.clone();
    let diagnostic_data_root = data_root.clone();
    let diagnostic_cache_root = cache_root.clone();
    tokio::spawn(async move {
        let status = tokio::select! {
            result = child.wait() => result,
            changed = stop_receiver.changed() => {
                if changed.is_ok() && *stop_receiver.borrow() {
                    match child.kill().await {
                        Ok(()) => child.wait().await,
                        Err(error) => Err(error),
                    }
                } else {
                    child.wait().await
                }
            }
        };
        let (state, exit_code, message) = match status {
            Ok(status) if status.success() => (
                "exited",
                status.code(),
                Some("Minecraft closed normally".to_owned()),
            ),
            Ok(status) => (
                "exited",
                status.code(),
                Some("Minecraft exited with an error code".to_owned()),
            ),
            Err(error) => (
                "failed",
                None,
                Some(format!("Unable to monitor Minecraft process: {error}")),
            ),
        };
        let abnormal_exit = state == "failed" || exit_code.is_some_and(|code| code != 0);
        if abnormal_exit {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let _ = crate::diagnostics::run_report(
                monitor_app.clone(),
                diagnostic_data_root,
                diagnostic_cache_root,
                Some(monitor_id.clone()),
                false,
                "minecraft_process_abnormal_exit".to_owned(),
            )
            .await;
        }
        if let Ok(mut active) = sessions().lock() {
            if active
                .get(&monitor_id)
                .is_some_and(|session| session.snapshot.pid == pid)
            {
                active.remove(&monitor_id);
            }
        }
        let _ = monitor_app.emit(
            "nova-process-state",
            ProcessStateEvent {
                instance_id: monitor_id,
                state: state.to_owned(),
                pid: Some(pid),
                exit_code,
                message,
            },
        );
    });

    Ok(snapshot)
}

pub(crate) fn list_sessions() -> Result<Vec<LaunchSession>, String> {
    let active = sessions()
        .lock()
        .map_err(|_| "Launch session registry is unavailable".to_owned())?;
    Ok(active
        .values()
        .map(|session| session.snapshot.clone())
        .collect())
}

pub(crate) fn stop_instance(instance_id: &str) -> Result<(), String> {
    let active = sessions()
        .lock()
        .map_err(|_| "Launch session registry is unavailable".to_owned())?;
    let session = active
        .get(instance_id)
        .ok_or_else(|| "This instance is not running".to_owned())?;
    session
        .stop
        .send(true)
        .map_err(|_| "The Minecraft process could not receive the stop signal".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{expand_argument, offline_uuid, parse_java_major, rules_allow, safe_relative_path};
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn parses_legacy_and_modern_java_versions() {
        assert_eq!(parse_java_major("java version \"1.8.0_402\""), Some(8));
        assert_eq!(
            parse_java_major("openjdk version \"21.0.2\" 2024-01-16"),
            Some(21)
        );
    }

    #[test]
    fn creates_stable_java_compatible_offline_uuid() {
        let first = offline_uuid("NOVAPlayer");
        assert_eq!(first, offline_uuid("NOVAPlayer"));
        assert_eq!(first.len(), 32);
        assert_eq!(&first[12..13], "3");
        assert!(first.chars().all(|value| value.is_ascii_hexdigit()));
    }

    #[test]
    fn applies_metadata_rules_in_order_and_checks_features() {
        let rules = json!([
            {"action":"allow"},
            {"action":"disallow","os":{"name":"windows"}},
            {"action":"allow","os":{"name":super::os_name()}}
        ]);
        assert!(rules_allow(Some(&rules)));
        let feature_rule = json!([{"action":"allow","features":{"is_demo_user":true}}]);
        assert!(!rules_allow(Some(&feature_rule)));
    }

    #[test]
    fn expands_launch_placeholders_and_rejects_unknown_variables() {
        let values = HashMap::from([("auth_player_name".to_owned(), "NOVAPlayer".to_owned())]);
        assert_eq!(
            expand_argument("--username=${auth_player_name}", &values).unwrap(),
            "--username=NOVAPlayer"
        );
        assert!(expand_argument("${unknown}", &values).is_err());
    }

    #[test]
    fn rejects_metadata_path_traversal() {
        assert!(safe_relative_path("org/example/library.jar").is_ok());
        assert!(safe_relative_path("../../outside.jar").is_err());
        assert!(safe_relative_path("C:/Windows/system.dll").is_err());
    }
}
