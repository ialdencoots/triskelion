use super::*;

pub const CRYSTAL_CAVERNS: InstanceDef = InstanceDef {
    kind: InstanceKind::CrystalCaverns,
    terrain: TerrainConfig {
        seed: 42,
        frequency: 0.06,
        height_scale: 3.0,
        octaves: 6,
        lacunarity: 1.8,
        gain: 0.6,
        size: 128,
        tile_scale: 1.0,
    },
    //       (0) Entry
    //        ├──(1) Left: golems
    //        └──(2) Right: goblins + orc
    //             both → (3) convergence pack → (4) Boss arena
    nodes: &[
        PathNode {
            position: (0.0, 0.0),
            pack: None,
            node_kind: NodeKind::Normal,
            connects_to: &[1, 2],
        },
        PathNode {
            position: (20.0, 12.0),
            pack: Some(PackDef {
                mobs: &[MobSpawn { kind: MobKind::CrystalGolem, count: 3 }],
                spread: 6.0,
            }),
            node_kind: NodeKind::Normal,
            connects_to: &[3],
        },
        PathNode {
            position: (20.0, -12.0),
            pack: Some(PackDef {
                mobs: &[
                    MobSpawn { kind: MobKind::Goblin, count: 5 },
                    MobSpawn { kind: MobKind::Orc,    count: 1 },
                ],
                spread: 8.0,
            }),
            node_kind: NodeKind::Normal,
            connects_to: &[3],
        },
        PathNode {
            position: (40.0, 0.0),
            pack: Some(PackDef {
                mobs: &[
                    MobSpawn { kind: MobKind::CrystalGolem, count: 2 },
                    MobSpawn { kind: MobKind::Goblin,        count: 3 },
                ],
                spread: 7.0,
            }),
            node_kind: NodeKind::Normal,
            connects_to: &[4],
        },
        PathNode {
            position: (60.0, 0.0),
            pack: None,
            node_kind: NodeKind::BossArena,
            connects_to: &[],
        },
    ],
    max_players: 4,
};
