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
        size: 192,
        tile_scale: 2.0,
    },
    //       (0) Entry
    //        ├──(1) Left: golems        (56, +40)
    //        └──(2) Right: goblins/orc  (56, -40)
    //             both → (3) convergence (110, 0) → (4) Boss arena (150, 0)
    nodes: &[
        PathNode {
            position: (0.0, 0.0),
            pack: None,
            node_kind: NodeKind::Normal,
            connects_to: &[1, 2],
        },
        PathNode {
            position: (56.0, 40.0),
            pack: Some(PackDef {
                mobs: &[MobSpawn { kind: MobKind::CrystalGolem, count: 3 }],
                spread: 6.0,
            }),
            node_kind: NodeKind::Normal,
            connects_to: &[3],
        },
        PathNode {
            position: (56.0, -40.0),
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
            position: (110.0, 0.0),
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
            position: (150.0, 0.0),
            pack: Some(PackDef {
                mobs: &[MobSpawn { kind: MobKind::CrystalGolemLord, count: 1 }],
                spread: 0.0,
            }),
            node_kind: NodeKind::BossArena,
            connects_to: &[],
        },
    ],
    max_players: 4,
    use_layout_terrain: true,
};
