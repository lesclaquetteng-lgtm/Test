# Architecture

## High-level design

NOVA uses Tauri 2 as its native desktop shell, Rust for filesystem/process/network operations and React + TypeScript for presentation.

The frontend is deliberately not the authority for installation or account state. It requests typed operations from Rust and renders the returned state.

## Layers

### Frontend — React / TypeScript

Responsibilities:

- Navigation, forms, views and accessible interaction.
- Rendering operation state and progress events.
- Collecting user intent and validating basic form input.
- Calling a small, typed desktop API.

Must not:

- Store Microsoft refresh tokens.
- Construct Minecraft launch commands.
- Write directly into managed game directories.
- Treat a browser preview as proof that desktop functionality works.

### Desktop boundary — Tauri

Responsibilities:

- Expose explicit commands and events.
- Apply capability permissions.
- Keep the renderer isolated from arbitrary native operations.
- Validate every command argument on the Rust side.

### Native services — Rust

Planned modules:

- runtime: Java discovery, compatibility and installation.
- minecraft: official version manifest, metadata, libraries, assets, natives and launch arguments.
- instances: instance manifests, paths, import/export and isolation.
- downloads: a reusable Rust transport with global/per-host concurrency bounds, SHA-1 streaming verification, a bounded progress channel, selective retries and rollback-safe publication. Cancellation and HTTP range resume are still pending.
- accounts: Microsoft / Xbox / Minecraft authentication and secure token storage.
- loaders: Fabric first, then Forge, NeoForge and Quilt.
- content: Modrinth and CurseForge provider adapters, dependency and compatibility checks.
- diagnostics: process lifecycle, log streaming and crash reports.

## Data and filesystem boundaries

- Use one application data directory for launcher metadata and caches.
- Use one dedicated directory per managed instance.
- Never infer an instance path from an untrusted display name.
- Canonicalize and validate paths before destructive operations.
- Reject archive entries that escape the intended extraction root.
- Stage downloads in temporary files and verify before moving them into place.
- Prefer atomic rename/swap operations and preserve backups for destructive updates.

## Minecraft installation and launch

The implementation should consume official Minecraft version metadata rather than hard-code library lists or launch arguments.

Planned pipeline:

1. Fetch and validate the official version manifest.
2. Resolve a selected version's metadata.
3. Select a compatible Java runtime.
4. Download client, libraries, assets and platform natives.
5. Verify available size/hash metadata.
6. Build the argument list from the version metadata.
7. Expand supported placeholders safely.
8. Start the Java process with the instance working directory.
9. Stream stdout/stderr and track process exit.
10. Redact secrets from all logs and diagnostic output.

Modded profiles must be installed through loader-specific metadata and APIs, not by guessing filenames.

## Authentication and secrets

Microsoft authentication must use an appropriate supported public-client flow and secure state/PKCE handling where required by the provider. Client secrets must never be embedded in a desktop binary.

Tokens belong in the operating system credential store through a reviewed Rust integration. Until that is available, authentication must not pretend to be connected.

## Provider integrations

Modrinth and CurseForge must be isolated behind provider traits. Each adapter is responsible for:

- Provider-specific API schema and rate limits.
- Project, version and file metadata.
- Loader/game-version compatibility.
- Dependency and optional dependency semantics.
- Attribution, license and source URL display.

API keys and service credentials must follow the provider's terms and must not be committed to this repository.

## Testing strategy

- Unit tests for path validation, version resolution and argument construction.
- Fixture tests for official metadata and provider API responses.
- Integration tests for download resume, hash mismatch and atomic replacement.
- Process tests using a harmless fixture executable.
- UI tests for navigation, loading, empty, error and success states.
- Windows smoke tests before distributing a release.

The current native foundation implements the desktop shell, asynchronous Java diagnostics, performance counters and a standalone verified-download transport. The downloader is not yet wired to Mojang's version manifest, instance installation or the UI. Authentication, loaders, content providers and the full launch pipeline remain roadmap work.
