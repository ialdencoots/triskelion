# Physical Class — DAG / Grid Rework

## Overview

This document supersedes the DAG section of `minigame_spec.md` for the Physical class.
The DAG mechanic is differentiated by role:

- **Tank / Healer (Bulwark / Intercessor)**: Traditional flow DAG with timed junction
  inputs. Streak gates branch availability; timing precision amplifies modifier magnitude.
- **DPS (Duelist)**: A rectangular traversal grid that replaces the arc UI on activation.
  Streak is the step budget; arc commit quality history determines bonus magnitude at each
  node. Skill tree choices seed the grid's bonus type distribution and density.

The Arc mechanic is unchanged. All DAG / grid mechanics are active only while the
corresponding stance is engaged.

---

## Tank / Healer: Flow DAG

### World axis

A directed acyclic graph with 1–3 branch points per activation. Flow advances from the
entry node to the terminal node at a fixed world-controlled speed regardless of player
input. The graph structure is fixed per action but the modifier set available at each
branch point varies with current arc streak (see below).

At each branch point, up to 3 paths are available. Flow reaching the terminal node
resolves all collected modifiers and resets the DAG.

### Player input

One input per branch point. A **junction window** opens at a fixed distance before each
branch node and closes as flow passes it. Pressing the branch input within the window
selects the next path in the rotation.

**Miss behaviour**: missing the window auto-routes to the default path (lowest modifier
value). No additional penalty beyond the foregone bonus.

**Timing bonus**: pressing later within the open window (closer to the node) applies a
higher magnitude multiplier to the modifier delivered by that branch:

```
timing_bonus = lerp(1.0, max_bonus, (t_input - t_window_open) / window_duration)
```

`max_bonus` is a design parameter (suggested ~1.5×, to be tuned at balance time).

### Streak gating

The current arc streak determines how many of the 3 paths are available at each branch
point:

- **Low streak (0–2)**: only the default path is available. The other paths are dimmed.
- **Mid streak (3–6)**: the default path and one elevated path are available.
- **High streak (7+)**: all 3 paths are available, including any premium modifier option.

Unavailable paths are visually present but non-interactive. The player can see what they
are locked out of. A commit landing in the apex zone resets streak to zero immediately,
which may collapse available options mid-flow on an in-progress DAG.

**Composure (Bulwark design note)**: A candidate Bulwark-specific behaviour where
disruption-caused apex entries reduce streak by a fixed amount rather than resetting to
zero, distinguishing being hit from committing badly. Requires attributing each apex-zone
entry to either a disruption event or a player commit. Implementation: add a
`disruption_active: bool` flag to `ArcState`, set when an impulse is applied, checked when
streak-break is evaluated. Not yet resolved — flagged as a known design consideration.

### Branch modifiers

Paths at each branch node carry distinct modifier types. The exact set is action-specific,
but modifier types are drawn from the shared pool below. Not all types appear in every DAG
configuration — distribution is role-skewed and skill-tree-seeded.

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
  available at premium paths only
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
  counter-directional arc impulses; **premium path only**, high streak (7+) required

**Support**
- `AggroBonus` — increases threat generated by this action (Tank-primary)

Tank DAG activations skew toward `AggroBonus`, `StunOnHit`, `BaseDamage`, `Fracture`,
`CounterPressure`, and `Resolve` (premium). Healer DAG activations skew toward `Healing`,
`CooldownReduction`, `Amplify`, `Weaken`, and `Exhaust`. Duelist grid activations skew
toward `Bleed`, `Exploit`, `Surge`, `Buildup`, `Ricochet`, `Cleave`, `Footwork`, and
`FollowThrough`. These distributions are seeded by subclass skill tree choices (see Skill
Tree Seeding below).

### Modifier resolution

At the terminal node, all modifiers collected during traversal are applied atomically to
the activating action's output. The DAG then resets; `collected_modifiers` is cleared and
`flow_progress` returns to 0.

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

### Skill tree seeding

Grid bonus density and node type distribution are not dynamically computed from arc
performance. They are parameterised by **subclass skill tree choices** made outside of
combat. The random seed per activation draws from a distribution whose parameters the
player has configured.

Examples of skill tree seeding parameters:

- **Modifier type weights**: proportion of nodes seeded as `DamageOverTime` vs `BaseDamage`
  vs `StunOnHit` vs `CooldownReduction`. A "Hemorrhage" build weights heavily toward DoT;
  a "Disruptor" build weights toward CC and interrupt-adjacent modifiers.
- **Density range**: the min/max proportion of nodes that carry bonuses vs are empty. A
  high-density build produces richer grids requiring more complex routing; a low-density
  build produces sparse grids where the routing decision is simpler.
- **Critical mass cap**: a high cap produces infrequent large activations; a low cap
  produces frequent small ones. This interacts with density — frequent sparse grids vs
  infrequent rich grids is a meaningful build axis.

The same seeding principle applies to Tank and Healer DAGs: branch type distributions
and streak gate thresholds at junctions are skill tree parameters, giving all three Physical
roles a layer of build identity on top of the shared arc foundation.

---

## State changes required

### `ArcState` (shared/src/components/minigame/arc.rs)

Add a commit quality ring buffer:

```rust
pub recent_commit_qualities: VecDeque<f32>,   // capacity = critical_mass_cap
```

Populated on each commit alongside the existing `last_commit_quality`. Consumed (read,
not cleared) on grid activation to compute per-step bonus magnitudes.

### `DagState` (shared/src/components/minigame/dag.rs)

The existing fields (`flow_progress`, `next_branch_index`, `collected_modifiers`) cover
the Tank / Healer flow DAG. The DPS grid requires additional state. Whether this is
unified in `DagState` with a mode discriminant or split into a separate `GridState`
component is an implementation decision; both are viable.

DPS grid fields required:

```rust
pub grid: Vec<Vec<Option<DagModifier>>>,    // seeded at activation
pub path: Vec<(usize, usize)>,              // nodes visited in order
pub steps_remaining: u32,                   // streak budget remaining
pub active: bool,                           // overlay is open
```

### `tick_dag_states` / `tick_grid_states` (server/src/systems/minigame.rs)

Currently a stub. Needs implementation for:
- Tank/Healer: advancing `flow_progress`, detecting junction windows, consuming branch
  input, applying timing bonus, resolving terminal node
- DPS: detecting activation conditions (streak break, cap), seeding grid, consuming
  directional input, detecting blowout, resolving exit

### `render_dag` (client/src/ui/dag.rs)

Currently a stub. Needs implementation for:
- Tank/Healer: flow progress indicator, junction window highlight, available/locked branch
  display, streak tier indication, collected modifier icons
- DPS: grid overlay replacing arc display, visited node marking, dimmed self-intersection
  cells, step budget counter, bonus node type and magnitude preview

---

## Interaction summary (Physical — updated)

```
Arc commits → streak count
  → Tank/Healer: gates branch availability at DAG junctions
  → DPS: accumulates step budget for next grid activation

Arc commit quality → quality history buffer
  → DPS: per-step bonus magnitude on grid traversal

Streak break (apex commit) or critical mass cap
  → DPS: triggers grid overlay, step budget = streak at activation

Grid traversal
  → each step costs one streak arc; bonus nodes collect DagModifiers
  → dead-end blowout: all collected bonuses lost, streak consumed with no output
  → exit edge reached: bonuses resolve, overlay closes, arc play resumes

DAG junction timing (Tank/Healer)
  → timing_bonus = lerp(1.0, max_bonus, t_input / window_duration)
  → missed window: auto-route to default, no additional penalty

Disruption (all roles)
  → counter-directional arc impulse → degrades commit quality → reduces streak
  → Tank/Healer: may collapse available junction paths mid-flow
  → DPS: activation with disrupted quality history → reduced bonus magnitudes on grid nodes

Skill tree seeding
  → configures grid/DAG bonus type distribution, density, and activation cap
  → build identity expressed through what the grid looks like, not arc execution
```
