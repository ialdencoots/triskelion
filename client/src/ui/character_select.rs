use bevy::prelude::*;
use shared::components::player::{Class, Subclass};

use crate::plugin::{AppState, ClassChosen};

#[derive(Component)]
pub struct ClassSelectRoot;

#[derive(Component)]
pub struct ClassSelectButton(pub Class);

pub fn spawn_class_select(mut commands: Commands) {
    commands
        .spawn((
            ClassSelectRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(20.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 1.0)),
            GlobalZIndex(100),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("Choose Your Class"),
                TextFont { font_size: 28.0, ..default() },
                TextColor(Color::WHITE),
            ));

            for (label, class, color) in [
                ("Physical", Class::Physical, Color::srgb(0.85, 0.55, 0.20)),
                ("Arcane",   Class::Arcane,   Color::srgb(0.45, 0.35, 0.90)),
                ("Nature",   Class::Nature,   Color::srgb(0.25, 0.75, 0.35)),
            ] {
                root.spawn((
                    ClassSelectButton(class),
                    Button,
                    Node {
                        width: Val::Px(220.0),
                        height: Val::Px(54.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.08, 0.08, 0.14, 0.92)),
                    BorderColor::all(color),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(label),
                        TextFont { font_size: 18.0, ..default() },
                        TextColor(color),
                    ));
                });
            }
        });
}

pub fn despawn_class_select(
    mut commands: Commands,
    root_q: Query<Entity, With<ClassSelectRoot>>,
) {
    for e in root_q.iter() {
        commands.entity(e).despawn();
    }
}

pub fn handle_class_select(
    interaction_q: Query<(&Interaction, &ClassSelectButton), (Changed<Interaction>, With<Button>)>,
    mut class_chosen: ResMut<ClassChosen>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for (interaction, btn) in interaction_q.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        class_chosen.0 = Some(btn.0.clone());
        next_state.set(AppState::InGame);
    }
}

pub fn default_subclass(class: &Class) -> Subclass {
    match class {
        Class::Physical => Subclass::Duelist,
        Class::Arcane   => Subclass::Arcanist,
        Class::Nature   => Subclass::Thornweave,
    }
}
