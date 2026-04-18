use bevy::prelude::*;

/// Maps each action bar slot (0-indexed) to a `KeyCode`.
///
/// Slots 0-2 are stance keys (1/2/3) and intentionally duplicated here so the
/// action bar can read a single source of truth for all slot labels.
/// Slots 3-5 are fully remappable by the player in the future.
///
/// Default layout:
///   3 — W — secondary commit (DPS second weapon, deferred)
///   4 — R — primary commit
///   5 — Q — reserved for future use
#[derive(Resource)]
pub struct ActionBarBindings(pub Vec<KeyCode>);

impl Default for ActionBarBindings {
    fn default() -> Self {
        Self(vec![
            KeyCode::Digit1, // slot 0 — Tank stance
            KeyCode::Digit2, // slot 1 — DPS stance
            KeyCode::Digit3, // slot 2 — Heal stance
            KeyCode::KeyW,   // slot 3 — secondary commit
            KeyCode::KeyR,   // slot 4 — primary commit
            KeyCode::KeyQ,   // slot 5 — reserved
        ])
    }
}
