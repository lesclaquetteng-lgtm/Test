# NOVA Client — Fabric 1.21.11

NOVA Client is an independent client-side Fabric mod. It does not bundle or copy OneClient, OneConfig or their assets.

## Current feature set

- **Right Shift (default):** open the NOVA Client control panel.
- **Right Ctrl (default):** open the draggable HUD editor.
- **F8 (default):** toggle NOVA overlays.
- **Searchable HUD controls:** filter all NOVA HUD options by name or description without leaving the in-game panel.
- **Configurable shortcuts:** rebind the menu, HUD editor and global HUD toggle in-game; Backspace clears a binding and Escape cancels capture.
- **Statistics:** FPS, reciprocal-FPS frame-time estimate, server ping, left-click CPS, coordinates and optional Java heap usage.
- **Keystrokes:** live WASD and jump state.
- **Armor Status:** durability percentages for helmet, chestplate, leggings and boots.
- **Facing:** cardinal direction.
- **Themes:** Aurora, Amethyst and Ember.
- **Profiles:** Default, Competitive and Minimal; each stores module toggles, theme, widget positions and NOVA shortcuts.
- **Layout editor:** drag the four HUD panels independently; reset and save on release.
- **Persistence:** atomic local JSON settings under `config/nova-client/`.

## Requirements and compatibility

- Minecraft Java Edition **1.21.11**
- Fabric Loader **0.20.1 or newer**
- Fabric API **0.141.6+1.21.11**
- Java **21** to run the game
- JDK **25** to run the current Fabric Loom 1.19.x Gradle build
- The JAR targets Java 21 bytecode (`options.release = 21`).

This first release target is deliberately narrow. It is not a Forge, NeoForge, Ornithe or Minecraft 1.8.9 build.

## Build

Windows:

```powershell
.\gradlew.bat clean build
```

Linux/macOS:

```bash
./gradlew --no-daemon clean build
```

The remapped JAR is written to `build/libs/`. Automated tests run as part of `build`.

## Performance contract

- HUD rendering reads a precomputed immutable telemetry snapshot and draws only enabled modules.
- FPS, ping, coordinates, armor and facing are sampled every five client ticks (about four samples per second at 20 TPS).
- Heap usage is sampled every twenty ticks.
- The displayed frame-time value is an estimate derived from `1000 / FPS`; it is not a GPU timer or a true frame-pacing measurement.
- The HUD render callback performs no HTTP requests, filesystem writes, chunk scans or entity iteration.
- Configuration is persisted only when a user changes settings or releases a dragged widget.

## Scope

NOVA Client is a visual and configuration client layer. It does not automate combat, movement, inventory actions or server interactions, and it does not bypass server rules or anti-cheat.
