/// Ordered-reliable channel for authoritative game events: ability activations,
/// damage events, stance changes, spawn/despawn notifications.
pub struct GameChannel;

/// Unordered-unreliable channel for high-frequency positional updates.
/// Dropped packets are acceptable; stale data is superseded by the next frame.
pub struct PositionChannel;
