# Product strategy

## Vision

Build a desktop home for Minecraft: a launcher that feels immediate and coherent, while providing the depth expected by players who manage multiple versions, mod loaders, mods and modpacks.

NOVA should not be a visual reskin of an existing launcher. It should offer a distinct product experience and an independently engineered runtime.

## Competitive baseline

The reference project, Polyfrost OneLauncher / OneClient, is a substantial open-source launcher and client. Its public repository documents a Rust application and dedicated packages for authentication, Minecraft metadata and launch support, Java runtimes, content providers, persistence, networking and application UI.

The product baseline to meet includes:

- Multiple isolated game instances and profiles.
- Vanilla and modded Minecraft versions.
- Mod-loader installation and updates.
- Mod and modpack discovery, installation and dependency handling.
- Java detection and runtime management.
- Microsoft account sessions.
- Resource packs, shaders, screenshots, logs and crash reports.
- Import/export and instance portability.
- Per-instance memory and launch configuration.
- Updates, diagnostics and recovery paths.

NOVA must reach reliable parity in the core launcher journey before claiming to be better overall.

## Differentiation

### 1. A fast, understandable home

- Launch the selected instance without navigating through several screens.
- Make the selected version, loader, account and Java runtime visible.
- Keep download and launch progress understandable.
- Use responsive layouts, keyboard navigation and accessible contrast.

### 2. First-class instance isolation

- Every instance has a predictable root directory.
- Game settings, mods, resource packs, shader packs, saves and logs are isolated by default.
- Shared folders are opt-in and clearly explained.
- Import and export must preserve provenance and detect conflicts.

### 3. A dependable content workflow

- Search providers through explicit adapters.
- Show compatibility before installation.
- Resolve dependencies and conflicts before changing an instance.
- Stage downloads, verify checksums where metadata provides them, then swap files atomically.
- Preserve the previous working state if an update fails.

### 4. Useful performance controls

- Detect installed Java runtimes and explain version compatibility.
- Provide safe memory presets as well as advanced JVM arguments.
- Detect GPU and system information without making unsupported FPS promises.
- Explain which settings are global and which are per instance.

### 5. Transparent diagnostics

- Show actionable errors instead of generic failure dialogs.
- Keep launch logs accessible and redact credentials, tokens and session identifiers.
- Provide a one-click diagnostic bundle with an explicit preview before sharing.

## Non-negotiable quality rules

- Never mark an operation successful before it has completed.
- Never display sample instances or fake download activity as if they were installed.
- Never silently replace user worlds, configuration or mods.
- Never store Microsoft refresh tokens in browser local storage.
- Never launch arbitrary downloaded executables without a clear trust boundary.
- All long-running work must be cancellable where safe and report real progress.
- Network responses and manifests must be validated before use.

## Initial audience and platform

Windows is the first delivery target. The architecture must avoid Windows-only assumptions in the core so Linux and macOS can be added without rewriting the product layer.

The first complete user journey is: detect Java → choose an official Minecraft version → install it into an isolated instance → launch it → inspect logs.
