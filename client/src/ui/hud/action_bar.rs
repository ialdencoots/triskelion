use bevy::prelude::*;

use shared::components::player::{Class, Subclass};

const SLOT_SIZE: f32 = 64.0;
const SLOT_GAP: f32 = 6.0;
const BAR_BOTTOM_PAD: f32 = 18.0;

/// Maps a (class, subclass) pair to the stance icon asset path.
pub fn stance_icon_path(class: Class, subclass: Subclass) -> &'static str {
    match (class, subclass) {
        (Class::Physical, Subclass::Bulwark)     => "icons/stances/iron_stance.png",
        (Class::Physical, Subclass::Intercessor) => "icons/stances/flowing_guard.png",
        (Class::Physical, Subclass::Duelist)     => "icons/stances/edge_form.png",
        (Class::Arcane,   Subclass::Aegis)       => "icons/stances/null_field.png",
        (Class::Arcane,   Subclass::Conduit)     => "icons/stances/resonant_flow.png",
        (Class::Arcane,   Subclass::Arcanist)    => "icons/stances/overcharge.png",
        (Class::Nature,   Subclass::Wardbark)    => "icons/stances/deep_root.png",
        (Class::Nature,   Subclass::Mender)      => "icons/stances/pulse.png",
        (Class::Nature,   Subclass::Thornweave)  => "icons/stances/overgrowth.png",
        _ => "icons/stances/iron_stance.png",
    }
}

/// Keybind label and tooltip for each bar slot.
static SLOTS: &[(&str, &str)] = &[
    ("1", "Stance"),
    ("2", "Mobility"),
    ("3", "CC"),
    ("4", "Taunt"),
    ("5", "Interrupt"),
    ("6", "Exit Stance"),
];

#[derive(Component)]
pub struct ActionSlot(pub u8);

pub fn spawn_action_bar(mut commands: Commands) {
    let bar_width = SLOTS.len() as f32 * (SLOT_SIZE + SLOT_GAP) - SLOT_GAP;

    // Outer bar — fixed at bottom center
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
            for (i, (key, _tooltip)) in SLOTS.iter().enumerate() {
                bar.spawn((
                    ActionSlot(i as u8),
                    Node {
                        width: Val::Px(SLOT_SIZE),
                        height: Val::Px(SLOT_SIZE),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::End,
                        align_items: AlignItems::End,
                        padding: UiRect::all(Val::Px(2.0)),
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
                    // Keybind label in bottom-right corner
                    slot.spawn((
                        Text::new(*key),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(Color::srgba(0.75, 0.75, 0.75, 0.9)),
                    ));
                });
            }
        });
}
