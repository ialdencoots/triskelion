use bevy::prelude::*;

use shared::components::combat::CombatState;
use shared::components::player::RoleStance;

use crate::systems::keybindings::ActionBarBindings;
use crate::world::players::OwnServerEntity;

const SLOT_SIZE: f32 = 64.0;
const SLOT_GAP: f32 = 6.0;
const BAR_BOTTOM_PAD: f32 = 18.0;

/// Role/function labels for each slot (displayed at the top of stance slots).
static SLOT_LABELS: &[&str] = &["Tank", "DPS", "Heal", "Secondary", "Primary", "Cube-L", "Cube-B", "Cube-R"];

/// Marks the keybind text node for a given slot so it can be updated
/// when bindings change.
#[derive(Component)]
pub struct SlotKeybindText(pub u8);

/// Per-slot stance color used for the active-stance highlight.
/// Matches the same palette as the group-frame avatars for visual consistency.
const TANK_COLOR: Color = Color::srgb(0.30, 0.55, 1.00); // blue
const DPS_COLOR:  Color = Color::srgb(1.00, 0.40, 0.20); // orange-red
const HEAL_COLOR: Color = Color::srgb(0.25, 0.90, 0.45); // green

#[derive(Component)]
pub struct ActionSlot(pub u8);

/// One-frame pulse set by mouse clicks on action-bar slots. `gather_and_send_input`
/// OR-combines it with the keyboard press for the same slot, so clicks and
/// keypresses share a single activation path through `PlayerInput`.
#[derive(Resource, Default)]
pub struct SlotClickPulse(pub [bool; 8]);

pub fn spawn_action_bar(mut commands: Commands, bindings: Res<ActionBarBindings>) {
    let num_slots = SLOT_LABELS.len();
    let bar_width = num_slots as f32 * (SLOT_SIZE + SLOT_GAP) - SLOT_GAP;

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
            for (i, label) in SLOT_LABELS.iter().enumerate() {
                let key_str = keycode_label(bindings.0.get(i).copied());
                bar.spawn((
                    ActionSlot(i as u8),
                    Button,
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
                    if !label.is_empty() {
                        slot.spawn((
                            Text::new(*label),
                            TextFont { font_size: 10.0, ..default() },
                            TextColor(Color::srgba(0.70, 0.70, 0.80, 0.85)),
                        ));
                    } else {
                        slot.spawn(Node::default());
                    }

                    // Keybind label — bottom-right corner.
                    slot.spawn((
                        SlotKeybindText(i as u8),
                        Node {
                            align_self: AlignSelf::FlexEnd,
                            ..default()
                        },
                        Text::new(key_str),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(Color::srgba(0.75, 0.75, 0.75, 0.9)),
                    ));
                });
            }
        });
}

/// Syncs keybind text labels whenever `ActionBarBindings` is mutated.
pub fn update_keybind_labels(
    bindings: Res<ActionBarBindings>,
    mut text_q: Query<(&SlotKeybindText, &mut Text)>,
) {
    if !bindings.is_changed() {
        return;
    }
    for (slot, mut text) in text_q.iter_mut() {
        text.0 = keycode_label(bindings.0.get(slot.0 as usize).copied());
    }
}

fn keycode_label(key: Option<KeyCode>) -> String {
    match key {
        Some(KeyCode::Digit1) => "1".into(),
        Some(KeyCode::Digit2) => "2".into(),
        Some(KeyCode::Digit3) => "3".into(),
        Some(KeyCode::Digit4) => "4".into(),
        Some(KeyCode::Digit5) => "5".into(),
        Some(KeyCode::Digit6) => "6".into(),
        Some(KeyCode::KeyA)   => "A".into(),
        Some(KeyCode::KeyB)   => "B".into(),
        Some(KeyCode::KeyC)   => "C".into(),
        Some(KeyCode::KeyD)   => "D".into(),
        Some(KeyCode::KeyE)   => "E".into(),
        Some(KeyCode::KeyF)   => "F".into(),
        Some(KeyCode::KeyG)   => "G".into(),
        Some(KeyCode::KeyH)   => "H".into(),
        Some(KeyCode::KeyI)   => "I".into(),
        Some(KeyCode::KeyJ)   => "J".into(),
        Some(KeyCode::KeyK)   => "K".into(),
        Some(KeyCode::KeyL)   => "L".into(),
        Some(KeyCode::KeyM)   => "M".into(),
        Some(KeyCode::KeyN)   => "N".into(),
        Some(KeyCode::KeyO)   => "O".into(),
        Some(KeyCode::KeyP)   => "P".into(),
        Some(KeyCode::KeyQ)   => "Q".into(),
        Some(KeyCode::KeyR)   => "R".into(),
        Some(KeyCode::KeyS)   => "S".into(),
        Some(KeyCode::KeyT)   => "T".into(),
        Some(KeyCode::KeyU)   => "U".into(),
        Some(KeyCode::KeyV)   => "V".into(),
        Some(KeyCode::KeyW)   => "W".into(),
        Some(KeyCode::KeyX)   => "X".into(),
        Some(KeyCode::KeyY)   => "Y".into(),
        Some(KeyCode::KeyZ)   => "Z".into(),
        Some(KeyCode::Space)  => "Spc".into(),
        Some(KeyCode::Tab)    => "Tab".into(),
        None                  => "".into(),
        _                     => "?".into(),
    }
}

/// Records a one-frame click pulse for any slot whose button just entered the
/// `Pressed` state. `gather_and_send_input` reads and clears the pulse so the
/// click drives the same `PlayerInput` path as the bound key.
pub fn handle_action_slot_click(
    mut pulse: ResMut<SlotClickPulse>,
    interaction_q: Query<(&Interaction, &ActionSlot), (Changed<Interaction>, With<Button>)>,
) {
    for (interaction, slot) in interaction_q.iter() {
        if matches!(interaction, Interaction::Pressed) {
            if let Some(bit) = pulse.0.get_mut(slot.0 as usize) {
                *bit = true;
            }
        }
    }
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
