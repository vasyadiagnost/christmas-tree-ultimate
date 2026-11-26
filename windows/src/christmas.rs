//! The Christmas scene uses a separate transparent surface, independent of the weather.

use std::f32::consts::{FRAC_PI_2, TAU};

use snowhush_core::christmas::{HolidaySettings, TransportSprite};
use snowhush_core::geometry::V2;

/// Positions and sizes are DIPs in the complete monitor, with Y increasing upward.
/// The host translates transport anchors from the snow's work area before drawing.
pub struct Scene {
    pub size: [f32; 2],
    pub settings: HolidaySettings,
    pub selected: bool,
    pub transports: Vec<TransportSprite>,
    pub placing: bool,
}

pub const TREE_ASPECT: f32 = 0.75;

/// Bottom-left corner, width and height. X/Y are fractions of the available space,
/// so moving or resizing a monitor never leaves the tree beyond its edges.
pub fn tree_rect(size: [f32; 2], settings: &HolidaySettings) -> [f32; 4] {
    let height = (settings.tree_height as f32).min((size[1] - 12.0).max(1.0)).min((size[0] - 12.0).max(1.0) / TREE_ASPECT);
    let width = height * TREE_ASPECT;
    let left = (size[0] - width).max(0.0) * settings.tree_x.min(1000) as f32 / 1000.0;
    let bottom = (size[1] - height).max(0.0) * settings.tree_y.min(1000) as f32 / 1000.0;
    [left, bottom, width, height]
}

pub(crate) const TREE: &[u8] = include_bytes!("../assets/christmas/tree.png");
pub(crate) const TRACTOR: &[u8] = include_bytes!("../assets/christmas/tractor.png");
pub(crate) const SANTA: &[u8] = include_bytes!("../assets/christmas/santa-body.png");
pub(crate) const DEER_THIGH: &[u8] = include_bytes!("../assets/christmas/deer-thigh.png");
pub(crate) const DEER_SHANK: &[u8] = include_bytes!("../assets/christmas/deer-shank.png");
pub(crate) const DEER_HOOF: &[u8] = include_bytes!("../assets/christmas/deer-hoof.png");

// The supplied 30fps reference repeats after exactly 24 frames (.8s). Core's
// unmultiplied phase gives 1.253Hz at the unchanged 175 DIP/s travel speed.
// These eight keys are frames 001,004,007,010,013,016,019,022, normalized only
// when sampled. Source X is right, Y down; the reference animal faces LEFT.
// Visible distal joints are measured to about 3-5px; occluded proximal roots
// are inferred from the silhouette, not claimed as an anatomical reconstruction.
#[derive(Clone, Copy)]
struct ReferenceKey {
    fore: [[f32; 2]; 4],
    hind: [[f32; 2]; 4],
    axis: [[f32; 2]; 2],
}

// Exact per-frame floor clearance from the 24 reference masks (green threshold
// 65). Keeping every frame preserves the short forefoot landing at frames17-22.
const CLEARANCE: [f32; 24] = [0., 0., 10., 34., 61., 85., 107., 126., 142., 154., 146., 133., 116., 92., 61., 28., 4., 0., 1., 2., 2., 0., 7., 2.];

const REFERENCE: [ReferenceKey; 8] = [
    ReferenceKey {
        fore: [[363., 292.], [373., 340.], [381., 390.], [373., 417.]],
        hind: [[535., 348.], [505., 380.], [544., 407.], [524., 486.]],
        axis: [[371., 304.], [535., 329.]],
    },
    ReferenceKey {
        fore: [[378., 236.], [364., 281.], [406., 326.], [406., 350.]],
        hind: [[543., 309.], [553., 344.], [603., 370.], [640., 451.]],
        axis: [[377., 248.], [548., 302.]],
    },
    ReferenceKey {
        fore: [[356., 212.], [344., 254.], [371., 302.], [367., 326.]],
        hind: [[545., 277.], [558., 306.], [608., 302.], [651., 378.]],
        axis: [[352., 231.], [546., 271.]],
    },
    ReferenceKey {
        fore: [[338., 220.], [331., 265.], [351., 310.], [345., 334.]],
        hind: [[545., 259.], [558., 286.], [594., 261.], [648., 331.]],
        axis: [[338., 240.], [548., 259.]],
    },
    ReferenceKey {
        fore: [[329., 265.], [314., 302.], [333., 343.], [330., 368.]],
        hind: [[544., 260.], [562., 292.], [584., 253.], [641., 321.]],
        axis: [[326., 278.], [545., 260.]],
    },
    ReferenceKey {
        fore: [[347., 310.], [328., 369.], [307., 398.], [291., 460.]],
        hind: [[543., 293.], [556., 321.], [590., 291.], [616., 376.]],
        axis: [[348., 329.], [548., 282.]],
    },
    ReferenceKey {
        fore: [[369., 329.], [399., 382.], [371., 430.], [345., 486.]],
        hind: [[542., 305.], [546., 332.], [581., 320.], [594., 400.]],
        axis: [[367., 352.], [545., 294.]],
    },
    ReferenceKey {
        fore: [[371., 317.], [386., 374.], [394., 427.], [406., 488.]],
        hind: [[530., 336.], [518., 374.], [552., 388.], [542., 465.]],
        axis: [[374., 337.], [535., 327.]],
    },
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct LegPose {
    pub hip: V2,
    pub knee: V2,
    pub hock: V2,
    pub ankle: V2,
    pub hoof: V2,
    pub hoof_angle: f32,
    pub far: bool,
    pub front: bool,
    pub scale: f32,
    #[cfg(test)]
    pub contact: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PartPlacement {
    pub anchor: V2,
    pub size: [f32; 2],
    pub angle: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BodyPart {
    pub placement: PartPlacement,
    pub uv: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ReinBridge {
    pub from: V2,
    pub to: V2,
    pub width: f32,
    pub tint: [f32; 4],
}

/// Phase zero equals reference frame001; every eighth-turn is a measured pose key.
fn phase_at(phase: f32) -> (usize, f32) {
    let phase = if phase.is_finite() { phase } else { 0.0 };
    let position = phase.rem_euclid(TAU) * (8.0 / TAU);
    let index = (position.floor() as usize).min(7);
    (index, position - index as f32)
}

fn cubic(values: [f32; 4], t: f32) -> f32 {
    let [a, b, c, d] = values;
    0.5 * ((2.0 * b) + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t * t + (-a + 3.0 * b - 3.0 * c + d) * t * t * t)
}

fn sampled(phase: f32, value: impl Fn(ReferenceKey) -> f32) -> f32 {
    let (i, t) = phase_at(phase);
    cubic([value(REFERENCE[(i + 7) % 8]), value(REFERENCE[i]), value(REFERENCE[(i + 1) % 8]), value(REFERENCE[(i + 2) % 8])], t)
}

/// Periodic monotone Hermite interpolation keeps the measured floor contacts
/// flat and nonnegative, with zero velocity at the transition into flight.
fn clearance(phase: f32) -> f32 {
    let phase = if phase.is_finite() { phase } else { 0.0 };
    let position = phase.rem_euclid(TAU) * (24.0 / TAU);
    let i = (position.floor() as usize).min(23);
    let t = position - i as f32;
    let a = CLEARANCE[(i + 23) % 24];
    let b = CLEARANCE[i];
    let c = CLEARANCE[(i + 1) % 24];
    let d = CLEARANCE[(i + 2) % 24];
    let tangent = |x: f32, y: f32| if x * y > 0.0 { 2.0 * x * y / (x + y) } else { 0.0 };
    let m0 = tangent(b - a, c - b);
    let m1 = tangent(c - b, d - c);
    let t2 = t * t;
    let t3 = t2 * t;
    ((2.0 * t3 - 3.0 * t2 + 1.0) * b + (t3 - 2.0 * t2 + t) * m0 + (-2.0 * t3 + 3.0 * t2) * c + (t3 - t2) * m1).max(0.0)
}

fn reference_chain(phase: f32, front: bool) -> [V2; 3] {
    std::array::from_fn(|joint| {
        let get = |key: ReferenceKey| if front { key.fore } else { key.hind };
        V2::new(
            sampled(phase, |key| {
                let p = get(key);
                p[0][0] - p[joint + 1][0]
            }),
            sampled(phase, |key| {
                let p = get(key);
                p[0][1] - p[joint + 1][1]
            }),
        )
    })
}

fn pitch(phase: f32) -> f32 {
    sampled(phase, |key| (key.axis[1][1] - key.axis[0][1]).atan2(key.axis[1][0] - key.axis[0][0]))
}

fn rotate(point: V2, angle: f32) -> V2 {
    let (sn, cs) = angle.sin_cos();
    V2::new(point.x * cs - point.y * sn, point.x * sn + point.y * cs)
}

fn scale(value: f32, reference: f32) -> f32 {
    if value.is_finite() { value.max(1.0) / reference } else { 1.0 }
}

fn transform(sprite: &TransportSprite, point: V2) -> V2 {
    let direction = if sprite.direction < 0.0 { -1.0 } else { 1.0 };
    let angle = if sprite.angle.is_finite() { sprite.angle } else { 0.0 };
    let (sn, cs) = angle.sin_cos();
    let x = point.x * direction;
    sprite.anchor + V2::new(x * cs - point.y * sn, x * sn + point.y * cs)
}

pub(crate) fn santa_body_anchor(sprite: &TransportSprite) -> V2 {
    // Preserve the old, very small sleigh motion; deer jump independently.
    transform(sprite, V2::new(0.0, (sprite.phase * 4.0).sin() * 0.65 * scale(sprite.height, 176.0)))
}

/// All joints are world coordinates. Far legs go behind the painted body;
/// near legs go in front. Both reindeer retain their original painted bodies.
pub(crate) fn santa_legs(sprite: &TransportSprite) -> [LegPose; 8] {
    let first = deer_pose(sprite, 0).0;
    let second = deer_pose(sprite, 1).0;
    std::array::from_fn(|i| if i < 4 { first[i] } else { second[i - 4] })
}

fn deer_phase(sprite: &TransportSprite, deer: usize) -> f32 {
    let phase = if sprite.phase.is_finite() { sprite.phase } else { 0.0 };
    phase - deer as f32 * (TAU * 2.0 / 24.0)
}

fn deer_pivot(sprite: &TransportSprite, deer: usize) -> V2 {
    V2::new(((if deer == 0 { 0.548 } else { 0.8415 }) - 0.5) * sprite.width, 0.40 * sprite.height)
}

fn deer_pose(sprite: &TransportSprite, deer: usize) -> ([LegPose; 4], BodyPart) {
    let sx = scale(sprite.width, 440.0);
    let sy = scale(sprite.height, 176.0);
    let phase = deer_phase(sprite, deer);
    let body_pitch = pitch(phase);
    let pivot = deer_pivot(sprite, deer);
    let angle = if sprite.angle.is_finite() { sprite.angle } else { 0.0 };
    let direction = if sprite.direction < 0.0 { -1.0 } else { 1.0 };
    let mut legs = std::array::from_fn(|index| {
        let front = index % 2 == 0;
        let far = index >= 2;
        // Pair delay is one reference frame, not a diagonal half-stride offset.
        let gait = phase - if far { TAU / 24.0 } else { 0.0 };
        let chain = reference_chain(gait, front);
        let root_x = match (deer, front) {
            (0, false) => 0.483,
            (0, true) => 0.613,
            (1, false) => 0.773,
            _ => 0.910,
        };
        let root_y = if front { 0.622 } else { 0.642 };
        let x = (root_x - 0.5) * 440.0 * sx + if far { -1.0 * sx } else { 0.0 };
        let hip = pivot + rotate(V2::new(x, (1.0 - root_y) * 176.0 * sy) - pivot, body_pitch);
        let ratio = if front { 0.36 } else { 0.38 };
        let joint = |i: usize| hip + V2::new(chain[i].x * sx * ratio, chain[i].y * sy * ratio);
        let knee = joint(0);
        let hock = joint(1);
        let hoof = joint(2);
        #[cfg(test)]
        let contact = {
            let t = gait.rem_euclid(TAU) / TAU;
            if front { (16.0 / 24.0..=22.0 / 24.0).contains(&t) } else { !(2.0 / 24.0..23.0 / 24.0).contains(&t) }
        };
        let distal = chain[2] - chain[1];
        let air = (clearance(gait) / 20.0).clamp(0.0, 1.0);
        let hoof_pitch = distal.x.atan2(-distal.y).clamp(-0.8, 0.8) * 0.45 * air;
        let ankle = hoof + rotate(V2::new(0.0, 5.4 * sy), hoof_pitch);
        LegPose {
            hip,
            knee,
            hock,
            ankle,
            hoof,
            hoof_angle: angle + direction * hoof_pitch,
            far,
            front,
            scale: sy,
            #[cfg(test)]
            contact,
        }
    });
    let minimum = legs.iter().map(|leg| leg.hoof.y).fold(f32::INFINITY, f32::min);
    // A smooth minimum removes the velocity corner when support changes between
    // near/far feet. Its maximum floor bias is below half a DIP at normal scale.
    let smoothing = 0.35 * sy;
    let lowest = minimum - smoothing * legs.iter().map(|leg| ((minimum - leg.hoof.y) / smoothing).exp()).sum::<f32>().ln();
    let lift = clearance(phase) * 0.36 * sy - lowest;
    for leg in &mut legs {
        for point in [&mut leg.hip, &mut leg.knee, &mut leg.hock, &mut leg.ankle, &mut leg.hoof] {
            *point = transform(sprite, *point + V2::new(0.0, lift));
        }
    }
    let (left, right) = if deer == 0 { (0.43, 0.72) } else { (0.72, 1.0) };
    let bottom = V2::new(((left + right) * 0.5 - 0.5) * sprite.width, 0.0);
    let anchor = transform(sprite, pivot + rotate(bottom - pivot, body_pitch) + V2::new(0.0, lift));
    (
        legs,
        BodyPart {
            placement: PartPlacement { anchor, size: [sprite.width * (right - left), sprite.height], angle: angle + direction * body_pitch },
            uv: [left, 0.0, right - left, 1.0],
        },
    )
}

pub(crate) fn santa_body_parts(sprite: &TransportSprite) -> [BodyPart; 3] {
    let anchor = santa_body_anchor(sprite) + rotate(V2::new(-0.285 * sprite.width * sprite.direction, 0.0), sprite.angle);
    [
        BodyPart { placement: PartPlacement { anchor, size: [sprite.width * 0.43, sprite.height], angle: sprite.angle }, uv: [0.0, 0.0, 0.43, 1.0] },
        deer_pose(sprite, 0).1,
        deer_pose(sprite, 1).1,
    ]
}

fn part_point(sprite: &TransportSprite, part: BodyPart, u: f32, v: f32) -> V2 {
    let x = ((u - part.uv[0]) / part.uv[2] - 0.5) * part.placement.size[0] * sprite.direction;
    let y = (1.0 - v) * part.placement.size[1];
    part.placement.anchor + rotate(V2::new(x, y), part.placement.angle)
}

pub(crate) fn santa_reins(sprite: &TransportSprite) -> [ReinBridge; 3] {
    let parts = santa_body_parts(sprite);
    let sy = scale(sprite.height, 176.0);
    std::array::from_fn(|i| {
        let (a, b, u, v, width, tint) = match i {
            0 => (0, 1, 660.0 / 1983.0, 332.0 / 793.0, 3.4, [0.42, 0.24, 0.145, 1.0]),
            1 => (0, 1, 660.0 / 1983.0, 359.5 / 793.0, 3.6, [0.51, 0.27, 0.145, 1.0]),
            _ => (1, 2, 1320.0 / 1983.0, 491.0 / 793.0, 4.4, [0.44, 0.21, 0.12, 1.0]),
        };
        let end = match i {
            0 => (882.0 / 1983.0, 402.0 / 793.0),
            1 => (882.0 / 1983.0, 441.5 / 793.0),
            _ => (1460.0 / 1983.0, 488.0 / 793.0),
        };
        ReinBridge { from: part_point(sprite, parts[a], u, v), to: part_point(sprite, parts[b], end.0, end.1), width: width * sy, tint }
    })
}

/// Places one undeformed painted part between two joints. Rounded ends overlap
/// the adjoining part; the PNG alpha is preserved, rather than erased at runtime.
pub(crate) fn leg_segment(from: V2, to: V2, width: f32, proximal_v: f32, distal_v: f32) -> PartPlacement {
    let delta = from - to;
    let length = delta.length().max(0.0001);
    let axis = delta / length;
    let height = length / (distal_v - proximal_v);
    PartPlacement { anchor: to - axis * ((1.0 - distal_v) * height), size: [width, height], angle: delta.y.atan2(delta.x) - FRAC_PI_2 }
}

pub(crate) fn hoof_placement(leg: &LegPose) -> PartPlacement {
    let size = 14.0 * leg.scale;
    let (sn, cs) = leg.hoof_angle.sin_cos();
    // The generated hoof's sole is at V=.76, rather than the canvas bottom.
    PartPlacement { anchor: leg.hoof - V2::new(-sn, cs) * (size * 0.24), size: [size; 2], angle: leg.hoof_angle }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snowhush_core::christmas::TransportKind;

    fn sprite() -> TransportSprite {
        TransportSprite { kind: TransportKind::Santa, anchor: V2::ZERO, width: 440.0, height: 176.0, direction: 1.0, phase: 0.0, angle: 0.0 }
    }

    #[test]
    fn reference_keys_preserve_the_measured_fore_fold_and_hind_extension() {
        // Frame007: both forefeet tucked under the chest, hind pair stretched back.
        let fore = reference_chain(TAU * 6.0 / 24.0, true);
        let hind = reference_chain(TAU * 6.0 / 24.0, false);
        assert!((fore[2] - V2::new(-11.0, -114.0)).length() < 0.001);
        assert!((hind[2] - V2::new(-106.0, -101.0)).length() < 0.001);
        // Frame019: fore pair reaches toward travel, hind pair is folded under the rump.
        let fore = reference_chain(TAU * 18.0 / 24.0, true);
        let hind = reference_chain(TAU * 18.0 / 24.0, false);
        assert!((fore[2] - V2::new(24.0, -157.0)).length() < 0.001);
        assert!((hind[1] - V2::new(-39.0, -15.0)).length() < 0.001);
    }

    #[test]
    fn legs_in_each_pair_follow_one_frame_apart_instead_of_diagonal_trot() {
        let mut value = sprite();
        for i in 0..24 {
            value.phase = TAU * i as f32 / 24.0;
            let near = santa_legs(&value);
            value.phase += TAU / 24.0;
            let far = santa_legs(&value);
            for front in [0, 1] {
                for joint in 0..3 {
                    let relative = |leg: LegPose| [leg.knee - leg.hip, leg.hock - leg.hip, leg.hoof - leg.hip][joint];
                    assert!((relative(near[front]) - relative(far[front + 2])).length() < 0.002);
                }
            }
        }
    }

    #[test]
    fn measured_flight_clearance_and_fore_hind_contact_order_are_preserved() {
        let mut value = sprite();
        for (i, expected) in CLEARANCE.iter().enumerate() {
            value.phase = TAU * i as f32 / 24.0;
            let legs = santa_legs(&value);
            let floor = legs[..4].iter().map(|leg| leg.hoof.y).fold(f32::INFINITY, f32::min);
            let bias = floor - expected * 0.36;
            assert!((-0.002..=0.49).contains(&bias), "clearance frame{i}: bias={bias}");
        }
        value.phase = 0.0;
        let legs = santa_legs(&value);
        assert!(legs[1].contact && !legs[0].contact, "hind pair pushes first");
        value.phase = TAU * 18.0 / 24.0;
        let legs = santa_legs(&value);
        assert!(legs[0].contact && !legs[1].contact, "fore pair lands while hind pair is folded");
    }

    #[test]
    fn complete_rig_is_continuous_across_every_key_and_the_loop() {
        let mut value = sprite();
        for key in 0..=8 {
            let phase = TAU * key as f32 / 8.0;
            value.phase = phase - 0.00001;
            let before = santa_legs(&value);
            value.phase = phase + 0.00001;
            let after = santa_legs(&value);
            for (a, b) in before.iter().zip(after) {
                for (a, b) in [(a.hip, b.hip), (a.knee, b.knee), (a.hock, b.hock), (a.ankle, b.ankle), (a.hoof, b.hoof)] {
                    assert!((a - b).length() < 0.02, "joint jumped at key {key}: {a:?} -> {b:?}");
                }
            }
        }
    }

    #[test]
    fn mirrored_headings_and_display_scales_keep_finite_proportional_poses() {
        let mut value = sprite();
        for size in [[0.0, 0.0], [220.0, 88.0], [440.0, 176.0], [880.0, 352.0], [1760.0, 704.0]] {
            value.width = size[0];
            value.height = size[1];
            for i in 0..97 {
                value.phase = TAU * i as f32 / 96.0;
                value.angle = 0.0;
                value.direction = 1.0;
                let right = santa_legs(&value);
                value.direction = -1.0;
                let left = santa_legs(&value);
                for (a, b) in right.iter().zip(left) {
                    for (a, b) in [(a.hip, b.hip), (a.knee, b.knee), (a.hock, b.hock), (a.ankle, b.ankle), (a.hoof, b.hoof)] {
                        assert!(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite());
                        assert!((a.x + b.x).abs() < 0.002 && (a.y - b.y).abs() < 0.002);
                    }
                    for part in [leg_segment(a.hip, a.knee, 14.0 * a.scale, 0.17, 0.88), hoof_placement(a)] {
                        assert!(part.anchor.x.is_finite() && part.anchor.y.is_finite() && part.angle.is_finite());
                        assert!(part.size.iter().all(|v| v.is_finite() && *v > 0.0));
                    }
                }
            }
        }
    }

    #[test]
    fn body_velocity_does_not_snap_when_support_changes_between_paired_feet() {
        let mut value = sprite();
        let epsilon = 0.0003;
        for sample in 0..384 {
            let phase = TAU * sample as f32 / 384.0;
            value.phase = phase - epsilon;
            let before = santa_body_parts(&value)[1].placement.anchor.y;
            value.phase = phase;
            let at = santa_body_parts(&value)[1].placement.anchor.y;
            value.phase = phase + epsilon;
            let after = santa_body_parts(&value)[1].placement.anchor.y;
            let change = ((after - at) - (at - before)).abs() / epsilon;
            assert!(change < 4.0, "body velocity corner at {phase}: {change}");
        }
    }

    /// Writes production joint poses and the exact painted-part placement/UV/order
    /// used for an offline software preview; never starts the application or a GPU.
    #[test]
    #[ignore]
    fn export_reference_preview() {
        use std::io::Write;
        let directory = std::path::PathBuf::from(std::env::var_os("CTU_GALLOP_PREVIEW_DIR").expect("set CTU_GALLOP_PREVIEW_DIR"));
        std::fs::create_dir_all(&directory).unwrap();
        let mut parts = std::fs::File::create(directory.join("production-parts.csv")).unwrap();
        let mut joints = std::fs::File::create(directory.join("production-joints.csv")).unwrap();
        writeln!(parts, "frame,phase,order,kind,texture,x,y,width,height,angle,direction,u,v,uw,uh,r,g,b,ax,ay,bx,by").unwrap();
        writeln!(joints, "frame,leg,far,front,contact,hipx,hipy,kneex,kneey,hockx,hocky,anklex,ankley,hoofx,hoofy").unwrap();
        for frame in 0..48 {
            let mut value = sprite();
            value.anchor = V2::new(240.0, 12.0);
            value.phase = TAU * frame as f32 / 48.0;
            let legs = santa_legs(&value);
            for (i, leg) in legs.iter().enumerate() {
                writeln!(
                    joints,
                    "{frame},{i},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    leg.far,
                    leg.front,
                    leg.contact,
                    leg.hip.x,
                    leg.hip.y,
                    leg.knee.x,
                    leg.knee.y,
                    leg.hock.x,
                    leg.hock.y,
                    leg.ankle.x,
                    leg.ankle.y,
                    leg.hoof.x,
                    leg.hoof.y
                )
                .unwrap();
            }
            let mut order = 0;
            let mut image = |texture: &str, part: PartPlacement, uv: [f32; 4], tint: f32| {
                writeln!(
                    parts,
                    "{frame},{},{order},image,{texture},{},{},{},{},{},{},{},{},{},{},{tint},{tint},{tint},0,0,0,0",
                    value.phase, part.anchor.x, part.anchor.y, part.size[0], part.size[1], part.angle, value.direction, uv[0], uv[1], uv[2], uv[3]
                )
                .unwrap();
                order += 1;
            };
            let mut leg_image = |leg: &LegPose| {
                let tint = if leg.far { 0.74 } else { 1.0 };
                image("deer-hoof.png", hoof_placement(leg), [0., 0., 1., 1.], tint);
                image("deer-shank.png", leg_segment(leg.hock, leg.ankle, 11.0 * leg.scale, 0.14, 0.90), [0., 0., 1., 1.], tint);
                image("deer-shank.png", leg_segment(leg.knee, leg.hock, 13.0 * leg.scale, 0.14, 0.90), [0., 0., 1., 1.], tint);
                image(
                    "deer-thigh.png",
                    leg_segment(leg.hip, leg.knee, (if leg.front { 14.0 } else { 16.0 }) * leg.scale, 0.17, 0.88),
                    [0., 0., 1., 1.],
                    tint,
                );
            };
            for leg in legs.iter().filter(|leg| leg.far) {
                leg_image(leg);
            }
            // End the closure's temporary mutable borrow before drawing torso layers.
            drop(leg_image);
            for body in santa_body_parts(&value) {
                image("santa-body.png", body.placement, body.uv, 1.0);
            }
            drop(image);
            for rein in santa_reins(&value) {
                writeln!(
                    parts,
                    "{frame},{},{order},line,,0,0,{},0,0,1,0,0,1,1,{},{},{},{},{},{},{}",
                    value.phase, rein.width, rein.tint[0], rein.tint[1], rein.tint[2], rein.from.x, rein.from.y, rein.to.x, rein.to.y
                )
                .unwrap();
                order += 1;
            }
            for leg in legs.iter().filter(|leg| !leg.far) {
                for (texture, part) in [
                    ("deer-hoof.png", hoof_placement(leg)),
                    ("deer-shank.png", leg_segment(leg.hock, leg.ankle, 11.0 * leg.scale, 0.14, 0.90)),
                    ("deer-shank.png", leg_segment(leg.knee, leg.hock, 13.0 * leg.scale, 0.14, 0.90)),
                    ("deer-thigh.png", leg_segment(leg.hip, leg.knee, (if leg.front { 14.0 } else { 16.0 }) * leg.scale, 0.17, 0.88)),
                ] {
                    writeln!(
                        parts,
                        "{frame},{},{order},image,{texture},{},{},{},{},{},{},0,0,1,1,1,1,1,0,0,0,0",
                        value.phase, part.anchor.x, part.anchor.y, part.size[0], part.size[1], part.angle, value.direction
                    )
                    .unwrap();
                    order += 1;
                }
            }
        }
    }
}

/// Light positions follow the painted cable, in image coordinates from its top left.
pub(crate) const TREE_LIGHTS: &[[f32; 2]] = &[
    [0.54, 0.211],
    [0.48, 0.232],
    [0.41, 0.252],
    [0.44, 0.277],
    [0.51, 0.303],
    [0.58, 0.331],
    [0.53, 0.361],
    [0.44, 0.393],
    [0.37, 0.423],
    [0.38, 0.451],
    [0.45, 0.478],
    [0.55, 0.506],
    [0.65, 0.535],
    [0.62, 0.562],
    [0.52, 0.585],
    [0.41, 0.612],
    [0.31, 0.640],
    [0.34, 0.667],
    [0.44, 0.696],
    [0.55, 0.727],
    [0.61, 0.756],
    [0.55, 0.785],
    [0.43, 0.807],
    [0.31, 0.827],
    [0.29, 0.851],
    [0.40, 0.875],
    [0.53, 0.894],
    [0.65, 0.887],
    [0.75, 0.858],
    [0.79, 0.823],
];

pub(crate) const LIGHT_COLORS: [[f32; 4]; 5] =
    [[1.00, 0.16, 0.09, 1.0], [0.16, 0.62, 1.00, 1.0], [1.00, 0.73, 0.16, 1.0], [0.24, 0.95, 0.34, 1.0], [0.82, 0.25, 0.94, 1.0]];
