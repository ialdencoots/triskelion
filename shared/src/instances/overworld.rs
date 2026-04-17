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
    //  (0) Entry — no pack
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
            position: (5.0, -4.0),
            pack: Some(PackDef {
                mobs: &[MobSpawn { kind: MobKind::Goblin, count: 1 }],
                spread: 3.0,
            }),
            node_kind: NodeKind::Normal,
            connects_to: &[],
        },
        PathNode {
            position: (-8.0, 6.0),
            pack: Some(PackDef {
                mobs: &[MobSpawn { kind: MobKind::Orc, count: 1 }],
                spread: 3.0,
            }),
            node_kind: NodeKind::Normal,
            connects_to: &[],
        },
        PathNode {
            position: (12.0, 3.0),
            pack: Some(PackDef {
                mobs: &[MobSpawn { kind: MobKind::Troll, count: 1 }],
                spread: 3.0,
            }),
            node_kind: NodeKind::Normal,
            connects_to: &[],
        },
    ],
    max_players: 100,
};
