//! Shared scaffolding for system-level integration tests.
//!
//! Builds the smallest possible Bevy `App` capable of running game-logic
//! systems: a scheduler plus a manually-managed `Time` resource, with no
//! `MinimalPlugins` (whose `TimePlugin` would auto-advance and fight our
//! deterministic stepping). No window, renderer, or networking.

// Each `tests/*.rs` file is compiled as its own crate that pulls this module
// in independently — so a helper used by some tests but not others trips the
// unused-code lint per binary. Silencing here keeps the noise out of CI.
#![allow(dead_code)]

use std::time::Duration;

use bevy::prelude::*;

pub fn minimal_app() -> App {
    let mut app = App::new();
    app.init_resource::<Time>();
    app
}

/// Advance the app's `Time` by `dt`, then run one `Update` tick. Use this
/// instead of `app.update()` directly so `time.delta_secs()` is non-zero
/// and reproducible across test runs.
pub fn advance(app: &mut App, dt: Duration) {
    app.world_mut().resource_mut::<Time>().advance_by(dt);
    app.update();
}
