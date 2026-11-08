use crate::agent_presentation::{FigureMotion, FigurePose, MAX_FIGURE_STEPS, SceneAxis};
use serde::Deserialize;

type Matrix = [f32; 16];
type Quaternion = [f64; 4];

const IDENTITY: Matrix = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

#[derive(Deserialize)]
pub(super) struct PoseNode {
    #[serde(default)]
    parent_local: bool,
    #[serde(default = "identity")]
    bind: Matrix,
    parent: Option<usize>,
    step: usize,
    motion: Option<FigureMotion>,
}

fn identity() -> Matrix {
    IDENTITY
}

impl PoseNode {
    pub(super) fn animates_at(&self, elapsed: f64) -> bool {
        self.motion
            .as_ref()
            .is_some_and(|motion| motion.animates_at(elapsed))
    }

    pub(super) fn valid(&self, index: usize) -> bool {
        let bounded = |value: &f64| value.is_finite() && value.abs() <= 360.;
        self.bind.iter().all(|v| v.is_finite() && v.abs() <= 10000.)
            && self.parent.is_none_or(|parent| parent < index)
            && self.step < MAX_FIGURE_STEPS
            && self.motion.as_ref().is_none_or(|motion| {
                motion.pivot.iter().all(bounded)
                    && motion.keys.len() <= 64
                    && motion.keys.iter().all(|key| {
                        key.time().is_finite()
                            && (0.0..=(MAX_FIGURE_STEPS - 1) as f64).contains(&key.time())
                            && key.appearance_is_valid()
                            && key.scale.is_none_or(|scale| (0.01..=4.0).contains(&scale))
                            && key.translate.iter().chain(&key.rotate).all(bounded)
                    })
                    && motion.periodic_is_valid()
                    && motion.channels_are_valid((MAX_FIGURE_STEPS - 1) as f64)
            })
    }
}

fn euler([x, y, z]: [f64; 3]) -> Quaternion {
    let (sx, cx) = (x.to_radians() / 2.).sin_cos();
    let (sy, cy) = (y.to_radians() / 2.).sin_cos();
    let (sz, cz) = (z.to_radians() / 2.).sin_cos();
    [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
    ]
}

fn slerp(a: Quaternion, mut b: Quaternion, t: f64) -> Quaternion {
    let mut dot: f64 = a.iter().zip(b).map(|(a, b)| a * b).sum();
    if dot < 0. {
        dot = -dot;
        b = b.map(|v| -v);
    }
    if dot > 0.9995 {
        let result = std::array::from_fn::<_, 4, _>(|i| a[i] + t * (b[i] - a[i]));
        let length = result.iter().map(|v| v * v).sum::<f64>().sqrt();
        return result.map(|v| v / length);
    }
    let angle = dot.clamp(-1., 1.).acos();
    let weight_a = ((1. - t) * angle).sin() / angle.sin();
    let weight_b = (t * angle).sin() / angle.sin();
    std::array::from_fn(|i| a[i] * weight_a + b[i] * weight_b)
}

fn path_offset(path: &crate::agent_presentation::FigurePath, elapsed: f64) -> [f64; 3] {
    let count = path.points.len();
    let segments = if path.once { count - 1 } else { count };
    let phase = if path.once {
        let t = (elapsed / path.period).clamp(0., 1.);
        t * t * (3. - 2. * t)
    } else {
        elapsed.rem_euclid(path.period) / path.period
    };
    let segment = (phase * segments as f64).floor().min((segments - 1) as f64) as isize;
    let t = phase * segments as f64 - segment as f64;
    let point = |offset: isize| {
        let index = segment + offset;
        path.points[if path.once {
            index.clamp(0, count as isize - 1)
        } else {
            index.rem_euclid(count as isize)
        } as usize]
    };
    let [a, b, c, d] = [point(-1), point(0), point(1), point(2)];
    std::array::from_fn(|i| {
        let u = 1. - t;
        let b1 = b[i] + (c[i] - a[i]) / 6.;
        let b2 = c[i] - (d[i] - b[i]) / 6.;
        u * u * u * b[i] + 3. * u * u * t * b1 + 3. * u * t * t * b2 + t * t * t * c[i]
    })
}

fn local(node: &PoseNode, time: f64, elapsed: f64) -> Matrix {
    let Some(motion) = &node.motion else {
        return IDENTITY;
    };
    let time = motion.pose_time(time, elapsed);
    let rest = FigurePose {
        at: None,
        step: node.step,
        translate: [0.; 3],
        rotate: [0.; 3],
        scale: None,
        show: None,
        tint: None,
        glow: None,
    };
    let mut before = &rest;
    let mut after = &rest;
    for key in &motion.keys {
        after = key;
        if key.time() > time {
            break;
        }
        before = key;
    }
    let fraction = if after.time() > before.time() {
        ((time - before.time()) / (after.time() - before.time())).clamp(0., 1.)
    } else {
        0.
    };
    let amount = fraction * fraction * (3. - 2. * fraction);
    let channel_pose = (!motion.channels.is_empty()).then(|| motion.channel_pose(time));
    let mut q = channel_pose.map_or_else(
        || slerp(euler(before.rotate), euler(after.rotate), amount),
        |(_, rotate)| euler(rotate),
    );
    if let Some(spin) = &motion.spin {
        let phase = elapsed.rem_euclid(spin.period) / spin.period * 360.;
        let axis = match spin.axis {
            SceneAxis::X => 0,
            SceneAxis::Y => 1,
            SceneAxis::Z => 2,
        };
        let mut angles = [0.; 3];
        angles[axis] = phase;
        let [x, y, z, w] = q;
        let [a, b, c, d] = euler(angles);
        q = [
            w * a + x * d + y * c - z * b,
            w * b - x * c + y * d + z * a,
            w * c + x * b - y * a + z * d,
            w * d - x * a - y * b - z * c,
        ];
    }
    let tick = motion.tick.as_ref();
    let tick_amount = tick.map_or(0., |tick| tick.amount_at(elapsed));
    if let Some([from, to]) = tick.and_then(|tick| tick.rotate) {
        q = slerp(euler(from), euler(to), tick_amount);
    }
    let [x, y, z, w] = q;
    let mut matrix = [
        1. - 2. * (y * y + z * z),
        2. * (x * y + w * z),
        2. * (x * z - w * y),
        0.,
        2. * (x * y - w * z),
        1. - 2. * (x * x + z * z),
        2. * (y * z + w * x),
        0.,
        2. * (x * z + w * y),
        2. * (y * z - w * x),
        1. - 2. * (x * x + y * y),
        0.,
        0.,
        0.,
        0.,
        1.,
    ];
    let wave = |period: f64| (elapsed.rem_euclid(period) / period * std::f64::consts::TAU).sin();
    let before_scale = before.scale.unwrap_or(1.);
    let after_scale = after.scale.unwrap_or(1.);
    let staged_scale = before_scale + amount * (after_scale - before_scale);
    let pulse_scale = motion
        .pulse
        .as_ref()
        .map_or(1., |pulse| 1. + pulse.amplitude * wave(pulse.period));
    let scale = tick
        .and_then(|tick| tick.scale)
        .map_or(staged_scale * pulse_scale, |[from, to]| {
            from + (to - from) * tick_amount
        });
    for value in &mut matrix[..12] {
        *value *= scale;
    }
    let bob = motion.bob.as_ref().map_or([0.; 3], |bob| {
        bob.offset.map(|value| value * wave(bob.period))
    });
    let mut orbit_offset = [0.; 3];
    if let Some(orbit) = &motion.orbit {
        let (a, b) = match orbit.axis {
            SceneAxis::X => (1, 2),
            SceneAxis::Y => (2, 0),
            SceneAxis::Z => (0, 1),
        };
        let (sin, cos) =
            (elapsed.rem_euclid(orbit.period) / orbit.period * std::f64::consts::TAU).sin_cos();
        let u = motion.pivot[a] - orbit.center[a];
        let v = motion.pivot[b] - orbit.center[b];
        orbit_offset[a] = u * cos - v * sin - u;
        orbit_offset[b] = u * sin + v * cos - v;
    }
    let path = motion
        .path
        .as_ref()
        .map_or([0.; 3], |path| path_offset(path, elapsed));
    for row in 0..3 {
        let rotated_pivot: f64 = (0..3)
            .map(|col| matrix[col * 4 + row] * motion.pivot[col])
            .sum();
        let position = tick.and_then(|tick| tick.translate).map_or_else(
            || {
                channel_pose.map_or_else(
                    || {
                        before.translate[row]
                            + amount * (after.translate[row] - before.translate[row])
                    },
                    |(translate, _)| translate[row],
                ) + bob[row]
                    + orbit_offset[row]
                    + path[row]
            },
            |[from, to]| from[row] + (to[row] - from[row]) * tick_amount,
        );
        matrix[12 + row] = position + motion.pivot[row] - rotated_pivot;
    }
    matrix.map(|v| v as f32)
}

fn appearance_track<const N: usize>(
    keys: &[FigurePose],
    start: usize,
    time: f64,
    default: [f64; N],
    field: impl Fn(&FigurePose) -> Option<[f64; N]>,
) -> [f64; N] {
    let mut before = (start as f64, default);
    for key in keys {
        let Some(value) = field(key) else { continue };
        if key.time() > time {
            let t = ((time - before.0) / (key.time() - before.0).max(f64::EPSILON)).clamp(0., 1.);
            let t = t * t * (3. - 2. * t);
            return std::array::from_fn(|i| before.1[i] + (value[i] - before.1[i]) * t);
        }
        before = (key.time(), value);
    }
    before.1
}

pub(super) fn appearance(node: &PoseNode, time: f64, elapsed: f64, base: [f32; 4]) -> [f32; 4] {
    let Some(motion) = &node.motion else {
        return base;
    };
    let time = motion.pose_time(time, elapsed);
    let mut rgb = appearance_track(
        &motion.keys,
        node.step,
        time,
        [base[0] as f64, base[1] as f64, base[2] as f64],
        |key| key.tint,
    );
    let [mut glow] = appearance_track(&motion.keys, node.step, time, [0.], |key| {
        key.glow.map(|value| [value])
    });
    if let Some(tick) = &motion.tick {
        let t = tick.amount_at(elapsed);
        if let Some([from, to]) = tick.tint {
            rgb = std::array::from_fn(|i| from[i] + (to[i] - from[i]) * t);
        }
        if let Some([from, to]) = tick.glow {
            glow = from + (to - from) * t;
        }
    }
    [rgb[0] as f32, rgb[1] as f32, rgb[2] as f32, glow as f32]
}

pub(super) fn visibility(nodes: &[PoseNode], time: f64, elapsed: f64) -> Vec<bool> {
    let mut visible = Vec::with_capacity(nodes.len());
    for node in nodes {
        let parent_visible = node
            .parent
            .and_then(|index| visible.get(index))
            .copied()
            .unwrap_or(true);
        let own_visible = node
            .motion
            .as_ref()
            .and_then(|motion| {
                let time = motion.pose_time(time, elapsed);
                motion
                    .keys
                    .iter()
                    .rev()
                    .filter(|key| key.time() <= time)
                    .find_map(|key| key.show)
            })
            .unwrap_or(true);
        let own_visible = node
            .motion
            .as_ref()
            .and_then(|motion| motion.tick.as_ref())
            .and_then(|tick| {
                tick.show
                    .map(|values| values[usize::from(tick.amount_at(elapsed) >= 0.5)])
            })
            .unwrap_or(own_visible);
        visible.push(parent_visible && own_visible);
    }
    visible
}

fn multiply(parent: &Matrix, matrix: &Matrix) -> Matrix {
    std::array::from_fn(|index| {
        let (column, row) = (index / 4, index % 4);
        (0..4)
            .map(|k| parent[k * 4 + row] * matrix[column * 4 + k])
            .sum()
    })
}

pub(super) fn sample(nodes: &[PoseNode], time: f64, elapsed: f64) -> Vec<Matrix> {
    let mut world = Vec::with_capacity(nodes.len());
    let mut frames = Vec::with_capacity(nodes.len());
    for node in nodes {
        let matrix = local(node, time, elapsed);
        let parents = if node.parent_local { &frames } else { &world };
        let result = node
            .parent
            .and_then(|index| parents.get(index))
            .map_or(matrix, |parent| multiply(parent, &matrix));
        frames.push(multiply(&result, &node.bind));
        world.push(result);
    }
    world
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_track_holds_until_its_scheduled_start() {
        let nodes: Vec<PoseNode> = serde_json::from_value(serde_json::json!([
            {"step":0,"motion":{"pivot":[0,0,0],"keys":[
                {"at":0.25}, {"at":0.75,"translate":[2,0,0]}
            ]}}
        ]))
        .unwrap();
        assert!(nodes[0].valid(0));
        assert_eq!(sample(&nodes, 0.125, 0.)[0][12], 0.);
        assert_eq!(sample(&nodes, 0.5, 0.)[0][12], 1.);
        assert_eq!(sample(&nodes, 0.9, 0.)[0][12], 2.);
    }

    #[test]
    fn cyclic_keys_share_elapsed_time_and_persist_visibility() {
        let nodes: Vec<PoseNode> = serde_json::from_value(serde_json::json!([
            {"step":0,"motion":{"pivot":[0,0,0],"cycle":{"period":4,"ping_pong":true},"keys":[
                {"at":0}, {"at":0.5,"translate":[2,0,0],"show":false}, {"at":1,"translate":[3,0,0]}
            ]}},
            {"step":0,"parent":0,"parent_local":true}
        ]))
        .unwrap();
        assert!(nodes[0].valid(0));
        assert_eq!(sample(&nodes, 0., 1.)[1][12], 2.);
        assert_eq!(sample(&nodes, 0., 2.)[1][12], 3.);
        assert_eq!(sample(&nodes, 0., 3.)[1][12], 2.);
        assert_eq!(visibility(&nodes, 0., 2.), vec![false, false]);
        assert_eq!(visibility(&nodes, 0., 4.), vec![true, true]);
    }

    #[test]
    fn parent_local_inherits_group_bind_placement() {
        let nodes: Vec<PoseNode> = serde_json::from_value(serde_json::json!([
            {"step":0,"bind":[0,1,0,0,-1,0,0,0,0,0,1,0,3,0,0,1]},
            {"step":0,"parent":0,"parent_local":true}
        ]))
        .unwrap();
        let poses = sample(&nodes, 0., 0.);
        assert_eq!(poses[1][12], 3.);
        assert_eq!(poses[1][1], 1.);
        assert_eq!(poses[1][4], -1.);
    }

    #[test]
    fn mirrored_carrier_bind_reaches_native_child_unchanged() {
        let nodes: Vec<PoseNode> = serde_json::from_value(serde_json::json!([
            {"step":0,"bind":[-1,0,0,0,0,1,0,0,0,0,1,0,-2,0,0,1]},
            {"step":0,"parent":0,"parent_local":true}
        ]))
        .unwrap();
        assert!(nodes.iter().enumerate().all(|(i, node)| node.valid(i)));
        let matrix = sample(&nodes, 0., 0.)[1];
        assert_eq!(matrix[0], -1.);
        assert_eq!(matrix[12], -2.);
        assert_eq!(matrix[0] + matrix[12], -3.);
    }

    #[test]
    fn near_hidden_pose_scale_is_safe_and_zero_is_rejected() {
        let mut nodes: Vec<PoseNode> = serde_json::from_value(serde_json::json!([
            {"step":0,"motion":{"pivot":[0,0,0],"keys":[{"step":0,"scale":0.01},{"step":1,"scale":1.0}]}}
        ])).unwrap();
        assert!(nodes[0].valid(0));
        assert!((sample(&nodes, 0., 0.)[0][0] - 0.01).abs() < 0.0001);
        nodes[0].motion.as_mut().unwrap().keys[0].scale = Some(0.0);
        assert!(!nodes[0].valid(0));
    }

    #[test]
    fn native_figure_pose_inherits_hinge_and_spin() {
        let nodes: Vec<PoseNode> = serde_json::from_value(serde_json::json!([
            {"step":0,"motion":{"pivot":[1,0,0],"keys":[{"step":1,"rotate":[0,0,90]}],
              "spin":{"axis":"z","period":4}}},
            {"step":0,"parent":0,"motion":{"pivot":[3,0,0],"keys":[{"step":1,"translate":[1,0,0]}]}}
        ]))
        .unwrap();
        assert!(
            nodes
                .iter()
                .enumerate()
                .all(|(index, node)| node.valid(index))
        );
        let pose = sample(&nodes, 1., 1.)[1];
        let x = 3. * pose[0] + pose[12];
        let y = 3. * pose[1] + pose[13];
        assert!((x + 2.).abs() < 0.0001 && y.abs() < 0.0001, "{x}, {y}");
    }

    #[test]
    fn bob_and_pulse_propagate_to_children() {
        let nodes: Vec<PoseNode> = serde_json::from_value(serde_json::json!([
            {"step":0,"motion":{"pivot":[1,0,0],"bob":{"offset":[0,2,0],"period":4},
                "pulse":{"amplitude":0.5,"period":4}}},
            {"step":0,"parent":0}
        ]))
        .unwrap();
        assert!(
            nodes
                .iter()
                .enumerate()
                .all(|(index, node)| node.valid(index))
        );
        let child = sample(&nodes, 0., 1.)[1];
        assert!((3. * child[0] + child[12] - 4.).abs() < 0.0001);
        assert!((child[13] - 2.).abs() < 0.0001);
    }

    #[test]
    fn keyed_scale_and_pulse_compose_through_a_group() {
        let nodes: Vec<PoseNode> = serde_json::from_value(serde_json::json!([
            {"step":0,"motion":{"pivot":[1,0,0],"keys":[{"step":1,"scale":3}],
                "pulse":{"amplitude":0.5,"period":4}}},
            {"step":0,"parent":0}
        ]))
        .unwrap();
        assert!(
            nodes
                .iter()
                .enumerate()
                .all(|(index, node)| node.valid(index))
        );
        let child = sample(&nodes, 0.5, 1.)[1];
        assert!((child[0] + child[12] - 1.).abs() < 0.0001);
        assert!((3. * child[0] + child[12] - 7.).abs() < 0.0001);
    }

    #[test]
    fn cyclic_appearance_overrides_staged_values_and_inherits_visibility() {
        let nodes: Vec<PoseNode> = serde_json::from_value(serde_json::json!([
            {"step":0,"motion":{"pivot":[0,0,0],"keys":[{"step":0,"glow":1}],
             "tick":{"period":4,"tint":[[1,0,0],[0,0,1]],"glow":[0,0.5],"show":[true,false]}}},
            {"step":0,"parent":0}
        ]))
        .unwrap();
        assert!(nodes.iter().enumerate().all(|(i, node)| node.valid(i)));
        assert_eq!(visibility(&nodes, 0., 2.), [false, false]);
        assert_eq!(visibility(&nodes, 0., 4.), [true, true]);
        assert_eq!(appearance(&nodes[0], 0., 2., [1.; 4]), [0., 0., 1., 0.5]);
    }
}

#[cfg(test)]
#[test]
fn independent_channels_match_analytic_motion_and_carry_children() {
    let nodes: Vec<PoseNode> = serde_json::from_value(serde_json::json!([
        {"step":0,"motion":{"pivot":[0,0,0],"channels":[
            {"channel":"translate.y","start":0,"end":3,"from":0,"to":2},
            {"channel":"rotate.x","start":1,"end":5,"from":0,"to":90}
        ]}},
        {"step":0,"parent":0,"parent_local":true}
    ]))
    .unwrap();
    assert!(nodes[0].valid(0));
    for i in 0..49 {
        let time = i as f64 / 8.;
        let ease = |t: f64| {
            let t = t.clamp(0., 1.);
            t * t * (3. - 2. * t)
        };
        let y = 2. * ease(time / 3.);
        let angle = (90. * ease((time - 1.) / 4.)).to_radians();
        let frames = sample(&nodes, time, 0.);
        assert_eq!(frames[0], frames[1]);
        // Local point [0,0,1] after Rx and translation.
        assert!(((frames[1][9] + frames[1][13]) as f64 - (y - angle.sin())).abs() < 1e-5);
        assert!((frames[1][10] as f64 - angle.cos()).abs() < 1e-5);
    }
}
