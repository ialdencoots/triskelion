use std::net::{Ipv4Addr, SocketAddr};

pub const SERVER_PORT: u16 = 5000;
/// Unique protocol ID to prevent cross-game connections.
pub const PROTOCOL_ID: u64 = 0x0000_4D47_5052; // "RPGM" as ASCII
/// Tick rate: 64 Hz server simulation.
pub const FIXED_TIMESTEP_HZ: f64 = 64.0;

/// Y offset from terrain surface to the center of a player capsule
/// (radius + half-cylinder height + a small gap so the capsule base doesn't
/// intersect the mesh). Shared between client physics, server spawn placement,
/// and dead-reckoning floor clamping.
pub const PLAYER_FLOAT_HEIGHT: f32 = 1.1;

/// Y offset from terrain surface for boss-sized capsules (larger radius +
/// taller cylinder). Only used where rendering/placement code is
/// boss-aware (e.g., enemy mesh anchoring on the client).
pub const BOSS_FLOAT_HEIGHT: f32 = 2.2;

/// Vertical-velocity magnitude above which the local physics capsule is
/// considered airborne (used to suppress Tnua's resting spring oscillation
/// from being relayed to other clients as jump jitter).
pub const AIRBORNE_VY_THRESHOLD: f32 = 1.0;

/// Height above the terrain surface above which the local physics capsule
/// is considered airborne even when `vy ≈ 0` (catches the jump apex).
/// Must exceed `PLAYER_FLOAT_HEIGHT` plus the resting spring oscillation.
pub const AIRBORNE_HEIGHT_THRESHOLD: f32 = 2.2;

pub fn server_addr() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::UNSPECIFIED, SERVER_PORT))
}

pub fn server_listen_addr() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::UNSPECIFIED, SERVER_PORT))
}

pub fn client_connect_addr() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, SERVER_PORT))
}
