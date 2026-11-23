# NOVA Launcher

**A modern, modular Minecraft launcher — built from the ground up.**

NOVA is an independent desktop launcher project. Its goal is to combine a polished, fast experience with reliable Minecraft installation, isolated instances, mod-loader support, modpack management, account security and actionable diagnostics.

> Project status: early foundation. The current branch contains the desktop shell, navigation, a native Java/environment diagnostic command and the initial product/architecture plan. Minecraft installation and game launching are not implemented yet.

## Current stack

- **Desktop shell:** Tauri 2
- **Native layer:** Rust
- **Interface:** React + TypeScript + Vite
- **Initial target:** Windows, with a cross-platform architecture
- **Performance focus:** measurable native telemetry, bounded workloads, verified streaming downloads and low-overhead UI

## Requirements

- Node.js 20.19+ or 22.12+
- npm
- Rust stable and Cargo
- Tauri 2 prerequisites for your operating system

## Development

Install dependencies:

    npm install

Run the browser preview:

    npm run dev

Run the native desktop application:

    npm run tauri dev

Type-check and build the frontend:

    npm run check
    npm run build

Check Rust formatting:

    cd src-tauri
    cargo fmt --all -- --check

Build the desktop application:

    npm run tauri build

## Project map

    src/                 React interface
    src/lib/             Desktop bridge and frontend utilities
    src-tauri/           Rust/Tauri desktop application
    docs/                Product strategy, architecture and roadmap
    .github/workflows/   Continuous integration

## Product principles

1. Reliability before feature count.
2. Real operations, visible progress and useful error messages.
3. Each Minecraft instance owns its configuration and content.
4. Downloads are verified and installed atomically.
5. Credentials and refresh tokens stay out of frontend storage and logs.
6. No copied OneLauncher code or assets. NOVA is an independent implementation.

See Product strategy in docs/PRODUCT_STRATEGY.md, Architecture in docs/ARCHITECTURE.md, Performance budgets in docs/PERFORMANCE_BUDGETS.md and Roadmap in docs/ROADMAP.md.

## License

The project has not selected a distribution license yet. Do not assume OneLauncher's GPL-3.0-only license applies to NOVA. Third-party dependencies and assets must retain their own licenses and attribution.
