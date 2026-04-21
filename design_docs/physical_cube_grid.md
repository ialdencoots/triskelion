# Physical Class — Cube / Grid

## Overview

This document supersedes the DAG section of `minigame_spec.md` for the Physical class.
The bonus-routing mechanic is differentiated by role:

- **Tank / Healer (Bulwark / Intercessor)**: A **cube overlay** triggered on streak cap.
  The cube's visible edges fill toward bonus markers; timing inputs on those markers
  collect the bonus and rotate the cube, exposing a fresh face. Several rotations per
  activation.
- **DPS (Duelist)**: A rectangular **traversal grid** that replaces the arc UI on
  activation. Streak is the step budget; arc commit quality history determines bonus
  magnitude at each node. Skill tree choices seed the grid's bonus type distribution and
  density.

Both overlays are triggered by reaching a streak cap. For the Duelist, the grid also
triggers on streak break. The Arc mechanic itself is unchanged. Overlays are active only
while the corresponding stance is engaged.

The tank/healer cube and the DPS grid share a common architectural pattern: arc play
builds streak → reaching a threshold activates an overlay → the player spends a budget of
decisions → overlay resolves and returns the player to arc play. The difference is in the
shape of the overlay and the interaction mode (timed rotations vs. free directional
routing).

---

## Tank / Healer: Cube

### Activation

The cube activates when arc streak reaches a configurable cap. Streak break (an apex
commit) does **not** trigger the cube — it simply resets streak to zero. Cap-only
activation keeps the fiction clean: taking damage does not cause a bonus cashout, and the
event is earned by sustained clean play rather than by failure.

Reaching the cap consumes the streak. Streak resets to zero at the moment of activation
and arc play rebuilds from zero during the cube. Players with strong multitasking arrive
at cube-exit with meaningful streak already accumulated, compounding throughput.

### Structure

The cube is a rendered frame around (or behind) the arc. Three of its visible edges are
interactive: **left**, **bottom**, and **right**. The top edge is reserved — the player
character sits above the arc and must not be visually obstructed.

A **fill animation** sweeps from the corners of each interactive edge toward that edge's
center. At the center of each edge sits a **bonus marker**. When the fill reaches the
marker, a timing window opens; pressing the corresponding input within the window
collects that bonus.

Only one bonus is collected per rotation. The first successful input collects its bonus,
rotates the cube in that edge's direction, and presents a fresh face with three new
bonus markers. Fills then restart from the corners of the new face's edges.

### Fill cadence and miss behavior

The fill sweeps continuously. If the player misses all three bonus windows on a given
face, the fill **resets and sweeps again** — the face persists until the player lands a
hit. There is no dead-face / blank-face punishment state.

The implicit cost of camping a face is attentional: the arc continues running during the
cube (see below), so every missed fill cycle is a moment spent hunting cube timing rather
than committing clean arc nadirs. The rate-limiter is structural, not artificial.

Magnitude tuning constraint: per-bonus timing multipliers must stay within a range where
**two mid-quality bonuses beat one perfect bonus**. This is what prevents camping from
dominating the strategy. Suggested bound: perfect ≈ 1.5×, sloppy ≈ 1.0×. Wider ranges
invert the incentive.

### Arc interaction during the cube

The arc is **not suspended** while the cube is active. Tank/healer have a single arc to
manage, so juggling the arc and the cube is a meaningful skill expression. Strong
multitaskers rebuild streak faster during the cube, triggering the next activation
sooner.

This is the primary difference from the Duelist grid, which *does* suspend arc play
because the Duelist is already juggling dual arcs and a third concurrent layer is not
reasonable.

Disruption during the cube: the arc still takes counter-directional impulses from
incoming hits as normal. Cube rotation timing is independent of the arc and is not
directly perturbed by disruption — though disruption still degrades arc commit quality,
which degrades the streak being rebuilt for the next cube activation.

### Rotation count

The number of rotations per activation is a design parameter. Because the cube is a
visual abstraction rather than a topologically faithful 6-faced solid (see "Cube as
animation" below), rotation count is decoupled from face count.

Open tuning question. Viable options:

- **Fixed baseline** (e.g. 4 rotations per activation) with skill tree modifiers that
  trade length for magnitude. Short-sharp (3 rotations, higher per-bonus magnitude)
  vs. long-rich (6 rotations, lower per-bonus magnitude) is a real build axis.
- **Scaled by some other run parameter** — not streak quality (which already gates bonus
  tier) and not streak count (which is always the cap value at activation). Could be a
  skill tree stat or an encounter-derived parameter.

The constraint to respect: rotation count × fill cycle duration should be short enough
that the cube does not dominate the fight. ~5–10 seconds total feels right.

### Bonus tier — streak quality gating

The *type* of bonus that can appear on a cube face is gated by the **aggregate quality**
of the streak that triggered the cube. Aggregate quality is the mean of
`recent_commit_qualities` over the `cap` most recent arc commits.

```
q < 0.4         → default-tier pool only
0.4 ≤ q < 0.7   → mix of default + mid
q ≥ 0.7         → mid + premium, with occasional default
```

Within each tier, the **skill tree seeds the weighting** of which specific bonus types
can appear. A "Disruptor" Bulwark's default pool weights toward CC; a "Brawler" Bulwark's
default pool weights toward raw damage.

This gives the quality signal a meaningful role beyond magnitude scaling (its role on
the Duelist grid): on the cube, quality determines *what is available to collect* as
well as *how good those collections are*.

### Bonus magnitude — per-bonus timing

Unlike the Duelist grid (where magnitude is pre-determined by the quality history at
each step), cube bonus magnitudes are determined by the player's **timing precision on
the fill marker at the moment of collection**. A cleanly timed hit delivers a higher
magnitude than a sloppy one, within the tier-bounded range established by streak
quality.

```
magnitude = tier_range.lerp(timing_precision)
```

This gives the cube two independent skill axes operating simultaneously: (1) the arc
technique that built the streak, determining tier; (2) the cube timing, determining
within-tier magnitude.

### Cube as animation, not topology

The cube is a visual metaphor, not a simulated 6-faced object. The game does not
track which face you are currently on in a geometric sense, and faces are not reused
within a single activation (there is no "bouncing between the same two bonuses"). Each
rotation generates a fresh face with a fresh set of three bonus markers drawn from the
seeded pool, and no-revisit emerges naturally from the fact that the previous face is
simply gone.

This simplifies implementation significantly: there is no face-adjacency graph to
simulate, no per-face state to persist, and rotation count is not capped by the 6
faces of a real cube.

### Lookahead (deferred)

For planning depth, consider rendering **three faded bonus markers** just past each of
the three visible bonus markers — representing what the next face's bonus would be if
the player rotates in that direction. Gives the player one-move lookahead within the
single-primitive visualization.

Status: **deferred**. Implement the basic cube first; add lookahead as a subsequent
design pass.

---

## Bonus pool (shared between cube and grid)

Collected bonuses are drawn from the pool below. Distributions within the pool are
role-skewed and skill-tree-seeded.

**Output**
- `BaseDamage` — flat damage increment applied to the activating action
- `Bleed` — stacking physical DoT; stacks independently from `DamageOverTime`
- `DamageOverTime` — generic DoT application
- `Exploit` — bonus damage if the target has any active impairment; scales with impairment
  count

**Impairment**
- `Daze` — partial agency removal; target can act but movement and cast speed are reduced
- `Exhaust` — reduces target's action and cast speed for a short duration
- `Fracture` — reduces target's damage resistance for a short duration; magnitude is a
  design parameter
- `StunOnHit` — full agency removal for a short duration
- `Weaken` — reduces the magnitude of the target's next outgoing action

**Chaining**
- `Amplify` — refreshes and extends the duration of all DoTs currently active on the target
- `Buildup` — stacks a multiplier on the current target; the accumulated multiplier pays out
  as bonus damage on the next activation against the same target, then resets
- `Cleave` — the resolved effect splashes to all enemies within a short radius of the target
  at reduced magnitude
- `CooldownReduction` — shaves a flat duration off all ability cooldowns
- `CooldownReset` — resets the cooldown of the ability with the shortest remaining cooldown;
  available at premium tier only
- `FollowThrough` — the next Physical action taken within a short window inherits a copy of
  the highest-magnitude modifier collected during this traversal
- `Ricochet` — after the primary hit resolves, a secondary hit fires at a random nearby
  enemy at reduced magnitude
- `Surge` — bonus damage if the previous resolved action was also a Physical action

**Positioning**
- `Anchor` — prevents the target from using mobility abilities for a short duration
- `Footwork` — brief movement speed increase applied to the player after resolution
- `Menace` — briefly creates a slow-zone around the player after resolution; enemies who
  enter the zone have their movement speed reduced; radius and duration are design parameters

**Defensive**
- `CounterPressure` — the next hit taken within a short window reflects a portion of its
  damage back to the attacker as raw damage
- `Healing` — direct restoration (Healer-primary)
- `Resolve` — brief disruption immunity: incoming hits deal damage normally but do not apply
  counter-directional arc impulses; **premium tier only**, requires high streak quality

**Support**
- `AggroBonus` — increases threat generated by this action (Tank-primary)

Tank cube activations skew toward `AggroBonus`, `StunOnHit`, `BaseDamage`, `Fracture`,
`CounterPressure`, and `Resolve` (premium). Healer cube activations skew toward `Healing`,
`CooldownReduction`, `Amplify`, `Weaken`, and `Exhaust`. Duelist grid activations skew
toward `Bleed`, `Exploit`, `Surge`, `Buildup`, `Ricochet`, `Cleave`, `Footwork`, and
`FollowThrough`. These distributions are seeded by subclass skill tree choices (see Skill
Tree Seeding below).

### Immediate vs. charged resolution

Bonuses split into resolution modes based on whether they have a natural attachment to an
activating action:

- **Rider** (resolves on the next N arc commits post-activation, where N is a tuning
  parameter): `BaseDamage`, `Bleed`, `StunOnHit`, `Fracture`, `Weaken`, `Exploit`,
  `Cleave`, `Ricochet`, `AggroBonus`, `DamageOverTime`, `Daze`, `Exhaust`, `Anchor`,
  `Amplify`, `Buildup`, `Surge`, `Menace`. No meaning except as a rider on a hit.
- **Window buff** (self-buff on a short timer): `CounterPressure`, `Resolve`, `Footwork`,
  `FollowThrough`. Already time-boxed; no inventory management needed.
- **Charge** (held in a small inventory until manually spent): `Healing`, `CooldownReset`.
  Player discretion over when to spend is meaningful for these. Charge cap per type is
  small (1–2 slots). Exceeding the cap drops the oldest.

This split preserves the Intercessor's design constraint that Chi Burst Heal is "a reward
for sustained discipline, not an emergency button" — a cap of 1 on the Healing charge
means the player earns it, holds it, and spends it, but cannot stockpile a burst phase's
worth.

---

## DPS: Traversal Grid

### Overview

The traversal grid is the Duelist's primary output mechanic, not one ability among several.
Other Duelist abilities — mobility, CC, interrupt — are standalone tools independent of grid
state.

The grid is presented as a rectangular overlay that replaces the arc display when active.
The player routes through the grid collecting bonuses, then returns to arc play.

### Grid structure

A rectangular grid of **3–4 columns wide × 4–6 rows deep** (exact dimensions are a design
parameter, seeded per activation within this range). Each intersection is a node. Nodes
carry a bonus or are empty; bonus density and type distribution are seeded by skill tree
parameters (see Skill Tree Seeding below).

Entry is from one edge; exit is from the opposite edge. Reaching any node on the exit edge
with steps remaining triggers resolution and cashes in all collected bonuses.

### Activation

The grid activates automatically — there is no manual activation input. Activation occurs
when either of the following conditions is met:

1. **Streak break**: a commit lands in the apex zone, resetting streak to zero. The grid
   activates immediately with the streak count at the moment of the apex commit as the
   step budget.
2. **Critical mass cap**: streak reaches a configurable ceiling. The grid activates even
   if the streak has not broken. The exact cap is a design parameter, possibly surfaced as
   a skill tree variable.

The step budget — equal to the streak count at activation — determines how many grid moves
the player can make. A small streak produces a short run with limited routing options; a
large streak allows extended traversal with access to more bonus nodes.

### Routing window

On activation, the two-arc display is replaced by the grid overlay. Arc input is suspended
for the duration of the routing window. The player inputs directional moves
(up / down / left / right) to traverse the grid.

The Duelist's arc suspension during the grid is deliberate: Duelist uses dual arcs, and
asking the player to juggle dual arcs plus grid routing is not reasonable. Tank and Healer
have only a single arc and are expected to multitask during their cube; Duelist is not.

The player is motivated to route quickly and exit, since each moment in the overlay is time
not spent rebuilding streak. There is no explicit time pressure — the overlay persists until
the player exits or blows out — but the incentive to return to arc play is structural.

Path confirmation occurs automatically when the player reaches the exit edge, or manually
via a confirm input. On confirmation, all collected bonuses resolve and the overlay closes.

### Movement rules

Movement is 4-directional. At each interior node the player has at most 3 available moves:
the 4 cardinal neighbours minus the node just visited (no-backtrack). At edge and corner
nodes fewer moves are available. Visited nodes are visually marked and cannot be revisited
(no self-intersection).

Each move costs one step from the step budget. The remaining step count is displayed during
routing. A move that would exhaust the budget before the exit edge is reached may be
attempted — but if no further moves are available and the exit edge has not been reached,
a blowout occurs.

### Dead-end and blowout

A **blowout** occurs when the player reaches a node with no valid moves and has not yet
reached the exit edge. On blowout:

- All bonuses collected on the current path are lost.
- The streak steps committed to the route are consumed with no output.
- The overlay closes and arc play resumes with streak reset to zero.

Blowouts are a natural consequence of winding routes that box the player into a corner.
The risk/reward tension is explicit: direct routes to the exit edge are safe but collect
fewer bonuses; winding routes through dense bonus clusters risk a blowout that yields
nothing.

### Bonus magnitude — quality history

Each collected bonus node has a **magnitude** determined by the arc commit quality that
maps to that step. This requires a quality history buffer on `ArcState`:

```
recent_commit_qualities: ring buffer of f32, capacity = critical_mass_cap
```

On each arc commit, `last_commit_quality` is appended to the buffer. On grid activation,
step N's bonus magnitude is read from the N-th most recent entry in the buffer:

```
bonus_magnitude(step N) = recent_commit_qualities[streak_count - N]
```

The most recent commit maps to the first step; the oldest commit in the budget maps to
the final step. This means commit quality at the moment just before activation (the apex
commit that broke the streak, or the final nadir before cap) has the highest influence on
early routing bonuses.

The ghost arc history already renders recent commit theta positions client-side. The quality
buffer provides the parallel data needed to show commit quality as a "what I have to spend"
signal.

The cube also uses this buffer — it reads the aggregate mean to gate bonus tier on
activation. The buffer is a shared `ArcState` field, populated on every commit regardless
of subclass.

### Special grid node types

In addition to standard bonus nodes and empty nodes, the Duelist grid supports **structural
node types** that alter traversal behaviour rather than delivering a modifier directly.
These are seeded per activation alongside standard nodes; their frequency and type weighting
are skill tree parameters.

**Echo** — on entry, duplicates the modifier (type and magnitude) of the last bonus node
visited and adds a copy to the collected pool. If no bonus node has been visited yet, Echo
resolves as empty.

**Mimic** — on grid exit, scans the collected bonus list and adopts the type of the
most-frequently-collected modifier in the current run. Magnitude is read from the quality
history entry mapped to its step position in the normal way. Rewards routing toward a
consistent modifier type rather than spreading across the grid.

**Anchor** — designates the current collected pool as a **floor**. If a blowout occurs
after passing an Anchor node, the player exits with the floor bonuses rather than nothing.
The most recently passed Anchor supersedes any earlier ones. Anchor nodes carry no direct
bonus value — routing through one is a conservative choice that trades potential bonus
density for blowout protection.

---

## Skill tree seeding

Overlay bonus density and node/face type distribution are not dynamically computed from
arc performance. They are parameterised by **subclass skill tree choices** made outside of
combat. The random seed per activation draws from a distribution whose parameters the
player has configured.

Examples of skill tree seeding parameters:

- **Modifier type weights**: proportion of nodes/faces seeded as `DamageOverTime` vs
  `BaseDamage` vs `StunOnHit` vs `CooldownReduction`. A "Hemorrhage" build weights heavily
  toward DoT; a "Disruptor" build weights toward CC and interrupt-adjacent modifiers.
- **Density range** (Duelist grid): the min/max proportion of nodes that carry bonuses vs
  are empty. A high-density build produces richer grids requiring more complex routing; a
  low-density build produces sparse grids where the routing decision is simpler.
- **Critical mass cap**: a high cap produces infrequent large activations; a low cap
  produces frequent small ones. This interacts with density / tier-gating — frequent
  sparse activations vs infrequent rich activations is a meaningful build axis.
- **Cube rotation count** (Tank/Healer): a skill tree axis trading length for per-bonus
  magnitude. Short-sharp (fewer rotations, higher magnitude) vs long-rich (more rotations,
  lower magnitude).

The same seeding principle applies across all three Physical roles, giving them a layer of
build identity on top of the shared arc foundation.

---

## State changes required

### `ArcState` (shared/src/components/minigame/arc.rs)

Add a commit quality ring buffer:

```rust
pub recent_commit_qualities: VecDeque<f32>,   // capacity = critical_mass_cap
```

Populated on each commit alongside the existing `last_commit_quality`. Consumed by both
the cube (aggregate mean on activation → tier gate) and the grid (per-step magnitude).

### `CubeState` (shared/src/components/minigame/cube.rs)

Replaces the former `DagState`. Fields required:

```rust
pub active: bool,                             // cube overlay is open
pub current_face: [Option<PhysicalBonus>; 3], // L/B/R bonus markers on the current face
pub fill_progress: f32,                       // [0, 1] fill animation
pub rotations_remaining: u32,                 // decremented on each successful rotation
pub collected: Vec<(PhysicalBonus, f32)>,     // (bonus, timing-precision) pairs
```

### `GridState` (shared/src/components/minigame/grid.rs, new)

Extracted from the former `DagState` (which previously conflated both mechanics):

```rust
pub active: bool,
pub grid: Vec<Vec<Option<PhysicalBonus>>>,    // seeded at activation
pub path: Vec<(usize, usize)>,                // nodes visited in order
pub steps_remaining: u32,                     // streak budget remaining
```

### `tick_cube_states` / `tick_grid_states` (server/src/systems/minigame.rs)

Currently stubs. Needs implementation for:
- Cube: cap-trigger detection, face seeding, fill animation, timing window detection,
  rotation on hit, magnitude computation, resolution on budget exhaustion
- Grid: cap/break trigger detection, seeding, directional input, blowout detection,
  exit resolution

### `render_cube` (client/src/ui/cube.rs) / `render_grid` (client/src/ui/grid.rs)

Currently stubs. Needs implementation for:
- Cube: edge fill visualization, bonus marker rendering, rotation animation, collected
  bonus list, current tier indication
- Grid: grid overlay replacing arc display, visited node marking, step budget counter,
  bonus node preview

---

## Interaction summary (Physical — updated)

```
Arc commits → streak count + commit quality history
  → Tank/Healer: streak accrues until cap is reached
  → DPS: streak accrues until cap is reached OR until streak breaks

Tank/Healer — cube activation:
  → cap reached → streak resets, cube opens
  → aggregate quality of the capped streak → tier gate for face bonuses
  → skill tree → weighting within each tier
  → arc continues running during the cube (multitasking skill)
  → each rotation: fill animation → player times input on a bonus → collects + rotates
  → timing precision → magnitude within tier range
  → miss all three: fill cycles, face persists
  → rotations exhausted: bonuses resolve (rider / window / charge split)

DPS — grid activation:
  → cap reached OR streak break → grid opens, arc suspends
  → streak at activation = step budget
  → recent_commit_qualities → per-step bonus magnitudes
  → skill tree → grid seeding (density, type distribution, structural nodes)
  → 4-directional movement, no-revisit
  → reach exit edge: bonuses resolve, grid closes, arc resumes (streak from zero)
  → dead-end with budget remaining: blowout, bonuses lost (unless Anchor passed)

Disruption (all roles)
  → counter-directional arc impulse → degrades commit quality → lowers streak tier
  → Tank/Healer: lower aggregate quality on next cube → lower-tier bonus pool
  → DPS: degraded quality history → reduced bonus magnitudes on next grid run

Bonus resolution (rider / window / charge)
  → rider bonuses: ride the next N arc commits post-activation
  → window bonuses: self-buff for a short duration
  → charge bonuses: deposit into small inventory, spent on player input
```
