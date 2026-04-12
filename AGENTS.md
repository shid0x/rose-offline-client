# AGENTS.md

This file applies to the `rose-offline-client` repository.

Use it for Bevy client, UI, rendering, asset loading, client networking flow, and client-side scripting work.

## Repo Role

`rose-offline-client` is the standalone Rust + Bevy game client.

It is responsible for:
- app startup and config
- loading ROSE assets through a virtual filesystem
- rendering, materials, particles, terrain, water, and world-space UI
- client networking and packet handling
- client-side gameplay presentation and interaction
- UI, dialogs, debug windows, and input
- audio, animation, and Lua-based scripting helpers

This repo is the source of truth for player-facing behavior on the client.

## Important Repo Facts

- This repo is its own git repository, not part of the `rose-offline` workspace.
- It depends on shared crates from the server-side codebase through path dependencies.
- Before changing dependency paths or repo structure, inspect `Cargo.toml` carefully.
- Client bugs can originate here, in `GameFiles`, or in shared crates from `rose-offline`.

## Entry Points And Modes

- `src/main.rs`
  CLI entry point.
  Supports:
  - normal game mode
  - `--model-viewer`
  - `--zone-viewer`
  - `--zone <id>`
  - config overrides for data paths, server IP/port, login, sound, graphics, and versions

- `src/lib.rs`
  Main application setup.
  Important responsibilities:
  - config parsing
  - virtual filesystem creation
  - Bevy app/plugin setup
  - state wiring
  - asset loader registration
  - mode dispatch through `run_game`, `run_model_viewer`, and `run_zone_viewer`

- `src/resources/app_state.rs`
  Main client states:
  - `GameLogin`
  - `GameCharacterSelect`
  - `Game`
  - `ModelViewer`
  - `ZoneViewer`

## Module Map

- `src/render/`
  Custom materials, pipelines, world UI rendering, terrain, sky, water, particles, damage digits, and zone lighting.
  Start here for visual rendering behavior or shader/material integration.

- `src/ui/`
  Egui-based game UI, dialogs, windows, widgets, drag-and-drop, settings, debug panels, and HUD behavior.
  Start here for player-facing menus and interface logic.

- `src/systems/`
  Main gameplay-facing Bevy systems.
  This is where most runtime behavior changes land:
  - login and connection flow
  - command handling
  - camera and input
  - entity presentation
  - projectile/effect/status updates
  - zone/model viewer flow

- `src/resources/`
  Shared runtime state and caches:
  - connections
  - game data
  - selected target
  - UI resources
  - sound settings/cache
  - world/zone time
  - debug render config

- `src/protocol/`
  Client-side protocol behavior.
  `src/protocol/irose/` contains iROSE-specific login/world/game clients.
  Start here for packet flow, connection behavior, or client/server protocol mapping.

- `src/audio/`
  Global, spatial, streaming, and format-specific audio handling.

- `src/animation/`
  Skeletal, mesh, transform, and camera animation systems/loaders.

- `src/scripting/`
  Lua4 VM integration and quest/script helpers.
  Start here when the issue is about client-side script behavior or Lua bindings.

- Asset/VFS loaders at repo root:
  - `zone_loader.rs`
  - `model_loader.rs`
  - `effect_loader.rs`
  - `zms_asset_loader.rs`
  - `vfs_asset_io.rs`
  - `exe_resource_loader.rs`

## Where To Put Changes

- Rendering bug, material setup, particle visuals, terrain/water/sky:
  - `src/render/`

- Inventory, hotbar, party UI, dialogs, settings, debug windows, widgets:
  - `src/ui/`

- Camera, input, entity presentation, connection response handling, per-frame game behavior:
  - `src/systems/`

- Shared client state, caches, config-derived runtime data:
  - `src/resources/`

- Packet handling, async client connection flow, iROSE protocol mapping:
  - `src/protocol/`

- Sound playback/mixing/decoding:
  - `src/audio/`

- Animation playback and animation-driven effects:
  - `src/animation/`

- Quest/script helper behavior:
  - `src/scripting/`

- Asset format loading or VFS-backed asset resolution:
  - root loaders such as `zone_loader.rs`, `model_loader.rs`, `effect_loader.rs`, `vfs_asset_io.rs`

## Cross-Repo Guidance

Not every client-visible bug belongs in this repo.

Check `rose-offline` when the issue is really about:
- packet definitions or shared message types
- file readers for raw ROSE formats
- normalized game data loading
- iROSE gameplay formulas or server-authoritative behavior

Check `GameFiles` when the issue depends on:
- real extracted assets
- UI data/content files
- zones, models, effects, sounds, strings, or scripts from the original game data

## Config And Data Notes

- The client auto-loads `config.toml` from the working directory if present.
- It can also accept CLI overrides for data sources, server connection, account info, graphics, and sound.
- Filesystem devices support VFS and extracted-directory inputs, including override layering.
- Device precedence is last-defined-wins after internal reversal, so override directories can intentionally take priority.
- Default local login target is `127.0.0.1:29000`.

## Working Rules

- Do not edit `target/`.
- Prefer fixing behavior in the correct module rather than adding one-off conditionals in `lib.rs`.
- Keep app-state transitions explicit; do not blur login, character select, game, model viewer, and zone viewer behavior.
- Reuse existing resources/events/systems where possible instead of creating parallel state paths.
- Be conservative with render changes; small material/pipeline edits can have wide visual impact.
- When touching both client protocol code and shared crates, confirm whether the real fix belongs in `rose-offline`.

## Validation

Run commands from the `rose-offline-client/` repo root.

Prefer targeted validation:
- `cargo check`
- `cargo test`
- `cargo test zone_loader`
- `cargo test game_connection_system`
- `cargo test ui_inventory_system`
- `cargo test ui_party_system`

Runtime validation is especially important for:
- rendering and materials
- UI layout or interaction
- login/character/game flow
- asset loading and zone/model viewing
- audio

Useful manual checks:
- launch the normal client flow against the local server
- run `--zone-viewer` for map/terrain/collider issues
- run `--model-viewer` for isolated model/material/animation issues

## Common Pitfalls

- Fixing a shared packet/data issue locally in the client instead of in `rose-offline`
- Putting UI state into unrelated gameplay systems
- Mixing rendering concerns with asset-loading concerns
- Assuming a visual bug is a shader issue when the source asset/data is wrong
- Forgetting that viewer modes and normal game mode may exercise different paths
- Changing config/data-source behavior without checking CLI overrides and `config.toml`

## When Unsure

- Start in `src/systems/` for runtime behavior.
- Start in `src/ui/` for interface issues.
- Start in `src/render/` for visual output.
- Start in `src/protocol/irose/` for client/server packet flow.
- Start in `zone_loader.rs` or `model_loader.rs` for asset loading issues.

