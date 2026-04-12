# Minigame Mechanics Specification

## Overview

Each class has two mechanics. These mechanics are not independent: one produces a continuous
performance signal and the other modifies or routes output based on that signal.
Mechanics activate on entering combat, or may be initiated via specific actions — they are
not running at all times. When active, the world axis of each mechanic advances
autonomously regardless of player input. Player skill is expressed through precision and
timing of inputs against that autonomous process.

---

## Class: Physical

### Mechanics
- **Arc** — continuous performance signal; commit quality tracked as ghost arc history
- **DAG** — each action activation presents its own DAG; branch choices apply modifiers
  to that action's output

---

### Arc

#### World axis
A semicircular track. A dot travels along it sinusoidally — slow at the apexes, fast at the
nadir. The dot's position is parameterised as:

```
theta(t) = pi/2 + A * sin(omega * t + phi)
```

where `A` defines the sweep width and `omega` defines the oscillation frequency. The dot's
speed at any position is the derivative of `theta` with respect to time.

The arc is divided into three zones by angular distance from the nadir:
- **Nadir zone**: innermost ~20% of the sweep arc. Dot velocity is highest here.
- **Mid zone**: next ~60%.
- **Apex zone**: outermost ~20% near each apex. Dot velocity is lowest here.

#### Player input
Single commit input. On press, the current zone and dot velocity are sampled.

**Lockout**: exactly one commit is permitted per apex-to-nadir-to-apex traverse (i.e. per
half-oscillation). The lockout engages immediately on commit and releases when the dot next
reaches the opposite apex. This means the maximum commit rate is one per half-period,
occurring at the nadir. Attempting to commit during lockout has no effect.

Commit quality is a continuous value proportional to dot velocity at the moment of commit:

```
commit_quality = dot_velocity / peak_velocity   [range 0–1]
```

A nadir commit produces quality ≈ 1.0; an apex commit produces quality ≈ 0.

#### Disruption
An incoming hit applies a velocity impulse to the dot in the **opposite direction to its
current motion**, creating a damped irregular oscillation that decays back to the clean
sinusoidal pattern over approximately 2 seconds. Impulse magnitude is determined by the hit.
Multiple overlapping disruptions compound additively. The counter-directional impulse means
disruption always works against the dot's momentum — it cannot accidentally push the dot
toward the nadir.

#### Intermediate state: Ghost arc history
There is no Momentum bar. Commit quality is represented spatially as a **ghost arc history**
rendered below the active arc. On each commit, a ghost copy of the arc is dropped into the
history stack, with the commit marker position frozen at the dot's angle at commit time.

Ghost arcs accumulate downward. The vertical alignment of commit markers across the stack
visualises streak quality:
- A run of nadir commits produces markers that are all aligned near the bottom centre —
  visually forming a near-straight vertical line through the stack.
- Apex commits or disrupted commits produce markers offset left or right of centre.
- Mixed quality produces a jagged, irregular marker column.

Ghost arcs fade out after a fixed number of entries (suggested: 4–6 visible at once) to
prevent visual clutter.

**Streak** is defined as the count of consecutive commits landing in the nadir zone without
an apex-zone commit intervening. Streak is tracked as an integer and is the primary input
to DAG path quality (see below). A commit landing in the apex zone resets streak to zero.
Mid-zone commits neither increment nor reset the streak.

---

### DAG

#### Overview
The DAG is not a fixed routing structure that determines which action fires. Instead, each
action activation presents its own DAG. When the player activates an action, that action's
specific DAG begins flowing. The DAG's traversal applies modifiers to the action's base
output — branch choices determine *how* the action executes, not *what* fires. The action
fires at the terminal node regardless of path taken; the path determines the modifier set
applied.

#### World axis
Flow advances autonomously from entry node to terminal node at a fixed speed, regardless of
player input. The graph structure is defined per action and may contain 1–3 branch points.
At each branch point, 2–4 paths are available. If no input is received before flow passes a
branch node, flow auto-routes to the default path (the first path, lowest modifier value).

Branch count and graph depth are action-specific design parameters. More complex actions
may have deeper graphs with more branch points; simpler actions may have a single branch
or none.

#### Player input
One input per branch point — pressing the branch input when flow is within the **branch
window** of an upcoming node selects the next path in the rotation. The branch window opens
at a fixed distance before the node and closes as flow passes it. Pressing outside the
window has no effect.

**Timing bonus**: pressing closer to the node (later within the open window) applies a
higher timing multiplier to the modifier value delivered by that branch:

```
timing_bonus = lerp(1.0, max_bonus, (input_time - window_open) / window_duration)
```

where `max_bonus` is a design parameter (TBD at balance time, suggested ~1.5×).

#### Branch modifiers
Paths at a branch node each carry a distinct modifier type, not a generic value tier.
Example modifier types (exact set per action TBD at design time):

- Base damage increment
- Damage-over-time application
- Minor stun on hit
- Cooldown reduction on next action
- Healing component
- Aggro bonus

The modifier set available at a branch point is action-specific. Not all branch points
offer the same options — some may offer a choice between two damage variants, others between
a damage modifier and a utility modifier.

**Streak interaction**: the current arc streak determines the **quality tier** of paths
available at each branch point. At low streak (0–2), only base modifier variants are
available. At higher streak levels, additional or stronger modifier options unlock at branch
points. This means a player maintaining clean nadir commits gains access to more impactful
branch options, not a larger timing window. The paths are visually distinct at different
streak tiers — unavailable paths are dimmed or absent.

#### Action gating
Action activation is gated by **attack timing** alone — each action has an internal
cooldown independent of arc or DAG state. There is no resource cost. The arc and DAG
mechanics determine output quality and modifier access, not whether the action can fire.

---

### Mechanic interaction summary (Physical)

Arc commit quality → streak count → determines which DAG modifier paths are available.
Arc ghost history → visual record of recent commit quality → player self-assessment signal.
Action activation → presents action-specific DAG → flow advances autonomously.
DAG branch inputs → apply modifier set to action output → timing bonus scales modifier value.
Disruption → counter-directional impulse on dot → degrades commit quality → reduces streak
→ limits available DAG modifier paths.

---

## Class: Arcane

### Mechanics
- **Bar Fill** — generates Arcane Pool
- **Wave Interference** — accumulates Wave Pressure or Wave Potential

---

### Bar Fill

#### World axis
A horizontal bar that fills from left to right. Fill progress advances autonomously according
to a non-linear curve:

```
fill_rate(p) = base_rate * power(p, exponent)
```

where `p` is current fill proportion (0–1) and `exponent > 1` produces slow initial fill with
rapid acceleration near full. When fill reaches 1.0, the bar instantly resets to 0.0. The
rendered position and the true fill value are always identical — there is no separation
between display and internal state.

#### Disruption
An incoming hit drains the bar by an amount proportional to the hit's interference magnitude.
This is a direct reduction of the true fill value:

```
fill_after_hit = max(0, fill_before_hit - drain_amount)
```

`drain_amount` is determined by the hit. The player sees the bar drop immediately and
accurately. Under heavy disruption the bar may be repeatedly reset before a commit can be
made.

#### Player input
Single press commit. On press, the current fill value is transferred into the Arcane Pool:

```
pool_gain = current_fill
```

The bar resets to 0.0 immediately on commit. There is no lockout. Committing at low fill is
valid but low value; the decision is whether to commit early (lower gain, avoids the hard
reset) or hold longer (higher gain, risks disruption drain or auto-reset).

#### Bonus markers
At the start of each fill cycle, a small number of **bonus markers** are placed at randomly
selected fill percentages on the bar. These are visible to the player. If the player commits
within a tolerance delta of a bonus marker's position, the bonus associated with that marker
is added to the Arcane Pool alongside the regular fill transfer:

```
if |current_fill - marker_position| < delta:
    pool_gain += marker_bonus_value
```

Bonus types are drawn from a small set (exact set TBD at design time; examples: pool output
multiplier, reduced pool cost on next action, secondary effect on next action). This creates
a tactical incentive to sometimes commit at a non-maximal fill value if a bonus marker is
positioned there, rather than always waiting for the highest fill before reset.

#### Intermediate state: Arcane Pool
Arcane Pool accumulates from bar fill commits. Pool has a hard cap of approximately 500%.
Pool decays passively at a fixed rate. Pool level is visible to the player.

Pool output scales super-linearly above 100%: actions fired with pool above 100% produce
bonus output proportional to the overage. This incentivises the hoarding decision against
the passive decay pressure.

---

### Wave Interference

#### World axis — Traveling wave
A complex waveform composed of three sinusoidal components at non-harmonically-related
spatial frequencies, scrolling leftward at a fixed speed `SPD`:

```
tw(x, t) = A * (c1 * sin(K1*(x + SPD*t))
              + c2 * sin(K2*(x + SPD*t) + phi2)
              + c3 * sin(K3*(x + SPD*t) + phi3))
```

Frequency and amplitude coefficients are design parameters; the constraint is that K1, K2,
K3 are non-harmonically related so the composite pattern does not repeat with a short period.

#### World axis — Standing wave
A spatially fixed, temporally oscillating waveform present only within the commit zone:

```
sw(x, t) = A * PAMP * sin(K1*x + phi) * cos(omega_sw * t)
```

`PAMP` is a player-stat-derived constant — not player-adjustable in real time. `omega_sw`
defines the standing wave's temporal oscillation frequency and is a design parameter. Nodes
and antinodes are spatially fixed; the envelope oscillates in time.

#### Commit zone
A fixed region on the left side of the display. The traveling wave enters from the right.
The standing wave exists only within the zone.

Outside the zone, the traveling wave is rendered raw, allowing the player to read the
incoming wave's character before it arrives. Within the zone, the traveling wave is rendered
as the interference result `tw(x,t) + sw(x,t)`, so the player simultaneously observes the
standing wave's current phase and the interference it is producing on the incoming wave.

#### Player input
Single commit input. On commit, the zone-width segment of the result wave is frozen at
`t_commit`:

```
committed_segment(x) = tw(x, t_commit) + sw(x, t_commit)  for x in [0, ZW]
```

The frozen segment travels leftward at wave speed `SPD` and exits the zone over a duration
of `ZW / SPD`. While a segment is traveling, the zone continues to display live interference
and new commits are not blocked by in-transit segments.

**Commit cooldown**: one commit is permitted per half standing-wave period, matching the
cadence of the arc mechanic. This allows a commit at each amplitude maximum and each
amplitude minimum of the standing wave. Multiple committed segments may coexist in the zone.

#### Intermediate state: Wave Accumulation
As each committed segment exits the left boundary of the zone, the **absolute area** under
that segment's result curve is computed and added to the Wave Accumulation counter:

```
area = integral of |committed_segment(x)| dx   over zone width [0, ZW]
```

Uncommitted wave — portions of the traveling wave that pass through the zone while no commit
is active — contribute their full raw area to Wave Accumulation:

```
area_uncommitted = integral of |tw(x, t)| dx   over zone width, per unit time * dt
```

Wave Accumulation is therefore minimised by maintaining continuous commit coverage with
strong destructive interference, and maximised by committing with strong constructive
interference or by leaving the zone uncovered.

**Interpretation depends on interference direction:**

- **Destructive (tank/mitigation context)**: Wave Accumulation represents incoming Pressure.
  The goal is to keep Pressure low via cancellation commits. Pressure that exceeds a
  threshold manifests as damage or debuff. Arcane Pool is spent to actively relieve
  accumulated Pressure.

- **Constructive (DPS/heal context)**: Wave Accumulation represents accumulated Potential.
  The goal is to maximise Potential via amplification commits. Arcane Pool is spent to
  convert accumulated Potential into output — more Potential yields a larger effect on the
  same Pool expenditure.

Both orientations use the same accumulation mechanic and the same pool-spending structure.
The distinction is in the strategic target: reduce accumulation vs. build accumulation.

Wave Accumulation decays passively at a slow rate when not being actively added to.

#### Disruption
An incoming hit adds a high-frequency spatial component to the **standing wave**, representing
destabilisation of the player's own resonance state:

```
sw_disrupted(x, t) = A * PAMP * (sin(K1*x + phi) + da * sin(dk*x + dp)) * cos(omega_sw * t)
```

where `da` is disruption amplitude (decays over ~3s), and `dk`, `dp` are random spatial
frequency and phase per hit. Multiple hits accumulate `da` additively up to a cap.

Disruption corrupts the standing wave's spatial profile, degrading interference quality and
making good commit windows harder to identify. Committed segments snapshot the disruption
state at `t_commit` — a segment committed under heavy disruption permanently shows the
degraded result and contributes degraded area to Wave Accumulation.

#### Pool and accumulation interaction
Arcane Pool is the resource spent to act on Wave Accumulation. In the destructive context,
Pool expenditure relieves Pressure — more Pool spent per action relieves more. In the
constructive context, Pool expenditure converts Potential into output — more accumulated
Potential yields a larger effect on the same expenditure. Pool cost and scaling are
action-specific design parameters.

---

### Mechanic interaction summary (Arcane)

Bar fill → Arcane Pool → resource spent to act on Wave Accumulation.
Wave interference → Wave Accumulation → the state that Pool expenditure acts upon.
Destructive commits reduce Accumulation directly; constructive commits build it.
Uncovered zone time adds raw wave area to Accumulation continuously.
Disruption → drains bar fill (reduces Pool income) AND corrupts standing wave (degrades
commit quality) → dual pressure on both mechanics simultaneously.
The central tension: bar disruption and standing wave disruption operate independently,
meaning a heavy hit phase can simultaneously drain Pool and degrade interference quality,
leaving the player with neither the resource nor the wave conditions to respond effectively.

---

## Class: Nature

### Mechanics
- **Value Lock** — player-controlled frequency selection via hold-and-release bar fill
- **Heartbeat** — delivers per-pulse output value at the locked frequency

---

### Value Lock

#### World axis
A horizontal bar that the player fills via hold-and-release input. The bar has no autonomous
fill motion and no pre-defined zone. Its position is entirely determined by player input —
the bar fills while held and locks in place on release.

#### Player input
Hold-and-release. Pressing and holding fills the bar at a fixed rate. Releasing locks the
current fill value. The locked value maps linearly to a target heartbeat frequency across
the full bar range:

```
target_freq = freq_min + locked_value * (freq_max - freq_min)
```

Any value in [0, 1] is valid. There is no rejected range. The player's choice of where to
release is entirely discretionary — higher values target higher frequencies, lower values
target lower frequencies. The full strategic space of frequencies is available on every lock.

The locked value from the **previous** lock is the implicit target for the next lock.
Releasing within `±delta` of the previous locked value constitutes an **entrainment match**
and increments the Entrainment streak. Deliberately releasing at a different value is a
valid strategic choice (e.g. to modulate frequency around an enemy mechanic) but resets the
streak to zero.

#### Entrainment
Entrainment is an integer streak counter. It increments on each matching re-lock and resets
to zero on a non-matching re-lock. Entrainment is one of two multipliers applied to Pulse
Charge delivery value (see below):

```
effective_charge = pulse_charge * freq_multiplier(current_freq) * entrainment_multiplier(streak)
```

Both multiplier functions are monotonically defined (exact curves TBD at balance time).

#### Target frequency decay
After a lock event, the target frequency decays toward zero at a **fixed rate** independent
of the locked frequency value:

```
d(target_freq)/dt = -k_decay   [constant, not proportional to target_freq]
```

This implements a **stepped decay**: the target frequency holds near its locked value for
most of the decay window, then falls sharply in the final ~20%. The decay window duration is
therefore the same regardless of whether the player locked high or low. High-frequency
operation is already more demanding because the heartbeat commits arrive more frequently and
the primary spike window is narrower; requiring additionally more frequent re-locks at high
frequency would compound the difficulty disproportionately.

---

### Heartbeat

#### World axis — frequency
The heartbeat circle grows and shrinks continuously. Its current frequency interpolates
toward the target:

```
d(freq)/dt = (target_freq - current_freq) / tau_interp
```

where `tau_interp` is the interpolation time constant (suggested ~0.8s). Frequency changes
after a re-lock ramp gradually rather than snapping. This means the player can anticipate
where the frequency is heading and prepare commit timing before it fully arrives.

#### World axis — pulse shape
The circle radius follows a heartbeat envelope per period:

```
t_n = t mod (1 / current_freq)   [normalised phase within current beat]

r(t_n):
  0.00 – 0.07  : rise to peak (primary systole)
  0.07 – 0.18  : fall from peak
  0.18 – 0.25  : secondary rise (~40% of primary peak)
  0.25 – 0.36  : fall from secondary peak
  0.36 – 1.00  : diastolic flat (near zero)
```

The primary spike is the high-value commit window. The secondary echo is a lower-value
window. The diastolic flat offers near-zero value. At high frequency, the period compresses
and all windows narrow proportionally — the primary spike becomes very brief and harder to
land.

#### Player input
Single commit input. On commit, the current normalised radius is sampled:

```
pulse_charge = r(t_n)   [range 0–1]
```

**Lockout**: one commit is permitted per half heartbeat period, consistent with the arc and
wave interference mechanics. The lockout engages on commit and releases at the next phase
midpoint. This prevents spam during the diastolic flat and enforces a maximum commit rate of
twice per period.

#### Delivered value
Each commit produces a delivered value that is the product of three terms:

```
delivered_value = pulse_charge
                * freq_multiplier(current_freq)
                * entrainment_multiplier(streak)
```

`freq_multiplier` is an inversely scaled function of current frequency — high frequency
produces a smaller per-commit multiplier, low frequency produces a larger one. Combined with
the higher commit rate at high frequency, this creates a genuine throughput tradeoff:

- **High frequency**: many commits per unit time, small multiplier per commit, narrow timing
  windows, high disruption vulnerability. Maximum theoretical throughput is highest here but
  requires near-perfect execution.
- **Low frequency**: few commits per unit time, large multiplier per commit, wide timing
  windows, robust under disruption. Lower throughput ceiling but more consistent output.

The delivered value is the per-pulse contribution to any active output effects.

#### Disruption
An incoming hit injects a transient frequency spike and envelope noise:

```
current_freq += hit_impulse
envelope_noise += hit_noise_amplitude   [decays over ~3s]
```

The tachycardia effect compresses all windows. The primary and secondary peaks become harder
to distinguish at high noise levels. The disruption also shifts the current frequency away
from the player's target, introducing a mismatch that the interpolation must recover from —
this recovery period produces degraded delivered values even after the envelope noise clears.

---

### Mechanic interaction summary (Nature)

Value lock → target frequency → player's chosen operating point on the frequency/reliability
tradeoff.
Target frequency decays at fixed rate → periodic re-lock required to maintain rhythm.
Matching re-lock within delta → Entrainment streak → multiplies delivered value.
Heartbeat → per-commit pulse charge sampled from envelope → delivered value is
pulse_charge × freq_multiplier × entrainment_multiplier.
Delivered value feeds sustained output channels — rhythm maintenance is continuous upkeep,
not a one-time activation.
Disruption → tachycardia spike + envelope noise → degrades pulse charge quality AND shifts
frequency away from target → dual recovery cost.
Deliberate frequency shift via non-matching re-lock → resets Entrainment → strategic cost of
adapting to enemy mechanic.


