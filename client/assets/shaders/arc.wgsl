#import bevy_ui::ui_vertex_output UiVertexOutput

// Arc mechanic shader — half-sine-wave curve with stacked ghost history.
//
// Each arc is a half period of a sine wave: y = cy + depth * sin(θ),
// where θ ∈ [0, π] maps left-to-right across the node.
// θ = 0 and θ = π are the endpoints (apexes, y = cy).
// θ = π/2 is the nadir (deepest point, y = cy + depth).
//
// Stacking: ghost i has its baseline at cy = (i+1) * STACK_OFFSET.
// Because we only shift cy, stacked sine waves are parallel curves with
// perfectly uniform vertical spacing everywhere — no geometric bunching.
//
// Rendering is back-to-front: oldest ghost first, main arc last.

struct ArcParams {
    core:       vec4<f32>,   // x=theta, y=amplitude, z=in_lockout(0/1), w=ghost_count
    ghost_a:    vec4<f32>,   // frozen commit thetas 0-3  (0 = most recent)
    ghost_b:    vec4<f32>,   // frozen commit thetas 4-7
    dimensions: vec4<f32>,   // x=node_w_px, y=node_h_px, z=time_secs, w=tilt (or source_cx_offset in central mode)
    commit:     vec4<f32>,   // x=pulse(1→0 after commit), y=physical theta / source tilt, z=hide_main, w=ghost_y_offset
    extra:      vec4<f32>,   // x=scroll_carry (unfinished scroll from interrupted animations)
}

@group(1) @binding(0) var<uniform> params: ArcParams;

const PI:           f32 = 3.14159265;
const HALF_PI:      f32 = 1.5707963;
// 3/4 of a half-sine, centred on the nadir (θ = π/2).
// Span = 3π/4 rad, so each endpoint sits at sin(π/8) ≈ 0.383 × depth.
const ARC_THETA_MIN:  f32 = 0.39270;  // π/8
const ARC_THETA_MAX:  f32 = 2.74889;  // 7π/8
const ARC_THETA_SPAN: f32 = 2.35619;  // 3π/4

const STACK_OFFSET:      f32 = 22.0;   // vertical px between successive arc baselines
const SIN_ARC_THETA_MIN: f32 = 0.38268; // sin(π/8) — height of arc endpoints above nadir
const N_GHOSTS:     i32 = 8;
const TRACK_W:      f32 = 3.2;    // rail half-width in pixels
const DOT_R:        f32 = 11.0;   // pendulum dot radius
const AURA_R:       f32 = 18.0;   // soft glow radius around the dot

// ── Helpers ───────────────────────────────────────────────────────────────────

fn ghost_theta(i: i32) -> f32 {
    switch i {
        case 0:      { return params.ghost_a.x; }
        case 1:      { return params.ghost_a.y; }
        case 2:      { return params.ghost_a.z; }
        case 3:      { return params.ghost_a.w; }
        case 4:      { return params.ghost_b.x; }
        case 5:      { return params.ghost_b.y; }
        case 6:      { return params.ghost_b.z; }
        default:     { return params.ghost_b.w; }
    }
}

fn blend_over(src: vec4<f32>, dst: vec4<f32>) -> vec4<f32> {
    let a = src.a + dst.a * (1.0 - src.a);
    if a < 0.0001 { return vec4<f32>(0.0); }
    return vec4<f32>((src.rgb * src.a + dst.rgb * dst.a * (1.0 - src.a)) / a, a);
}

fn sdisk(dist: f32, r: f32) -> f32 {
    return smoothstep(r + 1.5, r - 1.5, dist);
}

// Zone colour: green at nadir (θ=π/2), orange mid-range, red at apexes (θ=0,π).
fn zone_color(sine_theta: f32, amplitude: f32) -> vec3<f32> {
    let p = clamp(abs(sine_theta - HALF_PI) / amplitude, 0.0, 1.0);
    if p < 0.22 {
        return mix(vec3<f32>(0.18, 1.00, 0.45), vec3<f32>(0.28, 0.90, 0.28), p / 0.22);
    } else if p < 0.72 {
        let t = (p - 0.22) / 0.50;
        return mix(vec3<f32>(0.28, 0.90, 0.28), vec3<f32>(0.98, 0.52, 0.08), t);
    } else {
        let t = (p - 0.72) / 0.28;
        return mix(vec3<f32>(0.98, 0.52, 0.08), vec3<f32>(1.00, 0.14, 0.06), t);
    }
}

// ── Single sine-arc layer ─────────────────────────────────────────────────────
//
// cx, cy   = horizontal centre and baseline y of this arc
// half_w   = half the arc's horizontal extent (arc spans cx±half_w)
// depth    = sine amplitude in pixels (nadir is depth px below cy)
// theta    = dot position in [0, π]
// lockout  = whether to animate the dot
// time     = elapsed seconds
fn sine_arc_layer(
    px: f32, py: f32,
    cx: f32, cy: f32,
    half_w: f32, depth: f32,
    theta: f32, amplitude: f32,
    lockout: bool, time: f32,
    commit_pulse: f32, commit_theta: f32,
    is_main: bool,
    tilt: f32,
) -> vec4<f32> {
    // Inverse-rotate screen pixel into local arc space around (cx, cy) so the
    // arc geometry is always evaluated unmodified and the tilt is a true rotation.
    let cos_t = cos(tilt);
    let sin_t = sin(tilt);
    let dpx = px - cx;
    let dpy = py - cy;
    let rpx = cx + cos_t * dpx + sin_t * dpy;
    let rpy = cy - sin_t * dpx + cos_t * dpy;

    let x_left  = cx - half_w;
    let x_right = cx + half_w;

    // Fast reject outside horizontal band (with a small buffer for endpoint caps).
    if rpx < x_left - DOT_R || rpx > x_right + DOT_R { return vec4<f32>(0.0); }

    var out = vec4<f32>(0.0);

    // ── Rail ─────────────────────────────────────────────────────────────────
    // Only process pixels that are horizontally inside the arc.
    if rpx >= x_left && rpx <= x_right {
        // t ∈ [0, 1] — normalised position along the arc.
        let t = (rpx - x_left) / (2.0 * half_w);

        // Visual angle drives the curve SHAPE (3/4-sine section).
        let visual_theta = ARC_THETA_MIN + t * ARC_THETA_SPAN;
        let arc_y        = cy + depth * sin(visual_theta);

        // Physical angle drives COLOUR and MEANING (full [0, π] range).
        // Endpoints have physical_theta = 0 or π → proximity = 1 → full red.
        let physical_theta = t * PI;

        // Perpendicular distance. Derivative uses the visual angle.
        let slope     = depth * ARC_THETA_SPAN / (2.0 * half_w) * cos(visual_theta);
        let perp_dist = abs(rpy - arc_y) / sqrt(1.0 + slope * slope);

        let rail_w = 1.0 - smoothstep(TRACK_W, TRACK_W + 1.5, perp_dist);
        if rail_w > 0.001 {
            let zc = zone_color(physical_theta, amplitude);
            // Bevel: inner face (above arc, rpy < arc_y) bright; outer face darker.
            let t_bev = saturate((rpy - (arc_y - TRACK_W)) / (TRACK_W * 2.0));
            var bev   = mix(1.65, 0.52, t_bev);

            // Commit ripple on the main arc only.
            if is_main && commit_pulse > 0.001 {
                let commit_dist  = abs(physical_theta - commit_theta) / PI;
                let ripple_front = 1.0 - commit_pulse;
                let ripple       = max(0.0, 1.0 - abs(commit_dist - ripple_front) * 9.0)
                                   * commit_pulse;
                bev *= 1.0 + ripple * 0.9;
            }

            let col  = clamp(zc * bev, vec3<f32>(0.0), vec3<f32>(2.0));
            let glow = (1.0 - smoothstep(TRACK_W + 0.5, TRACK_W + 6.0, perp_dist)) * 0.14;
            out = vec4<f32>(col, rail_w * 0.97 + glow);
        }
    }

    // ── Rounded endpoint caps ─────────────────────────────────────────────────
    // Caps sit at the arc endpoints. Physical theta at endpoint = 0 or π → full red.
    let endpoint_y = cy + depth * sin(ARC_THETA_MIN);
    let apex_col   = clamp(zone_color(0.0, amplitude) * 1.15, vec3<f32>(0.0), vec3<f32>(1.5));
    let cap_r = TRACK_W * 1.05;

    let dl = distance(vec2<f32>(rpx, rpy), vec2<f32>(x_left,  endpoint_y));
    let dr = distance(vec2<f32>(rpx, rpy), vec2<f32>(x_right, endpoint_y));

    let cw_l = sdisk(dl, cap_r);
    let cw_r = sdisk(dr, cap_r);
    if cw_l > 0.001 { out = blend_over(vec4<f32>(apex_col, cw_l * 0.97), out); }
    if cw_r > 0.001 { out = blend_over(vec4<f32>(apex_col, cw_r * 0.97), out); }

    // ── Pendulum dot ──────────────────────────────────────────────────────────
    // theta ∈ [0, π] maps linearly to x ∈ [x_left, x_right] (no clamping).
    // The dot's y follows the visual (3/4-sine) curve shape at that t.
    let dot_t  = theta / PI;
    let dot_x  = x_left + dot_t * (2.0 * half_w);
    let dot_y  = cy + depth * sin(ARC_THETA_MIN + dot_t * ARC_THETA_SPAN);
    let dd    = distance(vec2<f32>(rpx, rpy), vec2<f32>(dot_x, dot_y));

    // Size pop on commit — main arc only; ghosts stay a fixed size.
    let size_pop = select(0.0, commit_pulse, is_main);
    let effective_dot_r  = DOT_R  * (1.0 + size_pop * 0.45);
    let effective_aura_r = AURA_R * (1.0 + size_pop * 0.60);

    let aura_w = sdisk(dd, effective_aura_r) * 0.20;
    let dw     = sdisk(dd, effective_dot_r);

    if dw > 0.001 || aura_w > 0.001 {
        var dc: vec3<f32>;
        if commit_pulse > 0.001 {
            // Dot takes on the zone colour of the commit position, fading to white.
            let commit_col = zone_color(commit_theta, amplitude);
            dc = mix(vec3<f32>(1.0, 1.0, 1.0), commit_col, commit_pulse);
        } else {
            dc = vec3<f32>(1.0, 1.0, 1.0);
        }
        let core_t = 1.0 - saturate(dd / max(effective_dot_r, 0.001));
        dc *= 0.72 + 0.28 * core_t;

        let combined = max(dw * 0.97, aura_w);
        out = blend_over(vec4<f32>(dc, combined), out);
    }

    return out;
}

// ── Fragment entry ────────────────────────────────────────────────────────────

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let node_w = params.dimensions.x;
    let node_h = params.dimensions.y;
    let time   = params.dimensions.z;

    // Guard against uninitialized material (node_w=0 maps all pixels to position 0
    // which lands inside the dot radius, producing a solid white fill).
    if node_w < 1.0 { return vec4<f32>(0.0); }

    let theta   = params.core.x;
    let amp     = params.core.y;
    let lockout = params.core.z > 0.5;
    let ghost_n = i32(params.core.w);
    let tilt    = params.dimensions.w;

    let commit_pulse = params.commit.x;
    let commit_theta = params.commit.y;

    let px = in.uv.x * node_w;
    let py = in.uv.y * node_h;
    let cx = node_w * 0.5;

    // Sine wave geometry (derived from node width for consistent proportions).
    // half_w = 0.452 * node_w  →  arc spans 90% of the node width.
    // depth  = 0.310 * node_w  →  nadir sits ~130 px below baseline at 420 px wide.
    let half_w = node_w * 0.452;
    let depth  = node_w * 0.310;

    // Shift the main arc downward so rotated endpoints stay within the node bounds.
    // For tilt=0 this is 0; for large tilts it compensates the upward endpoint swing.
    let tilt_sin = sin(tilt);
    let tilt_cos = cos(tilt);
    let y_offset = max(0.0, half_w * abs(tilt_sin) - depth * SIN_ARC_THETA_MIN * tilt_cos);

    var color = vec4<f32>(0.0);

    // Render ghosts back-to-front (oldest = most transparent, rendered first).
    for (var i: i32 = N_GHOSTS - 1; i >= 0; i = i - 1) {
        if i >= ghost_n { continue; }

        let gt      = ghost_theta(i);
        // scroll_t runs 1→0 at twice the speed of commit_pulse, finishing halfway through.
        let scroll_t = saturate(commit_pulse * 2.0 - 1.0);
        // scroll_carry folds in any unfinished scroll from a previously interrupted animation.
        // Total travel = (1 + scroll_carry) slots, so ghosts continue from wherever they were.
        let ghost_cy = (f32(i + 1) - (1.0 + params.extra.x) * scroll_t) * STACK_OFFSET + params.commit.w;
        // Opacity: most-recent ghost 0 = 0.65, oldest ghost 7 ≈ 0.16.
        let opacity = 0.65 - f32(i) * 0.07;

        // Ghost: is_main=false suppresses size pop/ripple; commit_pulse=1 keeps zone colour on dot.
        // For the most recent ghost (i=0) in central mode, animate from the source arc into place.
        var g: vec4<f32>;
        if i == 0 && params.commit.z > 0.5 && commit_pulse > 0.001 {
            let src_tilt      = params.commit.y;
            let src_cx_offset = params.dimensions.w; // repurposed from tilt (always 0 for ghosts)
            let anim_tilt = src_tilt * scroll_t;
            let anim_cx   = cx + src_cx_offset * scroll_t;
            let src_y_off = max(0.0, half_w * abs(sin(src_tilt))
                                   - depth * SIN_ARC_THETA_MIN * cos(src_tilt));
            let anim_cy   = mix(ghost_cy, src_y_off, scroll_t);
            g = sine_arc_layer(px, py, anim_cx, anim_cy, half_w, depth, gt, amp, false, 0.0, 1.0, gt, false, anim_tilt);
        } else {
            g = sine_arc_layer(px, py, cx, ghost_cy, half_w, depth, gt, amp, false, 0.0, 1.0, gt, false, 0.0);
        }
        g = vec4<f32>(g.rgb, g.a * opacity);
        color = blend_over(g, color);
    }

    // Main arc rendered last (on top). Skipped when commit.z > 0.5 (ghost-only mode).
    if params.commit.z < 0.5 {
        let main_c = sine_arc_layer(px, py, cx, y_offset, half_w, depth, theta, amp, lockout, time, commit_pulse, commit_theta, true, tilt);
        color = blend_over(main_c, color);
    }

    return color;
}
