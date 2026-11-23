# NOVA Launcher

**A modern, modular Minecraft launcher — built from the ground up.**

NOVA is an independent desktop launcher project. Its goal is to combine a polished, fast experience with reliable Minecraft installation, isolated instances, mod-loader support, modpack management, account security and actionable diagnostics.

> Project status: early development. The launcher includes the native desktop shell, installation and launch services, instance management, loader and Modrinth integrations, and diagnostics. The separate NOVA Client module is beginning its own in-game feature set.

## Current stack

- **Desktop shell:** Tauri 2
- **Native layer:** Rust
- **Interface:** React + TypeScript + Vite
- **Initial target:** Windows, with a cross-platform architecture
- **Performance focus:** measurable native telemetry, bounded workloads, verified streaming downloads and low-overhead UI

## Development

Install dependencies with npm install, run the desktop application with npm run tauri dev, and build the frontend with npm run build.

## Project map

    src/                 React interface
    src-tauri/           Rust/Tauri desktop application
    nova-client/         Native Fabric 1.21.11 in-game client mod
    docs/                Product strategy, architecture and roadmap
    .github/workflows/   Continuous integration

## NOVA Client (in-game)

The separate nova-client module is the first native in-game client target. It adds a draggable HUD editor, FPS/ping/coordinates/left-CPS telemetry, keystrokes, themes and local profiles. See nova-client/README.md for requirements and build steps. Its HUD renderer consumes sampled telemetry and performs no world/chunk scans or disk/network work.

## Product principles

1. Reliability before feature count.
2. Real operations, visible progress and useful error messages.
3. Each Minecraft instance owns its configuration and content.
4. Downloads are verified and installed atomically.
5. Credentials and refresh tokens stay out of frontend storage and logs.
6. No copied OneLauncher or OneClient code/assets. NOVA is an independent implementation.

See Product strategy in docs/PRODUCT_STRATEGY.md, Architecture in docs/ARCHITECTURE.md, Performance budgets in docs/PERFORMANCE_BUDGETS.md and Roadmap in docs/ROADMAP.md.

## License

The launcher project has not selected a distribution license yet. The separate NOVA Client source is currently marked All Rights Reserved. Third-party dependencies and assets retain their own licenses and attribution.
