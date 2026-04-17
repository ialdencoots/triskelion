use bevy::prelude::*;
use lightyear::prelude::*;

use shared::channels::GameChannel;
use shared::instances::InstanceKind;
use shared::messages::RequestInstanceMsg;

// ── Marker ────────────────────────────────────────────────────────────────────

/// Marks the "Enter Crystal Caverns" button so the interaction system can
/// distinguish it from other buttons (e.g. party-frame rows).
#[derive(Component)]
pub struct EnterInstanceButton {
    pub kind: InstanceKind,
}

// ── Spawn ─────────────────────────────────────────────────────────────────────

pub fn spawn_instance_button(mut commands: Commands) {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(20.0),
            left: Val::Percent(50.0),
            // Pull left by half the button width so it centres on-screen.
            margin: UiRect::left(Val::Px(-90.0)),
            width: Val::Px(180.0),
            height: Val::Px(36.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        Button,
        BackgroundColor(Color::srgba(0.08, 0.08, 0.14, 0.85)),
        BorderColor {
            top: Color::srgba(0.5, 0.4, 0.8, 0.6),
            bottom: Color::srgba(0.5, 0.4, 0.8, 0.6),
            left: Color::srgba(0.5, 0.4, 0.8, 0.6),
            right: Color::srgba(0.5, 0.4, 0.8, 0.6),
        },
        EnterInstanceButton { kind: InstanceKind::CrystalCaverns },
    ))
    .with_children(|parent| {
        parent.spawn((
            Text::new("Enter Crystal Caverns"),
            TextFont { font_size: 13.0, ..default() },
            TextColor(Color::srgb(0.85, 0.75, 1.0)),
        ));
    });
}

// ── Interaction ───────────────────────────────────────────────────────────────

/// Sends `RequestInstanceMsg` when the button is clicked and tints it on hover.
pub fn handle_instance_button(
    mut interaction_q: Query<
        (&Interaction, &mut BackgroundColor, &EnterInstanceButton),
        (Changed<Interaction>, With<Button>),
    >,
    mut sender_q: Query<&mut MessageSender<RequestInstanceMsg>>,
) {
    for (interaction, mut bg, btn) in interaction_q.iter_mut() {
        match interaction {
            Interaction::Pressed => {
                bg.0 = Color::srgba(0.25, 0.15, 0.45, 0.95);
                if let Ok(mut sender) = sender_q.single_mut() {
                    sender.send::<GameChannel>(RequestInstanceMsg { kind: btn.kind });
                    info!("[CLIENT] Sent RequestInstanceMsg kind={:?}", btn.kind);
                }
            }
            Interaction::Hovered => {
                bg.0 = Color::srgba(0.15, 0.10, 0.28, 0.90);
            }
            Interaction::None => {
                bg.0 = Color::srgba(0.08, 0.08, 0.14, 0.85);
            }
        }
    }
}
