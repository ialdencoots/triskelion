# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Run the headless server (port 5000)
cargo run --release -p server

# Run the client (connects to 127.0.0.1:5000)
cargo run --release -p client

# Type-check the workspace without building binaries
cargo check

# All tests across the workspace
cargo test -j 2

# Just the server integration tests (system-level harness lives there)
cargo test -j 2 -p server

# A single test file from server/tests/
cargo test -j 2 -p server --test combat_system

# A single test by name
cargo test -j 2 -p server --test combat_system stance_multiplier_tracks_active_stance
```

**Always pass `-j 2` (or lower) when running tests in this repo.** The full Bevy/Lightyear build graph compiles many heavy crates in parallel and will OOM the dev machine without a job cap.

Debug builds compile in DEV-ONLY paths (gated by `#[cfg(debug_assertions)]`) — number keys 4/5/6 send `DevApplyDotMsg` to apply typed DoTs to the selected mob. Release builds strip these entirely.

## Workspace layout

Three crates: `shared` (protocol), `server` (authoritative simulation), `client` (renderer + input + local physics). `shared` defines all networked types; both binaries register them through `shared::SharedPlugin`.

## Architecture

### Server authority and tick model
The server is authoritative for all gameplay. It runs headless (no window/renderer) at **64 Hz** in `FixedUpdate` (`shared::settings::FIXED_TIMESTEP_HZ`). Lightyear UDP transports replicate components and messages between server and clients. Clients run at render rate; they predict only local-player movement via Avian/Tnua physics and dead-reckon remote players/enemies between updates.

Minigame state is **server-authoritative and not predicted**. Clients render exactly what the server replicates. UI flourishes (e.g. ghost-arc history) are reconstructed locally from the replicated state and never fed back. When adding a new minigame component to `shared/src/components/minigame/`, register it in `shared/src/lib.rs` with a plain `register_component` (no prediction config), and tick it in `server/src/systems/minigame.rs` from inside the `FixedUpdate` chain in `server/src/plugin.rs`.

### Damage-flow ordering (FixedUpdate chain)
The chain in `server/src/plugin.rs` is **load-bearing** — re-ordering it changes correctness. Read the comments there before reordering. Roughly:

1. Spawn/instance requests run first so `PlayerEntityLink` exists before input.
2. `process_player_inputs` emits `DamageEvent`s and a server-local `MitigationCommitEvent`.
3. `apply_mitigation_commits` resolves Intercessor pool fills before damage lands so same-tick hits see the new pool.
4. `tick_dots`, `tick_enemy_casts`, `tick_enemy_abilities` queue more damage.
5. `apply_damage_events` resolves all queued damage and emits scaled `DisruptionEvent`s post-mitigation (magnitude × `final_dmg / resolved_dmg`, so absorbed damage produces less disruption). `apply_disruption_events` then drains those into minigame state. New enemy attacks attach disruption to the `DamageEvent` (`disruption: Some(profile)`) rather than writing a separate `DisruptionEvent` — that legacy direct-emission path is gone.
6. `apply_death_transition` inserts the `Dead` marker — must run after damage so HP is final.
7. Minigame ticks run last so they observe the post-damage / post-disruption state.

`sync_replicated_mitigation_pool` and `sync_replicated_threat_list` mirror server-only state into replicated components for the client; they must run after their producers.

### Replication patterns to know
- **`replicate_once: true`** — identity (PlayerId, Class, Subclass, GroupId, Name), enemy markers, the `Dead` marker. Sent once at spawn, never again.
- **Per-tick** — positions/velocities, combat state, minigame state, replicated mirror components.
- **`InstanceId` is *not* replicate-once** — players move between instances and other clients need the updated value to filter visibility.
- **Messages with `add_map_entities()`** carry `Entity` references that Lightyear remaps at the client (e.g. `SelectTargetMsg`, `DamageNumberMsg`, `PlayerSelectedTarget`, `PlayerSpawnedMsg`).
- **`SelectedMobOrPlayer`** uses `Entity` for mobs (server-replicated, stable) but a `u64` `PlayerId` for players (client entity IDs differ per client).

### Position model
- Local player: client-owned Avian capsule + Tnua controller; server reconciles.
- Remote players: dead-reckoned via `client/src/world/DeadReckoning` using replicated `PlayerPosition`/`PlayerVelocity`. The XZ velocity is a `Vec2` where `vel.y` is **world Z**, with vertical motion in a separate `vel_y` field.
- Enemies: server replicates position + XZ velocity; client floor-clamps to terrain (no `vel_y`).
- Float heights (`PLAYER_FLOAT_HEIGHT`, `BOSS_FLOAT_HEIGHT`) and airborne thresholds in `shared/src/settings.rs` are shared by client physics, server spawn placement, and dead-reckoning floor clamping. Keep them in sync.
- **Local player Transform gotcha**: the local player's server-replicated entity (`OwnServerEntity`) carries `PlayerId` but **no** `Transform` — the visible avatar is the physics capsule entity with `PlayerMarker`. Any UI surface that floats world-space cues above a player target (heal numbers, floating text) must special-case `OwnServerEntity` to use the `PlayerMarker` capsule's transform; remote players are fine via the standard `PlayerId` query.

### Class / Subclass / Stance
Three classes (Physical, Arcane, Nature), each with three subclasses (one per role: Tank/Heal/DPS) and a corresponding `Stance` flavor. The **`RoleStance` enum (Tank/Dps/Heal)** drives gameplay output — outside of a stance the player has no output. Stance changes wipe per-stance minigame state via `combat::reset_on_stance_change` (called both from input handling and from instance transitions).

Each class uses a different bundle (`PhysicalPlayerBundle`, `ArcanePlayerBundle`, `NaturePlayerBundle`) carrying its two coupled minigame components. See `shared/src/components/player.rs`.

### Instances
Instances are static `InstanceDef` constants in `shared/src/instances/`, registered in `INSTANCE_DEFS`. Each has a node-graph layout (DAG; node 0 is spawn; `BossArena` nodes are leaves), a `TerrainConfig`, and `max_players`. Layout instances (`use_layout_terrain: true`) carve walkable rooms/corridors via `layout_sdf` and use `terrain_surface_y` for placement — bare `sample_height` will underplace entities into walls.

When adding an instance follow `shared/src/instances/DESIGNER_GUIDE.md`. New `MobKind` variants need a `spawn_mob` arm in `server/src/systems/mob_defs.rs` and optionally client visuals in `client/src/world/enemies.rs`.

## Tests

System-level integration tests live in `server/tests/`. They use a deliberately minimal harness in `server/tests/common/mod.rs` — bare `App` with a manually-managed `Time` resource, **no `MinimalPlugins`** (its `TimePlugin` would auto-advance and fight deterministic stepping). Use `common::advance(&mut app, dt)` instead of `app.update()` so `Time::delta_secs()` is non-zero.

Most combat systems take Lightyear `MessageReader`/`MessageSender` queues and would require standing up the Lightyear plugin stack to drive end-to-end. The harness covers systems whose query shape is plain Bevy components + `Res<Time>`; the gameplay-relevant math in the rest is covered by **pure-function tests inline in `server/src/systems/combat.rs`** (preferred over plumbing Lightyear into a test app).

## Design references

`design_docs/` contains the authoritative specs for game mechanics — consult these before changing minigame math or balance:
- `minigame_spec.md` — the six minigames and their commit/streak/quality model
- `physical_cube_grid.md` — Physical class cube/grid bonus routing
- `subclasses_and_abilities.md` — per-subclass ability lists and stance behaviour
