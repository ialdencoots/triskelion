// ═════════════════════════════════════════════════════════════════════════════
// DEV-ONLY — REMOVE BEFORE SHIP
// Consumes `DevApplyDotMsg` from clients. When a client presses 4/5/6 the
// server applies a typed DoT to that player's currently selected mob.
// To remove: delete this file, drop the `dev_dots` module from systems/mod.rs,
// and remove the `process_dev_dot_requests` registration in plugin.rs. Then
// remove the matching client + shared bits (grep for DEV-ONLY).
// ═════════════════════════════════════════════════════════════════════════════
use bevy::prelude::*;
use lightyear::prelude::*;

use shared::components::combat::DamageType;
use shared::components::enemy::EnemyMarker;
use shared::components::player::{PlayerSelectedTarget, SelectedMobOrPlayer};
use shared::messages::DevApplyDotMsg;

use super::combat::{DamageOverTime, DamageOverTimes};
use super::connection::PlayerEntityLink;

/// Tuning per damage type — deliberately distinct cadences so each type looks
/// different when testing visually.
fn dot_for(ty: DamageType, source: Entity) -> DamageOverTime {
    let (per_tick, interval, remaining_ticks) = match ty {
        DamageType::Physical => (5.0_f32, 1.0_f32, 5_u32), // 5 ticks × 1.0s = 25 raw / 5s
        DamageType::Arcane   => (8.0,     1.5,     4),     // 4 ticks × 1.5s = 32 raw / 6s
        DamageType::Nature   => (3.0,     0.5,    10),     // 10 ticks × 0.5s = 30 raw / 5s
    };
    DamageOverTime {
        source,
        ty,
        per_tick,
        interval,
        remaining_ticks,
        since_last: 0.0,
    }
}

/// Reads each client's `DevApplyDotMsg` buffer and attaches a DoT to that
/// client's selected mob. No-op if nothing is selected or selection is a player.
pub fn process_dev_dot_requests(
    mut link_query: Query<(&PlayerEntityLink, &mut MessageReceiver<DevApplyDotMsg>)>,
    player_query: Query<&PlayerSelectedTarget>,
    mut enemy_query: Query<Option<&mut DamageOverTimes>, With<EnemyMarker>>,
    mut commands: Commands,
) {
    for (link, mut receiver) in link_query.iter_mut() {
        // Drain buffered dev messages; apply each (rare — keypress cadence).
        let msgs: Vec<DevApplyDotMsg> = receiver.receive().collect();
        if msgs.is_empty() { continue; }

        let Ok(target) = player_query.get(link.0) else { continue };
        let Some(SelectedMobOrPlayer::Mob(mob)) = target.0 else {
            info!("[DEV DOT] no mob selected — ignoring");
            continue;
        };

        for msg in msgs {
            let dot = dot_for(msg.ty, link.0);
            match enemy_query.get_mut(mob) {
                Ok(Some(mut dots)) => {
                    dots.0.push(dot);
                }
                Ok(None) => {
                    commands.entity(mob).insert(DamageOverTimes(vec![dot]));
                }
                Err(_) => {
                    info!("[DEV DOT] {mob:?} not an enemy — ignoring");
                    continue;
                }
            }
            info!("[DEV DOT] applied {:?} DoT to {mob:?}", msg.ty);
        }
    }
}
