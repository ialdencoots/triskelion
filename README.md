# Triskelion

A skill-based multiplayer action RPG built with Rust and the Bevy game engine. Three mechanically distinct character classes each use a pair of coupled minigames to determine combat output — timing, precision, and resource management replace traditional cooldown rotations.

## Tech Stack

- **[Bevy 0.18](https://bevyengine.org/)** — ECS game engine
- **[Lightyear 0.26](https://github.com/cBournhonesque/lightyear)** — client-server networking (UDP, prediction, replication)
- **[Avian3D 0.6](https://github.com/Jondolf/avian)** — 3D physics
- **[bevy-tnua](https://github.com/idanarye/bevy-tnua)** — character controller
- **[fastnoise-lite](https://github.com/Auburn/FastNoiseLite)** — procedural terrain

## Getting Started

**Prerequisites:** Rust toolchain with Cargo.

```bash
# Run the server (headless, port 5000)
cargo run --release -p server

# Run the client (connects to localhost:5000)
cargo run --release -p client
```

## Project Structure

```
triskelion/
├── shared/     # Types, components, messages, game rules
├── server/     # Headless server: game logic, instances, AI
├── client/     # Renderer, UI, input, physics prediction
└── design_docs/
    ├── minigame_spec.md
    └── subclasses_and_abilities.md
```

## Classes & Minigames

Each class has two coupled minigames. The server runs all minigame simulations at 64 Hz and replicates state to clients; the client renders overlay UI from replicated state.

### Physical — Arc + DAG

**Arc:** A dot oscillates sinusoidally along a semicircular track, moving fastest at the center and slowest at the edges. The player commits when the dot reaches a desired position; proximity to center determines commit quality (0–1). Incoming hits apply counter-directional impulse, creating irregular oscillations.

**DAG:** Each action presents a directed graph with 1–3 branch points. Flow advances autonomously; the player selects branches by timing inputs within a window. Later inputs score higher multipliers. Available paths depend on current streak quality.

| Subclass | Role | Focus |
|---|---|---|
| Bulwark | Tank | Aggro, stun, high-streak reflection |
| Intercessor | Healer | Chi-based deflection, streak-gated healing |
| Duelist | DPS | Ramp damage, DoT stacking |

### Arcane — Bar Fill + Wave Interference

**Bar Fill:** A bar fills non-linearly (slow start, rapid acceleration). The player commits at any point to deposit the fill value into an Arcane Pool and reset the bar. Bonus markers at random positions grant multipliers when hit within tolerance. Pool scales super-linearly above 100%, decaying passively.

**Wave Interference:** A complex traveling wave scrolls leftward past a standing wave. The player commits to freeze an interference segment, which travels off-screen and contributes its area to Wave Accumulation. Aegis players minimize accumulation (cancellation); Conduit/Arcanist players maximize it (amplification).

| Subclass | Role | Focus |
|---|---|---|
| Aegis | Tank | Destructive interference; Pressure mitigation |
| Conduit | Healer | Constructive interference; bonus-marker cadence |
| Arcanist | DPS | Burst damage via Pool detonation scaled by Potential |

### Nature — Value Lock + Heartbeat

**Value Lock:** The player holds to fill a bar, releasing to lock the current value as a target heartbeat frequency. Releasing near the previous lock value increments an entrainment streak. Frequency interpolates smoothly toward target.

**Heartbeat:** A circle pulses with a physiological envelope (spike, echo, diastolic flat). The player commits at the spike; the delivered value is pulse charge × frequency multiplier × entrainment multiplier. High-frequency play yields many small commits; low-frequency play yields infrequent but large ones. Incoming hits cause tachycardia, shifting frequency and degrading commit windows.

| Subclass | Role | Focus |
|---|---|---|
| Wardbark | Tank | Low-frequency, disruption-resilient mitigation |
| Mender | Healer | Mid-frequency sustained healing via entrainment |
| Thornweave | DPS | High-frequency DoT; demanding entrainment uptime |

### Universal Actions

- **Taunt** — force enemy aggro onto caster
- **Interrupt** — cancel telegraphed enemy abilities

## Architecture Notes

The server is authoritative. Minigame state machines tick in `FixedUpdate` at 64 Hz on the server. Clients predict player movement locally via physics, receive replicated minigame state, and render overlay UI without running their own simulations. Enemy positions replicate unreliably; player positions reliably.

Instances (aside from overworld) support up to 5 players each and are managed server-side. Terrain is generated procedurally with Perlin noise.
