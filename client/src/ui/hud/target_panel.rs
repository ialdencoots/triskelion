use bevy::prelude::*;

use shared::components::combat::{Health, ReplicatedThreatList};
use shared::components::enemy::{EnemyMarker, EnemyName, MobTarget};
use shared::components::player::{PlayerId, PlayerName, PlayerSelectedTarget, SelectedMobOrPlayer};

use crate::ui::theme;
use crate::world::players::{OwnServerEntity, RemotePlayerMarker};
use crate::world::selection::SelectedTarget;

use super::frames::{CHAR_OFFSET, FRAME_H, FRAME_TOP_PCT, FRAME_W};

const MAX_BARS: usize = 5;
const BAR_ROW_H: f32 = 13.0;
const TOT_ROW_H: f32 = 28.0;

// ── Computed display state ────────────────────────────────────────────────────

#[derive(Resource, Default)]
pub struct ThreatDisplayData {
    pub show: bool,
    pub tot: Option<TotDisplay>,
    /// The entity that should become the new `SelectedTarget` when the ToT row is clicked.
    pub tot_entity: Option<Entity>,
    pub bars: [Option<BarDisplay>; MAX_BARS],
}

pub struct TotDisplay {
    pub name: String,
    pub health_pct: f32,
    pub avatar_color: Color,
}

pub struct BarDisplay {
    pub player_name: String,
    pub threat_pct: f32,
}

// ── Marker components ─────────────────────────────────────────────────────────

#[derive(Component)]
pub struct ThreatPanelRoot;

/// The target-of-target sub-row.
#[derive(Component)]
pub struct TotRow;

#[derive(Component)]
pub struct TotNameText;

/// Stores which entity to select when the ToT row is clicked.
#[derive(Component, Default)]
pub struct TotTargetEntity(pub Option<Entity>);

/// Avatar square inside the ToT row; color updated to match entity type.
#[derive(Component)]
pub struct TotAvatarBg;

/// Fill node inside the ToT mob health bar.
#[derive(Component)]
pub struct TotHealthFill;

/// One threat bar row; `usize` is the sorted index (0 = highest threat).
#[derive(Component)]
pub struct ThreatBarRow(pub usize);

/// Fill node inside a threat bar.
#[derive(Component)]
pub struct ThreatBarFill(pub usize);

/// Name label inside a threat bar row.
#[derive(Component)]
pub struct ThreatBarLabel(pub usize);

// ── Spawn ─────────────────────────────────────────────────────────────────────

pub fn spawn_target_panel(mut commands: Commands) {
    commands
        .spawn((
            ThreatPanelRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(FRAME_TOP_PCT),
                margin: UiRect {
                    left: Val::Px(CHAR_OFFSET),
                    top: Val::Px(FRAME_H + 2.0),
                    ..default()
                },
                width: Val::Px(FRAME_W),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(4.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(theme::PANEL_BG),
        ))
        .with_children(|panel| {
            // ── Target-of-target row ──────────────────────────────────────
            panel
                .spawn((
                    TotRow,
                    TotTargetEntity::default(),
                    Button,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(TOT_ROW_H),
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(5.0),
                        margin: UiRect::bottom(Val::Px(2.0)),
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                ))
                .with_children(|row| {
                    // Mini avatar — color driven by TotAvatarBg update system.
                    row.spawn((
                        TotAvatarBg,
                        Node {
                            width: Val::Px(22.0),
                            height: Val::Px(22.0),
                            flex_shrink: 0.0,
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                        theme::uniform_border(),
                    ));

                    // Name + health bar column
                    row.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        flex_grow: 1.0,
                        row_gap: Val::Px(3.0),
                        justify_content: JustifyContent::Center,
                        ..default()
                    })
                    .with_children(|col| {
                        col.spawn((
                            TotNameText,
                            Text::new(""),
                            TextFont { font_size: 9.5, ..default() },
                            TextColor(Color::srgb(0.80, 0.80, 0.80)),
                        ));
                        col.spawn((
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Px(5.0),
                                overflow: Overflow::clip(),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.15, 0.05, 0.05, 0.8)),
                        ))
                        .with_children(|bar| {
                            bar.spawn((
                                TotHealthFill,
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                BackgroundColor(theme::HEALTH_FILL),
                            ));
                        });
                    });
                });

            // ── Threat bar rows ───────────────────────────────────────────
            for i in 0..MAX_BARS {
                panel
                    .spawn((
                        ThreatBarRow(i),
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(BAR_ROW_H),
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(4.0),
                            display: Display::None,
                            ..default()
                        },
                    ))
                    .with_children(|row| {
                        row.spawn((
                            ThreatBarLabel(i),
                            Node {
                                width: Val::Px(58.0),
                                overflow: Overflow::clip(),
                                ..default()
                            },
                            Text::new(""),
                            TextFont { font_size: 9.0, ..default() },
                            TextColor(Color::srgb(0.72, 0.72, 0.72)),
                        ));
                        row.spawn((
                            Node {
                                flex_grow: 1.0,
                                height: Val::Px(6.0),
                                overflow: Overflow::clip(),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.10, 0.10, 0.12, 0.8)),
                        ))
                        .with_children(|bar| {
                            bar.spawn((
                                ThreatBarFill(i),
                                Node {
                                    width: Val::Percent(0.0),
                                    height: Val::Percent(100.0),
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.85, 0.65, 0.10)),
                            ));
                        });
                    });
            }
        });
}

// ── Compute ───────────────────────────────────────────────────────────────────

fn avatar_color_for(
    entity: Entity,
    own: &Option<Res<OwnServerEntity>>,
    remote_q: &Query<(), With<RemotePlayerMarker>>,
) -> Color {
    if own.as_ref().map(|r| r.0 == entity).unwrap_or(false) {
        theme::AVATAR_SELF
    } else if remote_q.contains(entity) {
        theme::AVATAR_PARTY
    } else {
        theme::AVATAR_ENEMY
    }
}

/// Reads the current selection and updates `ThreatDisplayData` each frame.
pub fn compute_threat_display(
    selected: Res<SelectedTarget>,
    mob_q: Query<(&EnemyName, Option<&Health>, Option<&ReplicatedThreatList>, Option<&MobTarget>), With<EnemyMarker>>,
    remote_q: Query<&PlayerSelectedTarget>,
    remote_marker_q: Query<(), With<RemotePlayerMarker>>,
    own_entity: Option<Res<OwnServerEntity>>,
    player_q: Query<(Entity, &PlayerId, &PlayerName, Option<&Health>), Without<EnemyMarker>>,
    mut data: ResMut<ThreatDisplayData>,
) {
    data.show = false;
    data.tot = None;
    data.tot_entity = None;
    data.bars = Default::default();

    let Some(target) = selected.0 else { return };

    let (entries, tot): (Vec<(u64, f32)>, Option<TotDisplay>) =
        if let Ok((_, _health, tl_opt, mob_target_opt)) = mob_q.get(target) {
            // Directly targeting a mob — show ToT if the mob has a chase target.
            let e = tl_opt.map(|tl| tl.entries.clone()).unwrap_or_default();
            let tot = if let Some(play_id) = mob_target_opt.and_then(|mt| mt.0) {
                if let Some((entity, _, name, health_opt)) = player_q.iter()
                    .find(|(_, pid, _, _)| pid.0 == play_id)
                {
                    let hp = health_opt
                        .map(|h| (h.current / h.max * 100.0).clamp(0.0, 100.0))
                        .unwrap_or(100.0);
                    let color = avatar_color_for(entity, &own_entity, &remote_marker_q);
                    data.tot_entity = Some(entity);
                    Some(TotDisplay { name: name.0.clone(), health_pct: hp, avatar_color: color })
                } else {
                    None
                }
            } else {
                None
            };
            (e, tot)
        } else if let Ok(ally_target) = remote_q.get(target) {
            match ally_target.0.as_ref() {
                None => return,
                Some(SelectedMobOrPlayer::Mob(mob_entity)) => {
                    // ToT is an enemy mob — show name, health, and threat bars.
                    let Ok((mob_name, health_opt, tl_opt, _)) = mob_q.get(*mob_entity) else { return };
                    let hp = health_opt
                        .map(|h| (h.current / h.max * 100.0).clamp(0.0, 100.0))
                        .unwrap_or(100.0);
                    let e = tl_opt.map(|tl| tl.entries.clone()).unwrap_or_default();
                    data.tot_entity = Some(*mob_entity);
                    (e, Some(TotDisplay { name: mob_name.0.clone(), health_pct: hp, avatar_color: theme::AVATAR_ENEMY }))
                }
                Some(SelectedMobOrPlayer::Player(play_id)) => {
                    // ToT is another PC — look up by PlayerId (stable across clients).
                    let found = player_q.iter()
                        .find(|(_, pid, _, _)| pid.0 == *play_id);
                    let Some((entity, _, name, health_opt)) = found else { return };
                    let hp = health_opt
                        .map(|h| (h.current / h.max * 100.0).clamp(0.0, 100.0))
                        .unwrap_or(100.0);
                    let color = avatar_color_for(entity, &own_entity, &remote_marker_q);
                    data.tot_entity = Some(entity);
                    (vec![], Some(TotDisplay { name: name.0.clone(), health_pct: hp, avatar_color: color }))
                }
            }
        } else {
            return;
        };

    // Nothing to show if no ToT and no threat entries.
    if entries.is_empty() && tot.is_none() {
        return;
    }

    data.show = true;
    data.tot = tot;

    let mut sorted = entries;
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let max_threat = sorted.first().map(|(_, t)| *t).unwrap_or(1.0).max(1.0);

    for (i, (pid, threat)) in sorted.iter().enumerate().take(MAX_BARS) {
        let player_name = player_q
            .iter()
            .find(|(_, id, _, _)| id.0 == *pid)
            .map(|(_, _, n, _)| n.0.clone())
            .unwrap_or_default();

        data.bars[i] = Some(BarDisplay {
            player_name,
            threat_pct: (threat / max_threat * 100.0).clamp(0.0, 100.0),
        });
    }
}

// ── Apply ─────────────────────────────────────────────────────────────────────

/// Applies `ThreatDisplayData` to all panel UI nodes.
/// `Without<>` filters on `Node` queries prove disjointness to Bevy's scheduler.
pub fn apply_threat_panel(
    data: Res<ThreatDisplayData>,
    // Node queries — each filtered to a disjoint entity set via Without<>.
    mut panel_q: Query<
        &mut Node,
        (With<ThreatPanelRoot>, Without<TotRow>, Without<TotHealthFill>, Without<ThreatBarRow>, Without<ThreatBarFill>),
    >,
    mut tot_row_q: Query<
        &mut Node,
        (With<TotRow>, Without<ThreatPanelRoot>, Without<TotHealthFill>, Without<ThreatBarRow>, Without<ThreatBarFill>),
    >,
    mut tot_health_q: Query<
        &mut Node,
        (With<TotHealthFill>, Without<ThreatPanelRoot>, Without<TotRow>, Without<ThreatBarRow>, Without<ThreatBarFill>),
    >,
    mut bar_row_q: Query<
        (&ThreatBarRow, &mut Node),
        (Without<ThreatPanelRoot>, Without<TotRow>, Without<TotHealthFill>, Without<ThreatBarFill>),
    >,
    mut bar_fill_q: Query<
        (&ThreatBarFill, &mut Node),
        (Without<ThreatPanelRoot>, Without<TotRow>, Without<TotHealthFill>, Without<ThreatBarRow>),
    >,
    mut tot_name_q: Query<&mut Text, (With<TotNameText>, Without<ThreatBarLabel>)>,
    mut bar_label_q: Query<(&ThreatBarLabel, &mut Text), Without<TotNameText>>,
    mut tot_avatar_q: Query<&mut BackgroundColor, With<TotAvatarBg>>,
    mut tot_target_q: Query<&mut TotTargetEntity, With<TotRow>>,
) {
    // Panel root visibility.
    if let Ok(mut node) = panel_q.single_mut() {
        node.display = if data.show { Display::Flex } else { Display::None };
    }

    // ToT row.
    if let Ok(mut node) = tot_row_q.single_mut() {
        node.display = if data.tot.is_some() { Display::Flex } else { Display::None };
    }
    if let Ok(mut text) = tot_name_q.single_mut() {
        text.0 = data.tot.as_ref().map(|t| t.name.clone()).unwrap_or_default();
    }
    if let Ok(mut node) = tot_health_q.single_mut() {
        node.width = Val::Percent(
            data.tot.as_ref().map(|t| t.health_pct).unwrap_or(100.0),
        );
    }
    if let Ok(mut bg) = tot_avatar_q.single_mut() {
        bg.0 = data.tot.as_ref().map(|t| t.avatar_color).unwrap_or(Color::NONE);
    }
    if let Ok(mut tot_target) = tot_target_q.single_mut() {
        tot_target.0 = data.tot_entity;
    }

    // Threat bar rows.
    for (ThreatBarRow(i), mut node) in bar_row_q.iter_mut() {
        node.display = if data.bars[*i].is_some() { Display::Flex } else { Display::None };
    }
    for (ThreatBarFill(i), mut node) in bar_fill_q.iter_mut() {
        node.width = Val::Percent(
            data.bars[*i].as_ref().map(|b| b.threat_pct).unwrap_or(0.0),
        );
    }
    for (ThreatBarLabel(i), mut text) in bar_label_q.iter_mut() {
        text.0 = data.bars[*i]
            .as_ref()
            .map(|b| b.player_name.clone())
            .unwrap_or_default();
    }
}

/// Selects the ToT entity when the ToT row is clicked; highlights on hover.
pub fn handle_tot_interaction(
    mut interaction_q: Query<
        (&Interaction, &mut BackgroundColor, &TotTargetEntity),
        (Changed<Interaction>, With<Button>, With<TotRow>),
    >,
    mut selected: ResMut<SelectedTarget>,
) {
    for (interaction, mut bg, tot_target) in interaction_q.iter_mut() {
        bg.0 = match interaction {
            Interaction::Pressed => {
                selected.0 = tot_target.0;
                Color::srgba(0.3, 0.3, 0.5, 0.4)
            }
            Interaction::Hovered => Color::srgba(0.15, 0.15, 0.3, 0.3),
            Interaction::None    => Color::srgba(0.0, 0.0, 0.0, 0.0),
        };
    }
}
