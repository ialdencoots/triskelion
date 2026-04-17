# Instance Designer Guide

Instances are self-contained zones — each has its own terrain, mob layout, and boss. They are defined as static Rust constants: type-checked at compile time, no runtime asset loading required.

## The Node Graph Model

An instance is a directed acyclic graph (DAG) of `PathNode`s stored in `InstanceDef::nodes`. Index **0 is always the entry/spawn point**.

```
         ┌──[Left: Golems x3]──┐
Entry(0)─┤                     ├──[Mid pack](3)──[Boss Arena](4)
         └──[Right: Goblins x5]─┘
```

Each node has:
- `position: (f32, f32)` — world-space XZ relative to the instance center
- `pack: Option<PackDef>` — the mob group placed here (or `None` for clear rooms)
- `node_kind: NodeKind` — `Normal` or `BossArena`
- `connects_to: &'static [usize]` — indices of outgoing edges; empty on leaf nodes

**`BossArena` nodes must have `connects_to: &[]`.**

Mob scatter positions are deterministic (terrain seed + node index), so they are reproducible.

Gating is currently soft — packs are positioned to naturally block corridors. The server tracks cleared packs in `pack_entities` for future hard-gate logic.

---

## Adding a New Instance

1. **Add enum variants** in `shared/src/instances/mod.rs`:
   ```rust
   pub enum InstanceKind { Overworld, CrystalCaverns, YourNewZone }
   pub enum MobKind { ..., YourNewMob }  // only if adding new mob types
   ```

2. **Create the instance file** `shared/src/instances/your_new_zone.rs`:
   ```rust
   use super::*;
   pub const YOUR_NEW_ZONE: InstanceDef = InstanceDef {
       kind: InstanceKind::YourNewZone,
       terrain: TerrainConfig { seed: 999, frequency: 0.05, height_scale: 4.0,
                                octaves: 5, lacunarity: 2.0, gain: 0.5,
                                size: 128, tile_scale: 1.0 },
       nodes: &[
           PathNode { position: (0.0, 0.0), pack: None,
                      node_kind: NodeKind::Normal, connects_to: &[1] },
           // ...more nodes...
       ],
       max_players: 4,
   };
   ```

3. **Register in `mod.rs`**:
   ```rust
   pub mod your_new_zone;
   pub use your_new_zone::YOUR_NEW_ZONE;
   pub const INSTANCE_DEFS: &[InstanceDef] = &[OVERWORLD, CRYSTAL_CAVERNS, YOUR_NEW_ZONE];
   ```

4. **Add a `spawn_mob` match arm** in `server/src/systems/mob_defs.rs` for any new `MobKind` variants.

5. *(Optional)* **Add client visuals** in `client/src/world/enemies.rs` — match on `MobKindTag` to set a distinct mesh color/size.

---

## Annotated Example: Crystal Caverns

```rust
pub const CRYSTAL_CAVERNS: InstanceDef = InstanceDef {
    kind: InstanceKind::CrystalCaverns,

    // Tighter, lower terrain than the overworld — feels like underground.
    terrain: TerrainConfig {
        seed: 42,        // different seed → different noise pattern
        frequency: 0.06, // higher = more jagged
        height_scale: 3.0, // lower = flatter cave floor
        octaves: 6, lacunarity: 1.8, gain: 0.6,
        size: 128, tile_scale: 1.0,
    },

    nodes: &[
        PathNode {
            position: (0.0, 0.0),  // players spawn here
            pack: None,            // no mobs at spawn
            node_kind: NodeKind::Normal,
            connects_to: &[1, 2],  // two routes out
        },
        PathNode {
            position: (20.0, 12.0),  // left branch
            pack: Some(PackDef {
                mobs: &[MobSpawn { kind: MobKind::CrystalGolem, count: 3 }],
                spread: 6.0,  // golems scattered within 6m of this position
            }),
            node_kind: NodeKind::Normal,
            connects_to: &[3],  // both branches converge at node 3
        },
        // ... nodes 2, 3, 4 (boss arena)
    ],
    max_players: 4,
};
```
