# Roadmap

The roadmap is ordered by user value and technical dependencies. A milestone is complete only when its tests and manual acceptance criteria pass.

## M0 — Foundation

- [x] Establish the repository and feature branch.
- [x] Create the Tauri 2 + Rust + React + TypeScript workspace.
- [x] Add a navigable desktop UI foundation.
- [x] Add a native Java and environment diagnostic command.
- [x] Add frontend CI and product/architecture documentation.
- [x] Add native Windows compilation to CI and include the required Windows icon resource.
- [x] Add native performance counters and a Performance diagnostics page.
- [x] Add a verified, bounded-concurrency HTTP artifact transport.
- [x] Wire the verified transport to Mojang metadata and the initial Vanilla installation pipeline.
- [ ] Add Windows packaging smoke tests and repeatable performance baselines.

## M1 — Real Vanilla launcher

The first installer milestone is implemented on the feature branch. Native archive extraction, the final launch-argument resolver and process lifecycle remain open before Minecraft can be launched.

- [ ] Detect the Minecraft directory and installed Java runtimes.
- [x] Fetch and cache the official version manifest with stale-cache fallback.
- [x] Resolve official release, snapshot and historical version metadata with manifest SHA-1 verification.
- [x] Install the client, libraries, asset index, objects and platform-native archives with bounded downloads.
- [x] Extract native libraries in an isolated staging directory with path, symlink, collision and decompressed-size safeguards.
- [x] Verify downloaded artifacts and resume safely by reusing already verified files after interrupted installs.
- [ ] Generate launch arguments from version metadata.
- [ ] Launch and stop Minecraft while streaming logs.
- [x] Persist version-specific instance directories and a completion manifest after verified installation.

Acceptance: a clean Windows machine with a supported Java runtime can install and launch a selected official Minecraft version through NOVA.

## M2 — Instances and mod loaders

- [ ] Multiple isolated instances.
- [ ] Per-instance Java, memory, game directory and JVM settings.
- [ ] Fabric installation and version compatibility.
- [ ] Forge, NeoForge and Quilt through dedicated installers/metadata.
- [ ] Import/export with validation and conflict previews.

Acceptance: two instances can use different versions and mod configurations without sharing mutable files accidentally.

## M3 — Content ecosystem

- [ ] Modrinth search and installation.
- [ ] CurseForge integration after API and distribution requirements are met.
- [ ] Dependency, incompatibility and loader checks.
- [ ] Modpack import/export.
- [ ] Resource packs and shader packs.
- [ ] Staged updates, rollback and content provenance.

Acceptance: installing or updating content never silently corrupts an instance.

## M4 — Accounts and player experience

- [ ] Microsoft public-client authentication.
- [ ] Secure token storage and refresh.
- [ ] Account switching and offline profiles where legally and technically appropriate.
- [ ] News and release notes.
- [ ] Themes, localization and keyboard shortcuts.

Acceptance: account sessions survive relaunch securely, and sensitive data is absent from logs.

## M5 — Diagnostics and polish

- [ ] Crash classification by JVM / Minecraft failure signature and targeted recovery suggestions.
- [x] Windows hardware inventory (CPU, logical processors, physical RAM, GPU names and driver versions where exposed); non-Windows fallback inventory.
- [x] Java candidate discovery and compatibility checks against official version metadata.
- [x] Automatic quick diagnostics at desktop startup, on launch preflight failure and after abnormal Minecraft process exit.
- [x] Quick and deep instance integrity scans; deep mode hashes indexed asset objects and applicable libraries.
- [x] Bounded log/crash-artifact capture, sensitive-field and home-path redaction, local JSON reports and a detailed diagnostic UI.
- [ ] GPU sensors / accurate per-adapter VRAM, disk health and live resource telemetry.
- [ ] Download queue management and resumability.
- [ ] Accessibility, translations and high-DPI review.
- [ ] macOS and Linux packaging and smoke tests.

## Release gate

No feature is called complete based only on a mock UI. Each release candidate must pass type-checking, Rust checks, automated tests, package build and a real Minecraft launch smoke test on its target platform.
