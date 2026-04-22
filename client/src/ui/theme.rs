use bevy::prelude::*;

pub const PANEL_BG:      Color = Color::srgba(0.05, 0.05, 0.08, 0.85);
pub const PANEL_BG_DARK: Color = Color::srgba(0.04, 0.04, 0.06, 0.75);
pub const BORDER:        Color = Color::srgba(0.4, 0.4, 0.5, 0.5);
pub const HEALTH_BAR_BG: Color = Color::srgba(0.15, 0.05, 0.05, 0.8);
pub const HEALTH_FILL:   Color = Color::srgb(0.20, 0.72, 0.20);

pub const AVATAR_SELF:   Color = Color::srgba(0.25, 0.55, 0.25, 0.5);
pub const AVATAR_PARTY:  Color = Color::srgba(0.20, 0.40, 0.60, 0.5);
pub const AVATAR_ENEMY:  Color = Color::srgba(0.65, 0.20, 0.20, 0.5);

/// Nadir-zone green — matches the shader's innermost zone color; used by the
/// streak counter for activation flashes and by the arc ripple tint.
pub const NADIR_GREEN:   Color = Color::srgba(0.25, 1.00, 0.38, 1.0);
/// Apex-zone red — matches the shader's outer zone color; used by the streak
/// counter for break flashes and by the arc ripple tint.
pub const APEX_RED:      Color = Color::srgba(1.00, 0.22, 0.14, 1.0);

pub fn uniform_border() -> BorderColor {
    BorderColor { top: BORDER, bottom: BORDER, left: BORDER, right: BORDER }
}
