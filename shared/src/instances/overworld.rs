use super::*;

pub const OVERWORLD: InstanceDef = InstanceDef {
    kind: InstanceKind::Overworld,
    terrain: TerrainConfig {
        seed: 0,
        frequency: 0.04,
        height_scale: 6.0,
        octaves: 4,
        lacunarity: 2.0,
        gain: 0.5,
        size: 128,
        tile_scale: 1.0,
    },
    //  (0) Entry — no pack. Player spawn slots fan out at radius 3 from here,
    //  so packs need pack_distance > aggro_range + spawn_radius + pack_spread
    //  to avoid auto-aggro at spawn. Patrol behavior is unbounded Lissajous
    //  (no leash to spawn), so mobs drift over time — positions below sit
    //  ~25 units past the safe threshold to give plenty of test-setup buffer
    //  before patrol drift could bring them within aggro range.
    //   ├─(1) Goblin camp
    //   ├─(2) Orc patrol
    //   └─(3) Troll den
    nodes: &[
        PathNode {
            position: (0.0, 0.0),
            pack: None,
            node_kind: NodeKind::Normal,
            connects_to: &[1, 2, 3],
        },
        PathNode {
            position: (30.0, -24.0),
            pack: Some(PackDef {
                mobs: &[MobSpawn { kind: MobKind::Goblin, count: 1 }],
                spread: 3.0,
            }),
            node_kind: NodeKind::Normal,
            connects_to: &[],
        },
        PathNode {
            position: (-32.0, 24.0),
            pack: Some(PackDef {
                mobs: &[MobSpawn { kind: MobKind::Orc, count: 1 }],
                spread: 3.0,
            }),
            node_kind: NodeKind::Normal,
            connects_to: &[],
        },
        PathNode {
            position: (38.0, 10.0),
            pack: Some(PackDef {
                mobs: &[MobSpawn { kind: MobKind::Troll, count: 1 }],
                spread: 3.0,
            }),
            node_kind: NodeKind::Normal,
            connects_to: &[],
        },
    ],
    max_players: 100,
    use_layout_terrain: false,
};
