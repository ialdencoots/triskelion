use bevy::prelude::*;

use shared::components::combat::CombatState;
use shared::components::player::RoleStance;

use crate::world::players::OwnServerEntity;

const SLOT_SIZE: f32 = 64.0;
const SLOT_GAP: f32 = 6.0;
const BAR_BOTTOM_PAD: f32 = 18.0;

/// Slots 0–2 are the three role stances; 3–5 are other abilities.
static SLOTS: &[(&str, &str)] = &[
    ("1", "Tank"),
    ("2", "DPS"),
    ("3", "Heal"),
    ("4", "Mobility"),
    ("5", "CC"),
    ("6", "Taunt"),
];

/// Per-slot stance color used for the active-stance highlight.
/// Matches the same palette as the group-frame avatars for visual consistency.
const TANK_COLOR: Color = Color::srgb(0.30, 0.55, 1.00); // blue
const DPS_COLOR:  Color = Color::srgb(1.00, 0.40, 0.20); // orange-red
const HEAL_COLOR: Color = Color::srgb(0.25, 0.90, 0.45); // green

#[derive(Component)]
pub struct ActionSlot(pub u8);

pub fn spawn_action_bar(mut commands: Commands) {
    let bar_width = SLOTS.len() as f32 * (SLOT_SIZE + SLOT_GAP) - SLOT_GAP;

    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(BAR_BOTTOM_PAD),
            left: Val::Percent(50.0),
            margin: UiRect::left(Val::Px(-bar_width / 2.0)),
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(SLOT_GAP),
            ..default()
        })
        .with_children(|bar| {
            for (i, (key, label)) in SLOTS.iter().enumerate() {
                bar.spawn((
                    ActionSlot(i as u8),
                    Node {
                        width: Val::Px(SLOT_SIZE),
                        height: Val::Px(SLOT_SIZE),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::SpaceBetween,
                        align_items: AlignItems::Stretch,
                        padding: UiRect::all(Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.5)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.05, 0.05, 0.10, 0.88)),
                    BorderColor {
                        top:    Color::srgba(0.45, 0.45, 0.55, 0.7),
                        bottom: Color::srgba(0.45, 0.45, 0.55, 0.7),
                        left:   Color::srgba(0.45, 0.45, 0.55, 0.7),
                        right:  Color::srgba(0.45, 0.45, 0.55, 0.7),
                    },
                ))
                .with_children(|slot| {
                    // Stance slots (0–2): show a role label at the top.
                    if i < 3 {
                        slot.spawn((
                            Text::new(*label),
                            TextFont { font_size: 10.0, ..default() },
                            TextColor(Color::srgba(0.70, 0.70, 0.80, 0.85)),
                        ));
                    } else {
                        // Non-stance slots: empty top so keybind stays at bottom.
                        slot.spawn(Node::default());
                    }

                    // Keybind label — bottom-right corner.
                    slot.spawn((
                        Node {
                            align_self: AlignSelf::FlexEnd,
                            ..default()
                        },
                        Text::new(*key),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(Color::srgba(0.75, 0.75, 0.75, 0.9)),
                    ));
                });
            }
        });
}

/// Highlights the active stance slot (0–2) and dims the other two.
/// Runs every frame but only mutates when `CombatState` has changed.
pub fn update_stance_highlight(
    own_entity: Option<Res<OwnServerEntity>>,
    combat_q: Query<&CombatState, Changed<CombatState>>,
    mut slot_q: Query<(&ActionSlot, &mut BackgroundColor, &mut BorderColor)>,
) {
    // Only proceed when our own CombatState has actually changed.
    let Some(own) = own_entity else { return };
    let Ok(combat) = combat_q.get(own.0) else { return };

    for (slot, mut bg, mut border) in slot_q.iter_mut() {
        let (is_active, role_color) = match slot.0 {
            0 => (combat.active_stance == Some(RoleStance::Tank), TANK_COLOR),
            1 => (combat.active_stance == Some(RoleStance::Dps),  DPS_COLOR),
            2 => (combat.active_stance == Some(RoleStance::Heal),  HEAL_COLOR),
            _ => continue, // non-stance slots are unaffected
        };

        if is_active {
            bg.0 = Color::srgba(0.10, 0.10, 0.20, 0.95);
            let bc = role_color.with_alpha(0.90);
            border.top    = bc;
            border.bottom = bc;
            border.left   = bc;
            border.right  = bc;
        } else {
            bg.0 = Color::srgba(0.05, 0.05, 0.10, 0.88);
            let bc = Color::srgba(0.45, 0.45, 0.55, 0.70);
            border.top    = bc;
            border.bottom = bc;
            border.left   = bc;
            border.right  = bc;
        }
    }
}
