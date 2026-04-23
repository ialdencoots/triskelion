use bevy::prelude::*;

/// Named, ordered action-bar slots. The discriminant doubles as the index into
/// `ActionBarBindings::0` and `SlotClickPulse`, so any new slot must be appended
/// in the order the bindings layout expects.
///
/// Slots split into two bands:
/// - Primary — the main commit keys for the active class minigame. Physical
///   uses both; Arcane/Nature may use only `Primary1` depending on stance.
/// - Secondary — the three cube-edge selectors (Tank/Heal) or their
///   class-specific equivalents. Named after their on-screen direction so
///   the binding reads naturally regardless of which class is bound to them.
///
/// Default layout (see `ActionBarBindings::default`):
///   Stance1        — 1 — Tank stance
///   Stance2        — 2 — DPS stance
///   Stance3        — 3 — Heal stance
///   Primary2       — W — secondary arc commit (DPS)
///   Primary1       — R — primary arc commit
///   SecondaryLeft  — A — cube Left (Tank/Heal)
///   SecondaryDown  — X — cube Bottom (Tank/Heal)
///   SecondaryRight — G — cube Right (Tank/Heal)
///   SecondaryUp    — Q — (unused; reserved for the Physical DPS grid's
///                       up direction once the grid is implemented)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionSlot {
    Stance1 = 0,
    Stance2,
    Stance3,
    Primary2,
    Primary1,
    SecondaryLeft,
    SecondaryDown,
    SecondaryRight,
    SecondaryUp,
}

impl ActionSlot {
    pub const COUNT: usize = 9;

    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Maps each action bar slot (indexed by `ActionSlot`) to a `KeyCode`.
/// `.0[ActionSlot::X.index()]` reads the keybind for slot X.
#[derive(Resource)]
pub struct ActionBarBindings(pub Vec<KeyCode>);

impl Default for ActionBarBindings {
    fn default() -> Self {
        let mut keys = vec![KeyCode::Digit1; ActionSlot::COUNT];
        keys[ActionSlot::Stance1.index()]         = KeyCode::Digit1;
        keys[ActionSlot::Stance2.index()]         = KeyCode::Digit2;
        keys[ActionSlot::Stance3.index()]         = KeyCode::Digit3;
        keys[ActionSlot::Primary2.index()]        = KeyCode::KeyW;
        keys[ActionSlot::Primary1.index()]        = KeyCode::KeyR;
        keys[ActionSlot::SecondaryLeft.index()]   = KeyCode::KeyA;
        keys[ActionSlot::SecondaryDown.index()]   = KeyCode::KeyX;
        keys[ActionSlot::SecondaryRight.index()]  = KeyCode::KeyG;
        keys[ActionSlot::SecondaryUp.index()]     = KeyCode::KeyQ;
        Self(keys)
    }
}
