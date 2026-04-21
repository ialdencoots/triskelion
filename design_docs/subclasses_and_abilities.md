# Subclasses & Ability Set

## Structure Overview

Each of the three classes — Physical, Arcane, Nature — has three subclasses corresponding
to the tank, healer, and DPS roles. Role output is delivered through **stance activation**:
entering a stance starts the class minigame, which runs autonomously and accepts player
inputs until the stance is exited. Outside of stances, the player has no output. All
non-stance abilities are standalone tools independent of minigame state.

Each class has a distinct **CC type**, **mobility option**, and shares access to **taunt**
and **interrupt** as universal combat tools.

---

## Class: Physical

**Minigame mechanics**: Arc (continuous performance signal) + Cube or Grid (bonus
routing, role-dependent). Arc commit quality drives streak. Tank and Healer use a cube
overlay triggered on streak cap, where timed rotations collect bonuses from tier-gated
faces. The Duelist uses a traversal grid: streak accumulates as a step budget and
auto-activates the grid on break or cap. Disruption applies a counter-directional impulse
to the arc dot, degrading coherence and streak.

---

### Bulwark — Tank

A war-hardened fighter who treats incoming force as data. Where others avoid disruption,
the Bulwark has developed the muscle memory to re-find the nadir even through a disrupted
arc. The ghost arc history reads as a ledger of absorbed punishment: the straighter the
column despite disruption, the deeper the discipline.

**Stance: Iron Stance**
Aggro generation and damage mitigation mode. Cube face bonuses skew toward stun-on-hit,
aggro amplification, and cooldown reduction. At high streak quality the damage reflection
modifier unlocks at premium tier — absorbed force is converted directly into output,
meaning sustained clean play makes disruption a two-way exchange.

---

### Intercessor — Healer

A chi practitioner who interposes between their ward and incoming harm. Their primary output
is damage prevention via deflection and aggro manipulation; direct restoration is available
but gated behind sustained discipline. The ghost arc history is a record of maintained
vigilance — a straight nadir column represents unbroken coverage.

**Stance: Flowing Guard**
Interception and chi management mode. Cube face bonuses are organized around threat
response rather than personal output:

- **Low streak quality** faces: partial deflection (split mitigation between Intercessor
  and ward), minor stun-on-hit to interrupt follow-up attacks.
- **High streak quality** faces: full nullification (cancel an incoming attack entirely),
  chi burst heal (direct restoration delivered to the ward — the only healing in the kit,
  delivered as a held charge rather than an immediate resolution), residual chi pulse
  (minor area mitigation on nearby allies post-deflection).

The chi burst heal being quality-gated is the defining constraint of the subclass. It is a
reward for sustained discipline, not an emergency button. An Intercessor who has lost streak
under disruption cannot heal on demand — they must restore chi coherence first, which
reframes the role: prevent damage first, restore only when performing well enough that it is
almost unnecessary. A disrupted arc means a failed interception: the ward took a hit, the
Intercessor's chi is fractured, and both protection and healing capacity are simultaneously
compromised.

---

### Duelist — DPS

The arc reduced to a philosophy of perfect violence. Every other variable is noise. The
ghost arc history is a personal record — a clean nadir column is not a UI element but a
proof of mastery. Disruptions are existential to the Duelist in a way they are not for the
Bulwark: the entire damage profile depends on streak.

**Stance: Edge Form**
Sustained single-target damage mode. Streak accumulates as a step budget for the traversal
grid, which auto-activates on streak break or cap. A larger streak at activation means more
steps — more bonus nodes reachable and a richer routing problem. A cold or disrupted
Duelist activates a small grid with degraded bonus magnitudes; a Duelist in full flow cashes
in a large, high-quality run. Bonus type distribution is shaped by skill tree choices.

---

### Physical Abilities

| Ability | Type | Description |
|---|---|---|
| Iron Stance / Flowing Guard / Edge Form | Stance | Role output mode; activates Arc + Cube (Tank/Healer) or Arc + Grid (Duelist) |
| **Lunge** | Mobility | Committed directional burst. Linear, self-propelled, with brief projectile invulnerability mid-travel. Aggressive repositioning — closes distance or crosses gaps. No intangibility on arrival; you are responsible for landing position. |
| **Stun** | CC | Removes enemy agency entirely for a short duration. No movement, no action. |
| Taunt | Utility | Forces enemy aggro onto caster for a duration. The Intercessor variant is targeted — aggro is redirected from a specified ally onto a specified target (typically the tank) rather than unconditionally onto the caster. |
| Interrupt | Reactive | Cancels a specific enemy cast or channeled action at the moment of execution. |

---

## Class: Arcane

**Minigame mechanics**: Bar Fill (generates Arcane Pool) + Wave Interference (accumulates
Wave Pressure or Wave Potential depending on subclass orientation). Disruption simultaneously
drains the bar and corrupts the standing wave — the dual-pressure failure mode that defines
the class's risk profile.

---

### Aegis — Tank

Defense as interference. The Aegis projects a personal standing wave that destructively
interferes with incoming energetic force. The bar sustains that projection; the wave
interference is its active expression. Taking heavy hits destabilizes both simultaneously —
pool drains and the standing wave degrades at the same moment, representing the shield
coming apart while power to rebuild it is also running out. Destructive interference commits
are the primary tool; a player who panics into constructive commits is actively worsening
their own situation.

**Stance: Null Field**
Damage mitigation and threat anchoring mode. Operates in the destructive interference
orientation — Wave Accumulation represents incoming Pressure. Pool expenditure directly
relieves Pressure. The goal is continuous cancellation coverage with high commit quality; an
uncovered zone or a constructively-committed segment adds Pressure directly.

---

### Conduit — Healer

A living relay for borrowed resonance. The Conduit does not generate healing energy; they
find it in the wave structure of the world and redirect it. Bar fill represents drawing
ambient resonance into the pool; wave interference commits shape that resonance into
coherent healing output. Constructive interference is the goal — amplify rather than cancel.
Bonus markers on the bar represent recognized resonance harmonics, incentivizing non-maximal
commits that align with favorable wave states.

**Stance: Resonant Flow**
Healing and pool management mode. Operates in the constructive interference orientation —
Wave Accumulation represents Potential. Pool expenditure converts accumulated Potential into
healing output; more Potential yields a larger effect on the same expenditure. The central
tension is opportunity management: a Conduit with high Potential and high Pool has enormous
burst heal capacity; one whose bar has been repeatedly drained while committing destructively
is in genuine crisis on both axes simultaneously.

---

### Arcanist — DPS

The pool is a weapon. The super-linear scaling above 100% pool is the entire game —
the Arcanist is always deciding whether now is the moment to detonate. Wave Potential
is the output amplifier; pool level is the charge. Passive pool decay is a clock; disruption
drain is a targeted attack on the detonation strategy. The throughput profile is
feast-or-famine: an Arcanist with high Potential and high Pool is a detonation event; one
that has been disrupted into low pool with degraded wave conditions produces dramatically
low output.

**Stance: Overcharge**
Burst damage mode. Operates in the constructive interference orientation — Wave Accumulation
represents Potential. Pool detonation events scale with accumulated Potential; maintaining
high pool via disciplined bar management and avoiding disruption drain are the primary skill
axes. Committed segments snapshot disruption state at commit time — segments committed under
heavy disruption permanently show degraded results and contribute degraded Potential.

---

### Arcane Abilities

| Ability | Type | Description |
|---|---|---|
| Null Field / Resonant Flow / Overcharge | Stance | Role output mode; activates Bar Fill + Wave Interference minigame |

| **Phase Step** | Mobility | Short-range teleport with a brief intangibility window bracketing arrival. Discontinuous — no travel path to intercept. Requires knowing the destination; a panicked step into a bad position is a wasted cooldown. |
| **Displacement** | CC | Forced repositioning via wave pressure event. Pushes or pulls the target to a specified location. Does not remove agency — the enemy can act immediately from their new position. Available as push or pull variant. |
| Taunt | Utility | Forces enemy aggro onto caster for a duration. |
| Interrupt | Reactive | Cancels a specific enemy cast or channeled action at the moment of execution. |

---

## Class: Nature

**Minigame mechanics**: Value Lock (hold-and-release bar fill sets target heartbeat
frequency) + Heartbeat (delivers per-pulse output at the locked frequency, scaled by
freq_multiplier and entrainment streak). Disruption injects tachycardia and envelope noise,
compressing timing windows and introducing a frequency mismatch the interpolation must
recover from.

---

### Wardbark — Tank

Slowness as armor. The Wardbark operates predominantly at the low end of the frequency
range — large per-commit multiplier, wide timing windows, strong disruption robustness.
Entrainment streak is the heartbeat of something ancient and patient: matching your own
previous lock again and again is meditative self-reinforcement. The primary vulnerability is
disruption-induced tachycardia — a forced frequency spike away from the low-frequency
operating zone is genuinely threatening, and the Wardbark must decide whether to re-lock to
the new higher frequency to preserve streak, or re-lock back to their preferred zone and eat
the reset.

**Stance: Deep Root**
Mitigation and taunt output mode. Delivered value feeds damage absorption and threat
generation channels. Low-frequency operation is the natural state: fewer commits per unit
time, large multiplier per commit, resilient under disruption. Entrainment streak amplifies
all mitigation output — a Wardbark who maintains rhythm under pressure is substantially
harder to move than one who has been knocked off their frequency.

---

### Mender — Healer

The heartbeat as medicine. The Mender's delivered value directly maps to healing output per
pulse — the heartbeat is the healing, and maintaining a steady entrained rhythm is the act
of care. Mid-frequency operation is the natural home: high enough commit rate for continuous
coverage, low enough that windows are manageable and disruptions don't collapse the system.
The frequency/reliability tradeoff maps directly onto reactive versus proactive healing
identity: locking lower before a burst damage phase accepts reduced throughput for
resilience; a stable phase rewards pushing higher. Breaking rhythm to adapt to an enemy
mechanic carries a real entrainment cost that must be consciously weighed.

**Stance: Pulse**
Sustained healing mode. Delivered value per commit feeds continuous healing on a designated
target or area. Entrainment streak provides the sustained multiplier — consistent rhythm is
the difference between adequate and strong throughput. Frequency selection is the primary
tactical decision, made at every re-lock.

---

### Thornweave — DPS

High-frequency toxicity. The Thornweave operates at the upper end of the frequency range,
accepting narrow commit windows and disruption vulnerability in exchange for raw commit
rate. The output is relentless and attritive rather than powerful — small multipliers, many
commits, damage-over-time as the delivery mechanism. The secondary echo of the heartbeat
envelope matters more here than for other subclasses: at high frequency, landing the primary
spike consistently is difficult enough that the secondary window becomes a meaningful
fallback. Maintaining entrainment streak at high frequency while recovering from disruption
frequency spikes is the defining skill challenge. A Thornweave who sustains streak at high
frequency has extraordinary throughput; one who repeatedly breaks entrainment produces
mediocre, inconsistent output.

**Stance: Overgrowth**
Sustained damage-over-time mode. Delivered value per commit stacks and extends active DoT
effects on target. High-frequency operation maximises stack generation rate but demands
near-perfect entrainment to reach ceiling throughput. The floor under disruption or poor
entrainment is punishing relative to the ceiling.

---

### Nature Abilities

| Ability | Type | Description |
|---|---|---|
| Deep Root / Pulse / Overgrowth | Stance | Role output mode; activates Value Lock + Heartbeat minigame |
| **Burrow** | Mobility | Player submerges, becoming untargetable and undetectable. Can move freely underground for a duration or distance limit, then surfaces voluntarily at a chosen location within range. Suspends rather than transfers aggro — the player exits the threat model entirely for the duration. Surfacing has a brief visible tell. |
| **Root** | CC | Locks enemy position; movement denied but action permitted. The enemy retains full agency to attack, cast, and act in place. Solves a positioning problem, not a threat problem. |
| Taunt | Utility | Forces enemy aggro onto caster for a duration. |
| Interrupt | Reactive | Cancels a specific enemy cast or channeled action at the moment of execution. |

---

## Universal Abilities (All Classes)

| Ability | Description |
|---|---|
| **Taunt** | Forces enemy aggro onto the caster for a duration. Primary tool for tanks to anchor threat; available to all classes as a situational tool. |
| **Interrupt** | Cancels a specific telegraphed enemy action — cast, channel, or triggered ability — at the moment of execution. Reactive, requires reading enemy behavior. Enables encounter design around interruptible threats. |

---

## Summary Table

| Class | Tank | Healer | DPS | Mobility | CC |
|---|---|---|---|---|---|
| Physical | Bulwark | Intercessor | Duelist | Lunge | Stun |
| Arcane | Aegis | Conduit | Arcanist | Phase Step | Displacement |
| Nature | Wardbark | Mender | Thornweave | Burrow | Root |
