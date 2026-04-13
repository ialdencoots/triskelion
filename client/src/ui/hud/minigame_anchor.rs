use bevy::prelude::*;

const MINIGAME_W: f32 = 420.0;
const MINIGAME_H: f32 = 180.0;
/// Distance above the action bar (slot height 64 + bottom pad 18 + gap 10).
const ABOVE_ACTION_BAR: f32 = 64.0 + 18.0 + 10.0;

#[derive(Component)]
pub struct MinigameRoot;

/// Spawned once at startup: centered horizontally, above the action bar.
pub fn spawn_minigame_root(mut commands: Commands) {
    commands.spawn((
        MinigameRoot,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            bottom: Val::Px(ABOVE_ACTION_BAR),
            width: Val::Px(MINIGAME_W),
            height: Val::Px(MINIGAME_H),
            margin: UiRect::left(Val::Px(-MINIGAME_W / 2.0)),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.03, 0.03, 0.08, 0.75)),
        BorderColor {
            top:    Color::srgba(0.35, 0.35, 0.50, 0.5),
            bottom: Color::srgba(0.35, 0.35, 0.50, 0.5),
            left:   Color::srgba(0.35, 0.35, 0.50, 0.5),
            right:  Color::srgba(0.35, 0.35, 0.50, 0.5),
        },
    ));
}
