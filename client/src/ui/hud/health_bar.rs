//! Shared health-bar primitives for the HUD.
//!
//! Every panel that shows HP (player frame, target frame, party rows, ToT,
//! floating enemy bars) needs the same `(current/max*100).clamp(0,100)` and
//! the same background-clip + fill structure. Reuse here so a future visual
//! tweak propagates without touching every panel.

use bevy::prelude::*;

use shared::components::combat::Health;

use crate::ui::theme;

/// Health as a percentage [0, 100] suitable for `Val::Percent` on a fill node.
pub fn percent(health: &Health) -> f32 {
    (health.current / health.max * 100.0).clamp(0.0, 100.0)
}

/// Bundle for a horizontal bar background: clipped, full parent width,
/// `height_px` tall, theme-coloured.
pub fn bar_bundle(height_px: f32) -> impl Bundle {
    bar_bundle_styled(height_px, theme::HEALTH_BAR_BG)
}

/// As `bar_bundle` but with a custom background color — used by the floating
/// enemy bar which renders against the world and uses a deeper red swatch
/// than the panel bars do.
pub fn bar_bundle_styled(height_px: f32, bg: Color) -> impl Bundle {
    (
        Node {
            width: Val::Percent(100.0),
            height: Val::Px(height_px),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(bg),
    )
}

/// Bundle for the fill node: full bar width, theme green. Pair with a marker
/// component on the same entity so update systems can find it.
pub fn fill_bundle() -> impl Bundle {
    fill_bundle_styled(theme::HEALTH_FILL)
}

/// As `fill_bundle` but with a custom fill color (enemy bars).
pub fn fill_bundle_styled(color: Color) -> impl Bundle {
    (
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(color),
    )
}
