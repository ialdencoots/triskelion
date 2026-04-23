use bevy::prelude::*;

use shared::components::combat::Health;
use shared::components::instance::InstanceId;
use shared::components::player::{PlayerId, PlayerName};

use crate::plugin::LocalClientId;
use crate::ui::theme;
use crate::world::selection::SelectedTarget;

use super::health_bar;

const PANEL_W: f32 = 180.0;
const ROW_H: f32 = 44.0;
const AVATAR_SIZE: f32 = 36.0;

// ── Marker components ─────────────────────────────────────────────────────────

#[derive(Component)]
pub struct GroupFrameRoot;

/// Container for the local player's row (always rendered first).
#[derive(Component)]
pub(super) struct SelfSection;

/// Container for all remote party members' rows.
#[derive(Component)]
pub(super) struct OthersSection;

/// One row in the party frame.  Stores the server-replicated entity it represents
/// so update systems can read `PlayerName` and `Health` from it.
#[derive(Component)]
pub struct PartyRow(pub Entity);

/// The green health fill inside a party row.
#[derive(Component)]
pub(super) struct PartyHealthFill(pub(super) Entity);

/// The name text inside a party row.
#[derive(Component)]
pub(super) struct PartyNameText(pub(super) Entity);

// ── Spawn ─────────────────────────────────────────────────────────────────────

pub fn spawn_group_frame(mut commands: Commands) {
    let self_section = commands
        .spawn((SelfSection, Node { flex_direction: FlexDirection::Column, ..default() }))
        .id();

    let others_section = commands
        .spawn((
            OthersSection,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                ..default()
            },
        ))
        .id();

    commands
        .spawn((
            GroupFrameRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(10.0),
                top: Val::Px(10.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(3.0),
                padding: UiRect::all(Val::Px(6.0)),
                min_width: Val::Px(PANEL_W),
                ..default()
            },
            BackgroundColor(theme::PANEL_BG_DARK),
        ))
        .add_children(&[self_section, others_section]);
}

// ── Row building ──────────────────────────────────────────────────────────────

fn spawn_party_row(commands: &mut Commands, game_entity: Entity, is_self: bool) -> Entity {
    let avatar_tint = if is_self { theme::AVATAR_SELF } else { theme::AVATAR_PARTY };

    let fill = commands
        .spawn((PartyHealthFill(game_entity), health_bar::fill_bundle()))
        .id();

    let bar_bg = commands
        .spawn(health_bar::bar_bundle(8.0))
        .add_child(fill)
        .id();

    let name_text = commands
        .spawn((
            PartyNameText(game_entity),
            Text::new(""),
            TextFont { font_size: 10.0, ..default() },
            TextColor(Color::srgb(0.85, 0.85, 0.85)),
        ))
        .id();

    let col = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            row_gap: Val::Px(2.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .add_children(&[name_text, bar_bg])
        .id();

    let avatar = commands
        .spawn((
            Node {
                width: Val::Px(AVATAR_SIZE),
                height: Val::Px(AVATAR_SIZE),
                flex_shrink: 0.0,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(avatar_tint),
            theme::uniform_border(),
        ))
        .id();

    commands
        .spawn((
            PartyRow(game_entity),
            Button,
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(5.0),
                padding: UiRect::all(Val::Px(3.0)),
                width: Val::Percent(100.0),
                height: Val::Px(ROW_H),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        ))
        .add_children(&[avatar, col])
        .id()
}

// ── Observers ─────────────────────────────────────────────────────────────────

/// Fires when the server replicates a `PlayerId` to this client.
/// Inserts a party row into the correct section of the group frame.
pub fn on_party_member_added(
    trigger: On<Add, PlayerId>,
    local_id: Res<LocalClientId>,
    player_id_q: Query<&PlayerId>,
    self_section_q: Query<Entity, With<SelfSection>>,
    others_section_q: Query<Entity, With<OthersSection>>,
    mut commands: Commands,
) {
    let game_entity = trigger.event_target();
    let Ok(play_id) = player_id_q.get(game_entity) else { return };
    let is_self = play_id.0 == local_id.0;

    let row = spawn_party_row(&mut commands, game_entity, is_self);

    if is_self {
        let Ok(section) = self_section_q.single() else { return };
        commands.entity(section).add_child(row);
    } else {
        let Ok(section) = others_section_q.single() else { return };
        commands.entity(section).add_child(row);
    }
}

/// Fires when a player disconnects and their `PlayerId` is removed.
/// Despawns the matching party row (and its children).
pub fn on_party_member_removed(
    trigger: On<Remove, PlayerId>,
    row_q: Query<(Entity, &PartyRow)>,
    mut commands: Commands,
) {
    let game_entity = trigger.event_target();
    for (row_entity, row) in row_q.iter() {
        if row.0 == game_entity {
            commands.entity(row_entity).despawn();
            break;
        }
    }
}

// ── Update systems ────────────────────────────────────────────────────────────

/// Refreshes name text and health bars for every party row each frame.
pub fn update_party_rows(
    player_names: Query<&PlayerName>,
    health_q: Query<&Health>,
    mut name_q: Query<(&PartyNameText, &mut Text)>,
    mut fill_q: Query<(&PartyHealthFill, &mut Node)>,
) {
    for (PartyNameText(game_entity), mut text) in name_q.iter_mut() {
        if let Ok(name) = player_names.get(*game_entity) {
            if text.0 != name.0 {
                text.0 = name.0.clone();
            }
        }
    }
    for (PartyHealthFill(game_entity), mut node) in fill_q.iter_mut() {
        let pct = health_q
            .get(*game_entity)
            .map(health_bar::percent)
            .unwrap_or(100.0);
        node.width = Val::Percent(pct);
    }
}

/// Fades rows whose game entity is in a different instance than the local player.
pub fn update_party_row_fade(
    local_id: Res<LocalClientId>,
    player_q: Query<(&PlayerId, Option<&InstanceId>)>,
    mut name_q: Query<(&PartyNameText, &mut TextColor)>,
    mut fill_q: Query<(&PartyHealthFill, &mut BackgroundColor)>,
) {
    let local_instance = player_q
        .iter()
        .find(|(pid, _)| pid.0 == local_id.0)
        .and_then(|(_, iid)| iid.copied());

    let same_instance = |game_entity: Entity| -> bool {
        let member_instance = player_q.get(game_entity).ok().and_then(|(_, iid)| iid.copied());
        match (local_instance, member_instance) {
            (Some(l), Some(m)) => l == m,
            _ => true,
        }
    };

    for (PartyNameText(e), mut color) in name_q.iter_mut() {
        let alpha = if same_instance(*e) { 0.85 } else { 0.30 };
        color.0 = color.0.with_alpha(alpha);
    }
    for (PartyHealthFill(e), mut bg) in fill_q.iter_mut() {
        let alpha = if same_instance(*e) { 1.0 } else { 0.30 };
        bg.0 = bg.0.with_alpha(alpha);
    }
}

/// Highlights rows on hover and sets `SelectedTarget` when any party row is
/// clicked — including the local player's own row (needed for self-targeted
/// abilities like heals).
pub fn handle_party_row_interaction(
    mut interaction_q: Query<
        (&Interaction, &mut BackgroundColor, &PartyRow),
        (Changed<Interaction>, With<Button>),
    >,
    mut selected: ResMut<SelectedTarget>,
) {
    for (interaction, mut bg, PartyRow(game_entity)) in interaction_q.iter_mut() {
        bg.0 = match interaction {
            Interaction::Pressed => {
                selected.0 = Some(*game_entity);
                Color::srgba(0.3, 0.3, 0.5, 0.4)
            }
            Interaction::Hovered => Color::srgba(0.15, 0.15, 0.3, 0.3),
            Interaction::None => Color::srgba(0.0, 0.0, 0.0, 0.0),
        };
    }
}
