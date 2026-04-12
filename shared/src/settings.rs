use std::net::{Ipv4Addr, SocketAddr};

pub const SERVER_PORT: u16 = 5000;
/// Unique protocol ID to prevent cross-game connections.
pub const PROTOCOL_ID: u64 = 0x0000_4D47_5052; // "RPGM" as ASCII
/// Tick rate: 64 Hz server simulation.
pub const FIXED_TIMESTEP_HZ: f64 = 64.0;

pub fn server_addr() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::UNSPECIFIED, SERVER_PORT))
}

pub fn server_listen_addr() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::UNSPECIFIED, SERVER_PORT))
}

pub fn client_connect_addr() -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, SERVER_PORT))
}
