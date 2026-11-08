//! A bounded, provider-neutral presentation contract for compact agent answers.
//!
//! The model can suggest data for registered Pluto components, but this module
//! never accepts markup, scripts, styles, arbitrary actions, or remote assets.
#[path = "figure_program.rs"]
mod figure_program;
use serde::{Deserialize, Serialize};

// Detailed, bounded scenes may exceed 32 KiB without being unsafe.
pub const MAX_FIGURE_STEPS: usize = 8;
pub const MAX_FIGURE_PARTS: usize = 192;
pub const MAX_FIGURE_ID_BYTES: usize = 256;
pub const MAX_FIGURE_INSTANCES: usize = 256;
pub(crate) const MAX_FIGURE_LABEL_CHARS: usize = 100;

pub(crate) fn bounded_figure_label(label: &str) -> String {
    label.trim().chars().take(MAX_FIGURE_LABEL_CHARS).collect()
}
const MAX_BLOCK_BYTES: usize = 256_000;
const MAX_NODES: usize = 36;
const MAX_DEPTH: usize = 3;
const MAX_TEXT_BYTES: usize = 4_000;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentPresentation {
    pub version: u8,
    #[serde(default)]
    pub title: Option<String>,
    pub elements: Vec<PresentationElement>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PresentationElement {
    Heading {
        text: String,
        #[serde(default)]
        level: Option<u8>,
    },
    Text {
        text: String,
        #[serde(default)]
        tone: Option<Tone>,
    },
    Badge {
        label: String,
        #[serde(default)]
        tone: Option<Tone>,
    },
    Stat {
        label: String,
        value: String,
        #[serde(default)]
        detail: Option<String>,
    },
    KeyValue {
        items: Vec<KeyValueItem>,
    },
    Callout {
        #[serde(default)]
        title: Option<String>,
        text: String,
        #[serde(default)]
        tone: Option<Tone>,
    },
    List {
        items: Vec<String>,
        #[serde(default)]
        ordered: bool,
    },
    Checklist {
        items: Vec<ChecklistItem>,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Code {
        code: String,
        #[serde(default)]
        language: Option<String>,
    },
    Quote {
        text: String,
        #[serde(default)]
        attribution: Option<String>,
    },
    Progress {
        label: String,
        value: f64,
        #[serde(default)]
        max: Option<f64>,
        #[serde(default)]
        detail: Option<String>,
    },
    Timeline {
        items: Vec<TimelineItem>,
    },
    BarChart {
        label: String,
        items: Vec<BarItem>,
    },
    FileChange {
        path: String,
        status: FileChangeStatus,
        #[serde(default)]
        summary: Option<String>,
    },
    Section {
        #[serde(default)]
        title: Option<String>,
        elements: Vec<PresentationElement>,
    },
    Columns {
        columns: Vec<Vec<PresentationElement>>,
    },
    MetricGrid {
        items: Vec<MetricItem>,
    },
    StatusStrip {
        items: Vec<StatusItem>,
    },
    PhaseCard {
        title: String,
        #[serde(default)]
        detail: Option<String>,
        #[serde(default)]
        state: Option<ChecklistState>,
    },
    Comparison {
        left: ComparisonSide,
        right: ComparisonSide,
    },
    BlackHole {},
    ProceduralScene {
        label: String,
        ops: Vec<SceneOp>,
        #[serde(default)]
        contacts: Vec<SceneContact>,
    },
    FigureScene {
        #[serde(default)]
        connected: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        assembly_plan: Option<FigureAssemblyPlan>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        playback: Option<FigurePlayback>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        camera: Option<FigureCamera>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        cues: Vec<FigureCue>,
        label: String,
        parts: Vec<FigurePart>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        labels: Vec<FigureLabel>,
        #[serde(default)]
        steps: Vec<VisualFrame>,
    },
    SystemMap {
        label: String,
        nodes: Vec<MapNode>,
        links: Vec<MapLink>,
    },
    LineChart {
        label: String,
        #[serde(default)]
        unit: Option<String>,
        points: Vec<BarItem>,
    },
    Waveform {
        label: String,
        values: Vec<f64>,
    },
    Heatmap {
        label: String,
        columns: Vec<String>,
        rows: Vec<HeatmapRow>,
    },
    CrossSection {
        label: String,
        layers: Vec<CrossSectionLayer>,
    },
    VisualScene {
        label: String,
        frames: Vec<VisualFrame>,
    },
    Divider {},
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigurePart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joint: Option<FigureJoint>,
    #[serde(default)]
    pub parent_local: bool,
    pub id: String,
    pub label: String,
    pub detail: String,
    pub shape: FigureShape,
    #[serde(default)]
    pub transform: SceneTransform,
    pub color: String,
    pub material: Option<FigureMaterial>,
    pub parent: Option<String>,
    #[serde(default)]
    pub step: usize,
    pub mirror: Option<SceneAxis>,
    pub repeat: Option<FigureRepeat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub around: Option<FigureAround>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut: Option<FigureCut>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thick: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<FigureMotion>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureJoint {
    pub anchor: [f64; 3],
    pub parent_anchor: [f64; 3],
    pub tolerance: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureLabel {
    pub text: String,
    pub part: Option<String>,
    pub point: Option<[f64; 3]>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureMotion {
    pub pivot: [f64; 3],
    #[serde(default)]
    pub keys: Vec<FigurePose>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<FigureChannel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spin: Option<FigureSpin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bob: Option<FigureBob>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pulse: Option<FigurePulse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orbit: Option<FigureOrbit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<FigurePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick: Option<FigureTick>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<FigureCycle>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureCycle {
    pub period: f64,
    #[serde(default)]
    pub phase: f64,
    #[serde(default)]
    pub ping_pong: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureTick {
    pub period: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<f64>,
    pub translate: Option<[[f64; 3]; 2]>,
    pub rotate: Option<[[f64; 3]; 2]>,
    pub scale: Option<[f64; 2]>,
    pub tint: Option<[[f64; 3]; 2]>,
    pub glow: Option<[f64; 2]>,
    pub show: Option<[bool; 2]>,
}

impl FigureTick {
    #[cfg(windows)]
    pub fn amount_at(&self, elapsed: f64) -> f64 {
        let phase = (elapsed.rem_euclid(self.period) / self.period + self.phase.unwrap_or(0.))
            * std::f64::consts::TAU;
        (1. - phase.cos()) * 0.5
    }

    fn valid(&self) -> bool {
        (0.25..=60.).contains(&self.period)
            && self.phase.is_none_or(|phase| (0.0..1.0).contains(&phase))
            && (self.translate.is_some()
                || self.rotate.is_some()
                || self.scale.is_some()
                || self.tint.is_some()
                || self.glow.is_some()
                || self.show.is_some())
            && self
                .translate
                .is_none_or(|ends| ends.iter().flatten().all(|v| (-24. ..=24.).contains(v)))
            && self
                .rotate
                .is_none_or(|ends| ends.iter().flatten().all(|v| (-360. ..=360.).contains(v)))
            && self
                .scale
                .is_none_or(|ends| ends.iter().all(|v| (0.1..=4.).contains(v)))
            && self
                .tint
                .is_none_or(|ends| ends.iter().flatten().all(|v| (0.0..=1.0).contains(v)))
            && self
                .glow
                .is_none_or(|ends| ends.iter().all(|v| (0.0..=1.0).contains(v)))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigurePath {
    pub points: Vec<[f64; 3]>,
    pub period: f64,
    #[serde(default)]
    pub once: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureOrbit {
    pub center: [f64; 3],
    pub axis: SceneAxis,
    pub period: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureBob {
    pub offset: [f64; 3],
    pub period: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigurePulse {
    pub amplitude: f64,
    pub period: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureChannel {
    pub channel: String,
    pub start: f64,
    pub end: f64,
    pub from: f64,
    pub to: f64,
}

impl FigureChannel {
    pub fn index(&self) -> Option<usize> {
        [
            "translate.x",
            "translate.y",
            "translate.z",
            "rotate.x",
            "rotate.y",
            "rotate.z",
        ]
        .iter()
        .position(|name| *name == self.channel)
    }
    pub fn sample(&self, time: f64) -> f64 {
        let t = ((time - self.start) / (self.end - self.start)).clamp(0., 1.);
        self.from + (self.to - self.from) * t * t * (3. - 2. * t)
    }
}

impl FigureMotion {
    pub fn channel_pose(&self, time: f64) -> ([f64; 3], [f64; 3]) {
        let mut values = [0.; 6];
        let mut seen = [false; 6];
        for track in &self.channels {
            if let Some(i) = track.index() {
                if !seen[i] || time >= track.start {
                    values[i] = track.sample(time);
                }
                seen[i] = true;
            }
        }
        (
            [values[0], values[1], values[2]],
            [values[3], values[4], values[5]],
        )
    }

    pub fn channels_are_valid(&self, last: f64) -> bool {
        if self.channels.is_empty() {
            return true;
        }
        if self.channels.len() > 64 || !self.keys.is_empty() || self.is_periodic() {
            return false;
        }
        let mut previous: [Option<(f64, f64)>; 6] = [None; 6];
        for track in &self.channels {
            let Some(i) = track.index() else {
                return false;
            };
            let limit = if i < 3 { 24. } else { 360. };
            if !track.start.is_finite()
                || !track.end.is_finite()
                || track.start < 0.
                || track.end <= track.start
                || track.end > last
                || !track.from.is_finite()
                || !track.to.is_finite()
                || track.from.abs() > limit
                || track.to.abs() > limit
                || (previous[i].is_none() && track.from != 0.)
                || previous[i].is_some_and(|(end, value)| {
                    track.start < end || (track.from - value).abs() > 1e-9
                })
            {
                return false;
            }
            previous[i] = Some((track.end, track.to));
        }
        true
    }

    #[cfg(windows)]
    pub fn pose_time(&self, stage: f64, elapsed: f64) -> f64 {
        self.cycle.as_ref().map_or(stage, |cycle| {
            let phase =
                (elapsed.rem_euclid(cycle.period) / cycle.period + cycle.phase).rem_euclid(1.);
            if cycle.ping_pong {
                1. - (1. - 2. * phase).abs()
            } else {
                phase
            }
        })
    }

    pub fn is_periodic(&self) -> bool {
        self.spin.is_some()
            || self.bob.is_some()
            || self.pulse.is_some()
            || self.orbit.is_some()
            || self.tick.is_some()
            || self.cycle.is_some()
            || self.path.is_some()
    }

    pub fn animates_at(&self, elapsed: f64) -> bool {
        self.spin.is_some()
            || self.bob.is_some()
            || self.pulse.is_some()
            || self.orbit.is_some()
            || self.tick.is_some()
            || self.cycle.is_some()
            || self
                .path
                .as_ref()
                .is_some_and(|path| !path.once || elapsed < path.period)
    }

    pub fn periodic_is_valid(&self) -> bool {
        let period_ok = |period| (0.25..=60.0).contains(&period);
        self.cycle.as_ref().is_none_or(|cycle| {
            period_ok(cycle.period)
                && (0.0..1.0).contains(&cycle.phase)
                && self.keys.len() >= 2
                && self.keys.first().and_then(|key| key.at) == Some(0.)
                && self.keys.last().and_then(|key| key.at) == Some(1.)
                && self
                    .keys
                    .iter()
                    .all(|key| key.at.is_some_and(|time| (0.0..=1.0).contains(&time)))
        }) && self.tick.as_ref().is_none_or(FigureTick::valid)
            && self.path.as_ref().is_none_or(|path| {
                period_ok(path.period)
                    && (if path.once { 2 } else { 3 }..=16).contains(&path.points.len())
                    && path.points.first() == Some(&[0.0; 3])
                    && path
                        .points
                        .iter()
                        .flatten()
                        .all(|value| (-24.0..=24.0).contains(value))
            })
            && self.spin.as_ref().is_none_or(|spin| period_ok(spin.period))
            && self.orbit.as_ref().is_none_or(|orbit| {
                period_ok(orbit.period)
                    && orbit
                        .center
                        .iter()
                        .all(|value| (-24.0..=24.0).contains(value))
            })
            && self.bob.as_ref().is_none_or(|bob| {
                period_ok(bob.period) && bob.offset.iter().all(|value| (-4.0..=4.0).contains(value))
            })
            && self.pulse.as_ref().is_none_or(|pulse| {
                period_ok(pulse.period) && (0.0..=0.5).contains(&pulse.amplitude)
            })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureSpin {
    pub axis: SceneAxis,
    pub period: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigurePose {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
    #[serde(default)]
    pub step: usize,
    #[serde(default)]
    pub translate: [f64; 3],
    #[serde(default)]
    pub rotate: [f64; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glow: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureCut {
    pub axis: SceneAxis,
    pub at: f64,
    #[serde(default)]
    pub keep_positive: bool,
    #[serde(default)]
    pub cap: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureAround {
    pub axis: SceneAxis,
    pub center: [f64; 3],
    pub count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureRepeat {
    pub count: usize,
    pub offset: [f64; 3],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FigureShape {
    Group,
    Sleeve {
        radius: f64,
        inner_radius: f64,
        height: f64,
        bevel: Option<f64>,
        segments: Option<usize>,
        arc: Option<f64>,
    },
    Surface {
        grid: Vec<Vec<[f64; 3]>>,
    },
    Blob {
        #[serde(default)]
        field: Vec<FigureField>,
        #[serde(default)]
        blend: f64,
        radius: Option<f64>,
        lumps: Option<f64>,
        seed: Option<u32>,
    },
    Box {
        size: [f64; 3],
        bevel: Option<f64>,
    },
    Sphere {
        radius: f64,
        #[serde(default)]
        dome: bool,
        segments: Option<usize>,
    },
    Arrow {
        path: Vec<[f64; 3]>,
        radius: f64,
        head_radius: f64,
        head_length: f64,
    },
    Poly {
        solid: FigureSolid,
        radius: f64,
    },
    Cylinder {
        radius: f64,
        top_radius: Option<f64>,
        height: f64,
        arc: Option<f64>,
        bevel: Option<f64>,
        segments: Option<usize>,
        #[serde(default)]
        open: bool,
    },
    Torus {
        radius: f64,
        thickness: f64,
        arc: Option<f64>,
        segments: Option<usize>,
        radial_segments: Option<usize>,
    },
    Capsule {
        radius: f64,
        length: f64,
    },
    Gear {
        root_radius: f64,
        radius: f64,
        depth: f64,
        teeth: usize,
        bore: Option<f64>,
        bevel: Option<f64>,
    },
    Spring {
        radius: f64,
        wire: f64,
        height: f64,
        turns: usize,
    },
    Extrude {
        outline: Vec<[f64; 2]>,
        depth: f64,
        #[serde(default)]
        holes: Vec<Vec<[f64; 2]>>,
        bevel: Option<f64>,
    },
    Lathe {
        profile: Vec<[f64; 2]>,
        #[serde(skip_serializing_if = "Option::is_none")]
        segments: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        arc: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        bevel: Option<f64>,
    },
    Tube {
        path: Vec<[f64; 3]>,
        radius: f64,
        #[serde(default)]
        closed: bool,
        segments: Option<usize>,
        radial_segments: Option<usize>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FigureField {
    Ball {
        center: [f64; 3],
        radius: f64,
    },
    Stick {
        from: [f64; 3],
        to: [f64; 3],
        radius: f64,
        to_radius: Option<f64>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub enum FigureSolid {
    #[serde(rename = "tetrahedron")]
    Tetrahedron,
    #[serde(rename = "cube")]
    Cube,
    #[serde(rename = "octahedron")]
    Octahedron,
    #[serde(rename = "dodecahedron")]
    Dodecahedron,
    #[serde(rename = "icosahedron")]
    Icosahedron,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FigureMaterial {
    Paint,
    Metal,
    Rubber,
    Glass,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Primary,
    Neutral,
    Info,
    Success,
    Warning,
    Danger,
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Violet,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChecklistState {
    Todo,
    InProgress,
    Done,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeyValueItem {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChecklistItem {
    pub text: String,
    pub state: ChecklistState,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineItem {
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub state: Option<ChecklistState>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BarItem {
    pub label: String,
    pub value: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MetricItem {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StatusItem {
    pub label: String,
    #[serde(default)]
    pub tone: Option<Tone>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonSide {
    pub label: String,
    pub text: String,
    #[serde(default)]
    pub tone: Option<Tone>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HeatmapRow {
    pub label: String,
    pub values: Vec<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CrossSectionLayer {
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapNode {
    pub id: String,
    pub label: String,
    pub detail: String,
    #[serde(default)]
    pub symbol: Option<MapSymbol>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MapSymbol {
    Module,
    Storage,
    Processor,
    Person,
    Energy,
    Cloud,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapLink {
    pub from: String,
    pub to: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneShape {
    Box,
    Cylinder,
    Sphere,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneAxis {
    X,
    Y,
    Z,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SceneTransform {
    pub position: [f64; 3],
    pub rotation: [f64; 3],
    pub scale: [f64; 3],
}

impl Default for SceneTransform {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneOp {
    Primitive {
        id: String,
        label: String,
        detail: String,
        shape: SceneShape,
        #[serde(default)]
        transform: SceneTransform,
        #[serde(default)]
        attach: Option<SceneAttachment>,
    },
    Extrude {
        id: String,
        label: String,
        detail: String,
        profile: Vec<[f64; 2]>,
        depth: f64,
        #[serde(default)]
        transform: SceneTransform,
        #[serde(default)]
        attach: Option<SceneAttachment>,
    },
    Loft {
        id: String,
        label: String,
        detail: String,
        sections: Vec<SceneLoftSection>,
        #[serde(default)]
        transform: SceneTransform,
        #[serde(default)]
        attach: Option<SceneAttachment>,
    },
    Instance {
        id: String,
        label: String,
        detail: String,
        source: String,
        #[serde(default)]
        mirror: Option<SceneAxis>,
        #[serde(default)]
        transform: SceneTransform,
        #[serde(default)]
        attach: Option<SceneAttachment>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SceneAttachment {
    pub to: String,
    pub target_anchor: [f64; 3],
    pub self_anchor: [f64; 3],
    #[serde(default)]
    pub offset: [f64; 3],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SceneLoftSection {
    pub z: f64,
    pub profile: Vec<[f64; 2]>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SceneContact {
    pub from: String,
    pub to: String,
}

/// Conservative broad-phase diagnostics from the same local axes and transform
/// order as the canvas renderer. Bounds can prove separation, not true contact.
fn figure_part_bounds(part: &FigurePart) -> Option<([f64; 3], [f64; 3])> {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    let mut include = |point: [f64; 3]| {
        for axis in 0..3 {
            low[axis] = low[axis].min(point[axis]);
            high[axis] = high[axis].max(point[axis]);
        }
    };
    match &part.shape {
        FigureShape::Group => return None,
        FigureShape::Surface { grid } => {
            for point in grid.iter().flatten() {
                include(*point);
            }
        }
        FigureShape::Extrude { outline, depth, .. } => {
            for point in outline {
                include([point[0], point[1], -*depth * 0.5]);
                include([point[0], point[1], *depth * 0.5]);
            }
        }
        FigureShape::Lathe { profile, .. } => {
            for point in profile {
                include([point[0].abs(), point[1], 0.0]);
                include([-point[0].abs(), point[1], 0.0]);
                include([0.0, point[1], point[0].abs()]);
                include([0.0, point[1], -point[0].abs()]);
            }
        }
        FigureShape::Tube { path, radius, .. } => {
            for point in path {
                for axis in 0..3 {
                    let mut a = *point;
                    let mut b = *point;
                    a[axis] -= radius;
                    b[axis] += radius;
                    include(a);
                    include(b);
                }
            }
        }
        FigureShape::Arrow {
            path,
            radius,
            head_radius,
            ..
        } => {
            let padding = radius.max(*head_radius);
            for point in path {
                for axis in 0..3 {
                    let mut a = *point;
                    let mut b = *point;
                    a[axis] -= padding;
                    b[axis] += padding;
                    include(a);
                    include(b);
                }
            }
        }
        FigureShape::Blob { field, radius, .. } if !field.is_empty() => {
            for feature in field {
                let (from, to, feature_radius) = match feature {
                    FigureField::Ball { center, radius } => (*center, *center, *radius),
                    FigureField::Stick {
                        from,
                        to,
                        radius,
                        to_radius,
                    } => (
                        *from,
                        *to,
                        radius.max(*to_radius.as_ref().unwrap_or(radius)),
                    ),
                };
                for point in [from, to] {
                    for axis in 0..3 {
                        let mut a = point;
                        let mut b = point;
                        a[axis] -= feature_radius;
                        b[axis] += feature_radius;
                        include(a);
                        include(b);
                    }
                }
            }
        }
        shape => {
            let half = match shape {
                FigureShape::Box { size, .. } => size.map(|value| value.abs() * 0.5),
                FigureShape::Sphere { radius, .. } | FigureShape::Poly { radius, .. } => {
                    [*radius; 3]
                }
                FigureShape::Cylinder {
                    radius,
                    top_radius,
                    height,
                    ..
                } => {
                    let radius = radius.max(*top_radius.as_ref().unwrap_or(radius));
                    [radius, height * 0.5, radius]
                }
                FigureShape::Sleeve { radius, height, .. } => [*radius, *height * 0.5, *radius],
                FigureShape::Torus {
                    radius, thickness, ..
                } => [radius + thickness, radius + thickness, radius + thickness],
                FigureShape::Capsule { radius, length } => {
                    [*radius, length * 0.5 + radius, *radius]
                }
                FigureShape::Gear { radius, depth, .. } => [*radius, *depth * 0.5, *radius],
                FigureShape::Spring {
                    radius,
                    wire,
                    height,
                    ..
                } => [radius + wire, height * 0.5 + wire, radius + wire],
                FigureShape::Blob {
                    radius: Some(radius),
                    ..
                } => [*radius; 3],
                FigureShape::Blob { .. } => return None,
                _ => return None,
            };
            for x in [-half[0], half[0]] {
                for y in [-half[1], half[1]] {
                    for z in [-half[2], half[2]] {
                        include([x, y, z]);
                    }
                }
            }
        }
    }
    if low.iter().any(|value| !value.is_finite()) {
        return None;
    }

    let angles = part.transform.rotation.map(f64::to_radians);
    let (sx, cx) = angles[0].sin_cos();
    let (sy, cy) = angles[1].sin_cos();
    let (sz, cz) = angles[2].sin_cos();
    let mut world_low = [f64::INFINITY; 3];
    let mut world_high = [f64::NEG_INFINITY; 3];
    for x in [low[0], high[0]] {
        for y in [low[1], high[1]] {
            for z in [low[2], high[2]] {
                let x = x * part.transform.scale[0];
                let y = y * part.transform.scale[1];
                let z = z * part.transform.scale[2];
                let y1 = cx * y - sx * z;
                let z1 = sx * y + cx * z;
                let x2 = cy * x + sy * z1;
                let z2 = -sy * x + cy * z1;
                let rotated = [
                    part.transform.position[0] + cz * x2 - sz * y1,
                    part.transform.position[1] + sz * x2 + cz * y1,
                    part.transform.position[2] + z2,
                ];
                for axis in 0..3 {
                    world_low[axis] = world_low[axis].min(rotated[axis]);
                    world_high[axis] = world_high[axis].max(rotated[axis]);
                }
            }
        }
    }
    Some((world_low, world_high))
}

/// Remove explicitly labeled display/staging geometry when the requested subject
/// does not call for a stand or platform. This is applied to restored scenes too,
/// so a previously saved bad scene is repaired when its session is reopened.
pub fn remove_unrequested_staging(presentation: &mut AgentPresentation, request: &str) -> usize {
    use PresentationElement::{Columns, FigureScene, Section};

    let request = request.to_ascii_lowercase();
    let requested_staging = [
        "display base",
        "display stand",
        "display platform",
        "support plane",
        "support platform",
        "mounting base",
        "pedestal",
        "plinth",
        "on a stand",
        "on a platform",
    ]
    .iter()
    .any(|marker| request.contains(marker));
    if requested_staging {
        return 0;
    }

    fn is_staging(part: &FigurePart) -> bool {
        let name = format!("{} {}", part.label, part.detail).to_ascii_lowercase();
        [
            "display base",
            "display stand",
            "display platform",
            "support plane",
            "support platform",
            "mounting base",
            "pedestal",
            "plinth",
        ]
        .iter()
        .any(|marker| name.contains(marker))
    }

    fn visit(elements: &mut [PresentationElement]) -> usize {
        let mut removed = 0;
        for element in elements {
            match element {
                FigureScene { parts, labels, .. } => {
                    let mut removed_ids: std::collections::HashSet<String> = parts
                        .iter()
                        .filter(|part| is_staging(part))
                        .map(|part| part.id.clone())
                        .collect();
                    loop {
                        let before = removed_ids.len();
                        let descendants: Vec<String> = parts
                            .iter()
                            .filter(|part| {
                                part.parent
                                    .as_ref()
                                    .is_some_and(|parent| removed_ids.contains(parent))
                            })
                            .map(|part| part.id.clone())
                            .collect();
                        removed_ids.extend(descendants);
                        if removed_ids.len() == before {
                            break;
                        }
                    }
                    if !removed_ids.is_empty() {
                        let before = parts.len();
                        parts.retain(|part| !removed_ids.contains(&part.id));
                        labels.retain(|label| {
                            label
                                .part
                                .as_ref()
                                .is_none_or(|id| !removed_ids.contains(id))
                        });
                        removed += before - parts.len();
                    }
                }
                Section { elements, .. } => removed += visit(elements),
                Columns { columns } => {
                    for column in columns {
                        removed += visit(column);
                    }
                }
                _ => {}
            }
        }
        removed
    }

    visit(&mut presentation.elements)
}

pub fn scene_diagnostics(presentation: &AgentPresentation) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for element in &presentation.elements {
        if let PresentationElement::FigureScene { parts, .. } = element {
            // Compare parent-local geometry in the same coordinate frame.
            // Only flag clear separation, not overlap/contact accuracy.
            for child in parts.iter().filter(|part| part.parent_local) {
                let Some(parent) = child
                    .parent
                    .as_ref()
                    .and_then(|id| parts.iter().find(|p| &p.id == id))
                else {
                    continue;
                };
                if parent.transform.rotation != [0.; 3] || parent.transform.scale != [1.; 3] {
                    continue;
                }
                let (Some((child_min, child_max)), Some((parent_min, parent_max))) =
                    (figure_part_bounds(child), figure_part_bounds(parent))
                else {
                    continue;
                };
                let gap: f64 = (0..3)
                    .map(|axis| {
                        let low = parent_min[axis] - parent.transform.position[axis];
                        let high = parent_max[axis] - parent.transform.position[axis];
                        (low - child_max[axis])
                            .max(child_min[axis] - high)
                            .max(0.)
                            .powi(2)
                    })
                    .sum::<f64>()
                    .sqrt();
                let size = (0..3)
                    .map(|axis| parent_max[axis] - parent_min[axis])
                    .fold(0., f64::max);
                if gap > size * 0.25 {
                    diagnostics.push(format!("Parent-local part '{}' is separated from parent '{}' by {:.2} coordinate units in the bind pose. Check for world coordinates mistakenly supplied as local offsets; inspect the intended attachment before revising. Parenting alone does not align geometry.", child.id, parent.id, gap));
                }
            }
            // A generated “display base” or support slab is often staging
            // geometry rather than part of the requested object. If it sits
            // through the body's vertical span, send the scene back for repair
            // instead of presenting a visibly bisected model as complete.
            let support_name = |part: &FigurePart| {
                let name = format!("{} {}", part.label, part.detail).to_ascii_lowercase();
                [
                    "display base",
                    "support plane",
                    "support platform",
                    "display stand",
                    "display platform",
                    "mounting base",
                    "pedestal",
                    "plinth",
                ]
                .iter()
                .any(|marker| name.contains(marker))
            };
            let supports: Vec<_> = parts.iter().filter(|part| support_name(part)).collect();
            if !supports.is_empty() {
                let body_bounds: Vec<_> = parts
                    .iter()
                    .filter(|part| !support_name(part))
                    .filter_map(figure_part_bounds)
                    .collect();
                if !body_bounds.is_empty() {
                    let body_min: [f64; 3] = std::array::from_fn(|axis| {
                        body_bounds
                            .iter()
                            .map(|bounds| bounds.0[axis])
                            .reduce(f64::min)
                            .unwrap_or(0.0)
                    });
                    let body_max: [f64; 3] = std::array::from_fn(|axis| {
                        body_bounds
                            .iter()
                            .map(|bounds| bounds.1[axis])
                            .reduce(f64::max)
                            .unwrap_or(0.0)
                    });
                    let body_height = body_max[1] - body_min[1];
                    let bisecting_support = supports.iter().any(|part| {
                        let Some((support_min, support_max)) = figure_part_bounds(part) else {
                            return false;
                        };
                        let support_size = std::array::from_fn::<_, 3, _>(|axis| {
                            support_max[axis] - support_min[axis]
                        });
                        let flat = support_size[1] <= support_size[0].max(support_size[2]) * 0.35;
                        let overlaps_body = (0..3).all(|axis| {
                            support_max[axis].min(body_max[axis])
                                - support_min[axis].max(body_min[axis])
                                > 0.02
                        });
                        flat && overlaps_body
                            && support_max[1] > body_min[1] + body_height * 0.16
                            && support_min[1] < body_max[1] - body_height * 0.16
                    });
                    if body_height > 0.1 && bisecting_support {
                        diagnostics.push("Elevated display/support platform intersects the subject. Remove staging geometry unless the user asked for a stand; keep genuine bases or feet integrated below the object's body, and check the assembled silhouette before finishing.".into());
                    }
                }
            }
        }
        let PresentationElement::ProceduralScene { ops, contacts, .. } = element else {
            continue;
        };
        let mut bounds = std::collections::HashMap::<&str, ([f64; 3], [f64; 3])>::new();
        let mut source_points =
            std::collections::HashMap::<&str, (Vec<[f64; 3]>, Option<SceneAxis>, bool)>::new();
        for op in ops {
            let (id, points, mirror, transform, sphere, attach) = match op {
                SceneOp::Primitive {
                    id,
                    shape,
                    transform,
                    attach,
                    ..
                } => {
                    let points = match shape {
                        SceneShape::Box => [-0.5, 0.5]
                            .into_iter()
                            .flat_map(|x| {
                                [-0.5, 0.5]
                                    .into_iter()
                                    .flat_map(move |y| [-0.5, 0.5].map(move |z| [x, y, z]))
                            })
                            .collect(),
                        SceneShape::Cylinder => (0..32)
                            .flat_map(|i| {
                                let angle = i as f64 * std::f64::consts::TAU / 32.0;
                                [-0.5, 0.5].map(move |z| [0.5 * angle.cos(), 0.5 * angle.sin(), z])
                            })
                            .collect(),
                        SceneShape::Sphere => Vec::new(),
                    };
                    (
                        id.as_str(),
                        points,
                        None,
                        transform,
                        matches!(shape, SceneShape::Sphere),
                        attach,
                    )
                }
                SceneOp::Extrude {
                    id,
                    profile,
                    depth,
                    transform,
                    attach,
                    ..
                } => {
                    let points = profile
                        .iter()
                        .flat_map(|[x, y]| [[*x, *y, -depth / 2.0], [*x, *y, depth / 2.0]])
                        .collect();
                    (id.as_str(), points, None, transform, false, attach)
                }
                SceneOp::Loft {
                    id,
                    sections,
                    transform,
                    attach,
                    ..
                } => {
                    let points = sections
                        .iter()
                        .flat_map(|section| {
                            section
                                .profile
                                .iter()
                                .map(move |[x, y]| [*x, *y, section.z])
                        })
                        .collect();
                    (id.as_str(), points, None, transform, false, attach)
                }
                SceneOp::Instance {
                    id,
                    source,
                    mirror,
                    transform,
                    attach,
                    ..
                } => {
                    let (points, inherited_mirror, sphere) = source_points
                        .get(source.as_str())
                        .cloned()
                        .unwrap_or_default();
                    (
                        id.as_str(),
                        points,
                        mirror.or(inherited_mirror),
                        transform,
                        sphere,
                        attach,
                    )
                }
            };
            source_points.insert(id, (points.clone(), mirror, sphere));
            let mut min = [f64::INFINITY; 3];
            let mut max = [f64::NEG_INFINITY; 3];
            if sphere {
                let radius = transform.scale.into_iter().fold(0.0_f64, f64::max) * 0.5;
                min = transform.position.map(|value| value - radius);
                max = transform.position.map(|value| value + radius);
            } else {
                for mut point in points {
                    if let Some(axis) = mirror {
                        point[match axis {
                            SceneAxis::X => 0,
                            SceneAxis::Y => 1,
                            SceneAxis::Z => 2,
                        }] *= -1.0;
                    }
                    let world = scene_transform_point(point, transform);
                    for axis in 0..3 {
                        min[axis] = min[axis].min(world[axis]);
                        max[axis] = max[axis].max(world[axis]);
                    }
                }
            }
            if let Some(attach) = attach
                && let Some((parent_min, parent_max)) = bounds.get(attach.to.as_str())
            {
                let target = scene_anchor(*parent_min, *parent_max, attach.target_anchor);
                let own = scene_anchor(min, max, attach.self_anchor);
                // Position is an additional offset from the solved attachment.
                // Both Rust diagnostics and the WebView use this same convention.
                let delta = std::array::from_fn::<_, 3, _>(|axis| {
                    target[axis] + attach.offset[axis] + transform.position[axis] - own[axis]
                });
                for axis in 0..3 {
                    min[axis] += delta[axis];
                    max[axis] += delta[axis];
                }
            }
            if min[0].is_finite() {
                bounds.insert(id, (min, max));
            }
        }
        for contact in contacts {
            if let (Some((min_a, max_a)), Some((min_b, max_b))) = (
                bounds.get(contact.from.as_str()),
                bounds.get(contact.to.as_str()),
            ) {
                let gap = (0..3)
                    .map(|axis| {
                        (min_b[axis] - max_a[axis])
                            .max(min_a[axis] - max_b[axis])
                            .max(0.0)
                            .powi(2)
                    })
                    .sum::<f64>()
                    .sqrt();
                if gap > 0.15 {
                    diagnostics.push(format!("Parts '{}' and '{}' have a {:.2} unit bounding gap despite a declared contact.", contact.from, contact.to, gap));
                }
            }
        }
        if ops.len() > 1 {
            let mut reached = std::collections::HashSet::new();
            if let Some(first) = ops.first() {
                reached.insert(scene_op_id(first));
            }
            for _ in 0..ops.len() {
                for op in ops {
                    if let Some(attach) = scene_op_attachment(op)
                        && reached.contains(attach.to.as_str())
                    {
                        reached.insert(scene_op_id(op));
                    }
                }
                for contact in contacts {
                    if reached.contains(contact.from.as_str()) {
                        reached.insert(contact.to.as_str());
                    }
                    if reached.contains(contact.to.as_str()) {
                        reached.insert(contact.from.as_str());
                    }
                }
            }
            let unlinked: Vec<_> = ops
                .iter()
                .map(scene_op_id)
                .filter(|id| !reached.contains(id))
                .take(6)
                .collect();
            if !unlinked.is_empty() {
                diagnostics.push(format!("No declared contact path or attachment path to: {}. Connect these parts or explain that they intentionally float apart.", unlinked.join(", ")));
            }
        }
    }
    diagnostics.truncate(12);
    diagnostics
}

fn scene_anchor(min: [f64; 3], max: [f64; 3], anchor: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| {
        (min[axis] + max[axis]) / 2.0 + anchor[axis] * (max[axis] - min[axis]) / 2.0
    })
}

fn scene_op_attachment(op: &SceneOp) -> Option<&SceneAttachment> {
    match op {
        SceneOp::Primitive { attach, .. }
        | SceneOp::Extrude { attach, .. }
        | SceneOp::Loft { attach, .. }
        | SceneOp::Instance { attach, .. } => attach.as_ref(),
    }
}

fn scene_op_id(op: &SceneOp) -> &str {
    match op {
        SceneOp::Primitive { id, .. }
        | SceneOp::Extrude { id, .. }
        | SceneOp::Loft { id, .. }
        | SceneOp::Instance { id, .. } => id,
    }
}

fn scene_transform_point(point: [f64; 3], transform: &SceneTransform) -> [f64; 3] {
    let [mut x, mut y, mut z] = std::array::from_fn(|axis| point[axis] * transform.scale[axis]);
    let [rx, ry, rz] = transform.rotation.map(f64::to_radians);
    (y, z) = (y * rx.cos() - z * rx.sin(), y * rx.sin() + z * rx.cos());
    (x, z) = (x * ry.cos() + z * ry.sin(), -x * ry.sin() + z * ry.cos());
    (x, y) = (x * rz.cos() - y * rz.sin(), x * rz.sin() + y * rz.cos());
    [
        x + transform.position[0],
        y + transform.position[1],
        z + transform.position[2],
    ]
}

#[cfg(test)]
mod scene_diagnostic_tests {
    use super::*;

    #[test]
    fn latest_session_visual_delimiters_recover_without_losing_elements() {
        {
            let raw = include_str!("test-fixtures/visual-rainbow-delimiters.txt");
            let (presentation, _) = extract_latest(raw);
            let presentation = presentation.expect("complete elements must survive wrapper errors");
            assert!(valid(&presentation));
            assert!(
                presentation.elements.len() >= 2,
                "{}",
                serde_json::to_string(&presentation).unwrap()
            );
        }
    }

    #[test]
    fn delimiter_recovery_rejects_broken_contents_and_truncated_streams() {
        for raw in [
            r#"{"version":1,"elements":[{"type":"text","text":oops}]}"#,
            r#"{"version":1,"elements":[{"type":"text","text":"ok"},garbage]}"#,
            r#"{"version":1,"elements":[{"type":"text","text":"ok"}"#,
        ] {
            assert!(parse_presentation_json(raw).is_none(), "{raw}");
        }
    }

    #[test]
    fn natural_show_me_question_requests_its_own_visual() {
        assert!(VisualPolicy::for_request("how does a rainbow form show me").requests_visual());
    }

    #[test]
    fn explicit_human_figure_request_keeps_the_3d_scene() {
        let scene =
            extract_complete_visual(include_str!("test-fixtures/quality-sol-first-figure.txt"))
                .expect("fixture contains a complete figure scene");
        let policy = VisualPolicy::for_request("Visualize Iron Man in 3D");
        assert!(policy.human_subject);
        assert!(policy.requests_visual());

        let published = apply_visual_policy(scene, &policy).expect("figure should be retained");
        assert!(matches!(
            published.elements.first(),
            Some(PresentationElement::FigureScene { .. })
        ));
    }

    #[test]
    fn human_figure_stays_text_when_visuals_are_not_requested_or_forbidden() {
        let fixture = include_str!("test-fixtures/quality-sol-first-figure.txt");

        let unsolicited = extract_complete_visual(fixture).unwrap();
        let policy = VisualPolicy::for_request("Iron Man character");
        let published = apply_visual_policy(unsolicited, &policy).expect("text fallback remains");
        assert!(matches!(
            published.elements.first(),
            Some(PresentationElement::Text { .. })
        ));

        let text_only = extract_complete_visual(fixture).unwrap();
        let policy = VisualPolicy::for_request("Visualize an Iron Man character, text only");
        assert!(!policy.requests_visual());
        let published = apply_visual_policy(text_only, &policy).expect("text fallback remains");
        assert!(matches!(
            published.elements.first(),
            Some(PresentationElement::Text { .. })
        ));
    }

    #[test]
    fn long_body_does_not_hide_a_detached_part_inside_a_large_bounding_sphere() {
        let scene: AgentPresentation = serde_json::from_str(r#"{"version":1,"elements":[{"type":"procedural_scene","label":"Assembly","ops":[{"op":"primitive","id":"body","label":"Body","detail":"Long housing","shape":"box","transform":{"scale":[10,0.2,0.2]}},{"op":"primitive","id":"wheel","label":"Wheel","detail":"Wheel","shape":"cylinder","transform":{"position":[0,2,0]}}],"contacts":[{"from":"body","to":"wheel"}]}]}"#).unwrap();
        let diagnostics = scene_diagnostics(&scene);
        assert!(
            diagnostics.iter().any(|item| item.contains("bounding gap")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn missing_contact_path_is_reported_without_claiming_precise_collision() {
        let scene: AgentPresentation = serde_json::from_str(r#"{"version":1,"elements":[{"type":"procedural_scene","label":"Assembly","ops":[{"op":"primitive","id":"a","label":"A","detail":"First","shape":"box"},{"op":"primitive","id":"b","label":"B","detail":"Second","shape":"sphere"}],"contacts":[]}]}"#).unwrap();
        assert!(
            scene_diagnostics(&scene)
                .iter()
                .any(|item| item.contains("No declared contact path"))
        );
    }
}

// A conservative guard complements the provider's semantic routing instructions.
// Keep this on the response, not globally: switching sessions must not reuse the
// most recently submitted request's visual preferences.
#[derive(Clone, Debug, Default)]
pub struct VisualPolicy {
    text_only: bool,
    human_subject: bool,
    organic_subject: bool,
    explicit_visual: bool,
}

impl VisualPolicy {
    pub fn requests_visual(&self) -> bool {
        self.explicit_visual && !self.text_only
    }

    pub fn permits_visual(&self) -> bool {
        !self.text_only
    }

    pub(crate) fn with_requested_visual(mut self) -> Self {
        self.explicit_visual = true;
        self
    }

    pub fn for_request(request: &str) -> Self {
        let lower = request.to_lowercase();
        let words = lower
            .split(|c: char| !c.is_alphanumeric())
            .collect::<Vec<_>>();
        let has = |terms: &[&str]| terms.iter().any(|term| words.contains(term));
        let organic = has(&[
            "heart", "lung", "lungs", "brain", "kidney", "kidneys", "anatomy", "organ", "organs",
            "muscle", "muscles",
        ]);
        Self {
            text_only: [
                "text only",
                "text-only",
                "no visuals",
                "no diagrams",
                "don't draw",
                "do not draw",
                "without diagrams",
                "without visuals",
                "do not visualize",
                "don't visualize",
                "no sketch",
            ]
            .iter()
            .any(|term| lower.contains(term)),
            human_subject: (!organic
                && has(&["human", "humans", "person", "people", "character", "body"]))
                || has(&[
                    "superman",
                    "batman",
                    "superhero",
                    "skeleton",
                    "humanoid",
                    "pose",
                    "poses",
                ])
                || [
                    "iron man",
                    "human figure",
                    "human flying",
                    "person flying",
                    "human movement",
                ]
                .iter()
                .any(|term| lower.contains(term)),
            organic_subject: organic,
            explicit_visual: has(&[
                "visualize",
                "visualise",
                "visulaize",
                "draw",
                "diagram",
                "sketch",
                "illustrate",
                "visually",
                "3d",
                "model",
            ]) || lower.contains("show me")
                || lower.starts_with("show ")
                || words.contains(&"show"),
        }
    }
}

pub fn apply_visual_policy(
    mut presentation: AgentPresentation,
    policy: &VisualPolicy,
) -> Option<AgentPresentation> {
    fn plain_visual(element: &PresentationElement) -> String {
        match element {
            PresentationElement::ProceduralScene { label, ops, .. } => format!(
                "{}\n{}",
                label,
                ops.iter()
                    .map(|op| match op {
                        SceneOp::Primitive { label, detail, .. }
                        | SceneOp::Extrude { label, detail, .. }
                        | SceneOp::Loft { label, detail, .. }
                        | SceneOp::Instance { label, detail, .. } => format!("{label}: {detail}"),
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            PresentationElement::FigureScene { label, parts, .. } => format!(
                "{}\n{}",
                label,
                parts
                    .iter()
                    .map(|part| format!("{}: {}", part.label, part.detail))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            PresentationElement::SystemMap { label, nodes, .. } => format!(
                "{}\n{}",
                label,
                nodes
                    .iter()
                    .map(|node| format!("{}: {}", node.label, node.detail))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            PresentationElement::VisualScene { label, frames } => format!(
                "{}\n{}",
                label,
                frames
                    .iter()
                    .map(|frame| frame.caption.as_deref().unwrap_or(&frame.label))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            _ => String::new(),
        }
        .chars()
        .take(4_000)
        .collect()
    }
    fn route(elements: &mut Vec<PresentationElement>, policy: &VisualPolicy) {
        elements.retain_mut(|element| {
            match element {
                PresentationElement::Section { elements, .. } => {
                    route(elements, policy);
                    return !elements.is_empty();
                }
                PresentationElement::Columns { columns } => {
                    for column in columns.iter_mut() {
                        route(column, policy);
                    }
                    columns.retain(|column| !column.is_empty());
                    return !columns.is_empty();
                }
                _ => {}
            }
            let graphical = matches!(
                element,
                PresentationElement::ProceduralScene { .. }
                    | PresentationElement::FigureScene { .. }
                    | PresentationElement::BlackHole { .. }
                    | PresentationElement::SystemMap { .. }
                    | PresentationElement::VisualScene { .. }
                    | PresentationElement::LineChart { .. }
                    | PresentationElement::BarChart { .. }
                    | PresentationElement::Heatmap { .. }
                    | PresentationElement::CrossSection { .. }
                    | PresentationElement::Waveform { .. }
            );
            let suppress = graphical
                && (policy.text_only
                    || (policy.human_subject && !policy.explicit_visual)
                    || (policy.organic_subject
                        && matches!(
                            element,
                            PresentationElement::ProceduralScene { .. }
                                | PresentationElement::FigureScene { .. }
                                | PresentationElement::VisualScene { .. }
                        )));
            if suppress {
                let text = plain_visual(element);
                if text.trim().is_empty() {
                    return false;
                }
                *element = PresentationElement::Text { text, tone: None };
            }
            true
        });
    }
    route(&mut presentation.elements, policy);
    (!presentation.elements.is_empty()).then_some(presentation)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VisualFrame {
    pub label: String,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub marks: Vec<VisualMark>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VisualMark {
    Dot {
        x: f64,
        y: f64,
        r: f64,
        #[serde(default)]
        tone: Option<Tone>,
    },
    Ring {
        x: f64,
        y: f64,
        r: f64,
        #[serde(default)]
        tone: Option<Tone>,
    },
    Ellipse {
        x: f64,
        y: f64,
        rx: f64,
        ry: f64,
        #[serde(default)]
        tone: Option<Tone>,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        #[serde(default)]
        tone: Option<Tone>,
    },
    Arrow {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        #[serde(default)]
        tone: Option<Tone>,
    },
    Curve {
        x1: f64,
        y1: f64,
        cx: f64,
        cy: f64,
        x2: f64,
        y2: f64,
        #[serde(default)]
        tone: Option<Tone>,
    },
    Label {
        x: f64,
        y: f64,
        text: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureAssemblyPlan {
    pub source: String,
    pub destination: String,
    pub assemblies: Vec<FigureAssemblyRole>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureAssemblyRole {
    pub id: String,
    #[serde(alias = "source_role")]
    pub source: String,
    #[serde(alias = "destination_role")]
    pub destination: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub landmarks: Vec<FigureEndpointLandmark>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_box: Option<FigureEndpointBox>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureEndpointBox {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureEndpointLandmark {
    pub point: [f64; 3],
    pub start: [f64; 3],
    pub end: [f64; 3],
    #[serde(default = "default_figure_landmark_tolerance")]
    pub tolerance: f64,
}

fn default_figure_landmark_tolerance() -> f64 {
    0.05
}

pub(crate) fn valid_assembly_plan_fields(plan: &FigureAssemblyPlan) -> bool {
    if !text_ok(&plan.source, 180)
        || !text_ok(&plan.destination, 180)
        || plan.source.eq_ignore_ascii_case(&plan.destination)
        || !(2..=16).contains(&plan.assemblies.len())
    {
        return false;
    }
    let mut ids = std::collections::HashSet::new();
    plan.assemblies.iter().all(|role| {
        ids.insert(role.id.as_str())
            && text_ok(&role.source, 100)
            && text_ok(&role.destination, 100)
            && (role.landmarks.is_empty()
                || ((2..=3).contains(&role.landmarks.len())
                    && role.landmarks.iter().enumerate().all(|(index, mark)| {
                        (0.01..=0.5).contains(&mark.tolerance)
                            && [mark.point, mark.start, mark.end]
                                .into_iter()
                                .flatten()
                                .all(|value| (-96.0..=96.0).contains(&value))
                            && role.landmarks[..index]
                                .iter()
                                .all(|other| other.point != mark.point)
                    })))
            && role.end_box.as_ref().is_none_or(|bounds| {
                bounds.min.into_iter().zip(bounds.max).all(|(min, max)| {
                    (-96. ..=96.).contains(&min) && (-96. ..=96.).contains(&max) && min < max
                })
            })
    })
}

pub(crate) fn valid_assembly_plan(plan: &FigureAssemblyPlan, parts: &[FigurePart]) -> bool {
    if !valid_assembly_plan_fields(plan) {
        return false;
    }
    let by_id: std::collections::HashMap<_, _> =
        parts.iter().map(|part| (part.id.as_str(), part)).collect();
    let mut root = None;
    let mut populated_groups = std::collections::HashSet::new();
    for mesh in parts
        .iter()
        .filter(|part| !matches!(part.shape, FigureShape::Group | FigureShape::Arrow { .. }))
    {
        let mut ancestor = mesh;
        let mut visited = std::collections::HashSet::new();
        while let Some(parent) = ancestor.parent.as_deref() {
            if !visited.insert(parent) {
                return false;
            }
            let Some(next) = by_id.get(parent).copied() else {
                return false;
            };
            ancestor = next;
            populated_groups.insert(parent);
        }
        if *root.get_or_insert(ancestor.id.as_str()) != ancestor.id {
            return false;
        }
    }
    plan.assemblies.iter().all(|role| {
        populated_groups.contains(role.id.as_str())
            && by_id
                .get(role.id.as_str())
                .is_some_and(|part| matches!(part.shape, FigureShape::Group))
    })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigurePlayback {
    #[serde(default)]
    hold_start: f64,
    #[serde(default)]
    hold_end: f64,
    return_seconds: f64,
}

impl FigurePlayback {
    pub fn valid(&self, forward: f64) -> bool {
        (0.25..=60.).contains(&forward)
            && (0.0..=30.).contains(&self.hold_start)
            && (0.0..=30.).contains(&self.hold_end)
            && (0.25..=60.).contains(&self.return_seconds)
            && self.hold_start + forward + self.hold_end + self.return_seconds <= 90.
    }

    #[cfg(windows)]
    pub fn progress(&self, elapsed: f64, forward: f64) -> f64 {
        let return_start = self.hold_start + forward + self.hold_end;
        let phase = elapsed.rem_euclid(return_start + self.return_seconds);
        if phase < return_start {
            ((phase - self.hold_start) / forward).clamp(0., 1.)
        } else {
            1. - (phase - return_start) / self.return_seconds
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureCamera {
    yaw: Option<f64>,
    pitch: Option<f64>,
    orbit_degrees_per_second: Option<f64>,
}

impl FigureCamera {
    fn valid(&self) -> bool {
        self.yaw.is_none_or(|v| (-360. ..=360.).contains(&v))
            && self.pitch.is_none_or(|v| (-72. ..=72.).contains(&v))
            && self
                .orbit_degrees_per_second
                .is_none_or(|v| (-90. ..=90.).contains(&v))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FigureCue {
    at: f64,
    text: String,
}

fn valid_figure_cues(cues: &[FigureCue]) -> bool {
    cues.len() <= 8
        && cues
            .iter()
            .all(|cue| (0.0..=1.0).contains(&cue.at) && text_ok(&cue.text, 180))
        && cues.windows(2).all(|pair| pair[0].at < pair[1].at)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FigureStreamHeader {
    #[serde(default)]
    connected: bool,
    label: String,
    #[serde(default)]
    assembly_plan: Option<FigureAssemblyPlan>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    playback: Option<FigurePlayback>,
    #[serde(default)]
    camera: Option<FigureCamera>,
    #[serde(default)]
    cues: Vec<FigureCue>,
    #[serde(default)]
    labels: Vec<FigureLabel>,
    #[serde(default)]
    steps: Vec<VisualFrame>,
}

// Recover a missing array terminator immediately before an object close.
// Track strings/escapes so text is never rewritten. Strict deserialization and
// the complete figure validator still decide whether the result is usable.
fn repair_figure_array_closures(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut stack = Vec::new();
    let (mut quoted, mut escaped) = (false, false);
    for ch in raw.chars() {
        if quoted {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        match ch {
            '"' => quoted = true,
            '{' | '[' => stack.push(ch),
            '}' => {
                while stack.last() == Some(&'[') {
                    out.push(']');
                    stack.pop();
                }
                if stack.last() == Some(&'{') {
                    stack.pop();
                }
            }
            ']' if stack.last() == Some(&'[') => {
                stack.pop();
            }
            _ => {}
        }
        out.push(ch);
    }
    out
}

fn parse_figure_header(raw: &str) -> Result<serde_json::Value, serde_json::Error> {
    let raw = raw.trim();
    let mut header: serde_json::Value = serde_json::from_str(raw).or_else(|error| {
        // Session 81 closed the assembly list and its object but omitted
        // the last brace. Accept only a fully parseable program header.
        if error.is_eof() && raw.ends_with("]}") {
            let candidate: serde_json::Value = serde_json::from_str(&format!("{raw}}}"))?;
            if candidate["program"] == true && candidate["assembly_plan"].is_object() {
                return Ok(candidate);
            }
        }
        Err(error)
    })?;
    if header["program"] == true
        && header["assembly_plan"].is_object()
        && header["assembly_plan"].get("assemblies").is_none()
        && header["assemblies"].is_array()
        && let Some(fields) = header.as_object_mut()
        && let Some(assemblies) = fields.remove("assemblies")
    {
        fields.get_mut("assembly_plan").expect("checked object")["assemblies"] = assemblies;
    }
    // The label is display text. A long title should not discard valid geometry.
    if let Some(label) = header.get_mut("label")
        && let Some(text) = label.as_str()
        && text.chars().count() > MAX_FIGURE_LABEL_CHARS
    {
        *label = serde_json::Value::String(bounded_figure_label(text));
    }
    // A source-pose description is caption data, not an assembly-plan field.
    // Normalize only this observed alias; all other schema checks remain strict.
    if let Some(steps) = header
        .get_mut("steps")
        .and_then(serde_json::Value::as_array_mut)
    {
        for step in steps {
            if let Some(fields) = step.as_object_mut()
                && fields
                    .get("source")
                    .is_some_and(serde_json::Value::is_string)
            {
                let source = fields.remove("source").expect("checked source");
                fields.entry("caption").or_insert(source);
            }
        }
    }
    Ok(header)
}

fn normalize_figure_part(value: &mut serde_json::Value, steps: &[VisualFrame]) -> Option<()> {
    let fields = value.as_object_mut()?;
    let id = fields.get("id").and_then(serde_json::Value::as_str)?;
    // Models often stream compact geometry records without prose metadata.
    // Derive only display text from a bounded ID; geometry still passes the
    // same strict FigurePart deserializer and validator below.
    let fallback = id.replace(['_', '-'], " ");
    if !fields.contains_key("label") {
        fields.insert("label".into(), serde_json::Value::String(fallback.clone()));
    }
    if !fields.contains_key("detail") {
        let label = fields
            .get("label")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&fallback)
            .to_owned();
        fields.insert("detail".into(), serde_json::Value::String(label));
    }
    if fields
        .get("shape")
        .and_then(|shape| shape.get("kind"))
        .and_then(serde_json::Value::as_str)
        == Some("group")
        && !fields.contains_key("color")
    {
        // A transform-only group has no visible material.
        fields.insert("color".into(), serde_json::Value::String("#FFFFFF".into()));
    }
    if let Some(shape) = fields
        .get_mut("shape")
        .and_then(serde_json::Value::as_object_mut)
        && shape.get("kind").and_then(serde_json::Value::as_str) == Some("box")
        && let (Some(size), Some(bevel)) = (
            shape.get("size").and_then(serde_json::Value::as_array),
            shape.get("bevel").and_then(serde_json::Value::as_f64),
        )
        && size.len() == 3
        && bevel.is_finite()
        && bevel >= 0.0
    {
        let minimum = size
            .iter()
            .filter_map(serde_json::Value::as_f64)
            .fold(f64::INFINITY, f64::min);
        if minimum.is_finite() && minimum > 0.0 && bevel > minimum * 0.45 {
            // Thin glass and trim often arrive with a bevel wider than
            // the thickness. Bound the radius instead of losing the rest
            // of a streamed model; the shape still passes full validation.
            shape.insert("bevel".into(), serde_json::json!(minimum * 0.45));
        }
    }
    let default_pivot = fields
        .get("transform")
        .and_then(|transform| transform.get("position"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!([0, 0, 0]));
    if let Some(motion) = fields
        .get_mut("motion")
        .and_then(serde_json::Value::as_object_mut)
        && !motion.contains_key("pivot")
    {
        motion.insert("pivot".into(), default_pivot);
    }
    if let Some(keys) = fields
        .get_mut("motion")
        .and_then(|motion| motion.get_mut("keys"))
        .and_then(serde_json::Value::as_array_mut)
    {
        // Extra pose keys beyond the declared stages cannot be played.
        // Keep the valid prefix; never extend a scene with phantom stages.
        keys.retain(|key| {
            key.get("step")
                .and_then(serde_json::Value::as_u64)
                .is_none_or(|step| step < steps.len() as u64)
        });
    }
    normalize_motion_channels(value);
    Some(())
}

fn extract_figure_stream(body: &str, closed: bool) -> Option<AgentPresentation> {
    if parse_figure_header(body.lines().next()?)
        .ok()?
        .get("program")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        let compiled = figure_program::compile(body, closed).ok()?;
        let scene = extract_figure_stream(&compiled, true)?;
        let PresentationElement::FigureScene {
            parts, duration, ..
        } = scene.elements.first()?
        else {
            return None;
        };
        return (parts.len() + 1 == compiled.lines().count()
            && duration.is_some_and(|v| (0.25..=60.).contains(&v)))
        .then_some(scene);
    }
    let mut lines = body.split_inclusive('\n');
    let first = lines.next()?;
    let mut header_value: serde_json::Value = serde_json::from_str(first).ok()?;
    // Some providers interpret `steps` as a stage count. Preserve the safe,
    // bounded count as empty stages rather than discarding every streamed part.
    if let Some(count) = header_value
        .get("steps")
        .and_then(serde_json::Value::as_u64)
        && (1..=4).contains(&count)
    {
        let stages: Vec<_> = (1..=count)
            .map(|index| serde_json::json!({"label": format!("Stage {index}"), "marks": []}))
            .collect();
        header_value["steps"] = serde_json::Value::Array(stages);
    }
    if let Some(stages) = header_value
        .get_mut("steps")
        .and_then(serde_json::Value::as_array_mut)
    {
        for stage in stages {
            if let Some(fields) = stage.as_object_mut()
                && !fields.contains_key("marks")
            {
                // A caption-only stage is still a valid pose beat. Keep the
                // strict mark validator for any marks the provider supplies.
                fields.insert("marks".into(), serde_json::json!([]));
            }
        }
    }
    let header: FigureStreamHeader = serde_json::from_value(header_value).ok()?;
    if !text_ok(&header.label, MAX_FIGURE_LABEL_CHARS)
        || !valid_figure_label_fields(&header.labels)
        || !valid_figure_cues(&header.cues)
        || header.camera.as_ref().is_some_and(|camera| !camera.valid())
        || header.playback.as_ref().is_some_and(|playback| {
            header.steps.len() < 2
                || !header
                    .duration
                    .is_some_and(|duration| playback.valid(duration))
        })
    {
        return None;
    }
    let mut parts = Vec::new();
    for line in lines.take(MAX_FIGURE_PARTS) {
        if !closed && !line.ends_with('\n') {
            break;
        }
        if line.len() > 8_000 {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(line)
            .or_else(|_| serde_json::from_str(&repair_figure_array_closures(line)))
        else {
            continue;
        };
        if normalize_figure_part(&mut value, &header.steps).is_none() {
            continue;
        }
        let Ok(part) = serde_json::from_value(value) else {
            continue;
        };
        // Parts are independent records. Reject a bad record without losing
        // later valid siblings; validating the growing graph also rejects a
        // child whose parent was discarded or a duplicate ID.
        let mut candidate = parts.clone();
        candidate.push(part);
        if valid_figure_prefix(&candidate, &header.steps) {
            parts = candidate;
        }
    }
    // Preserve the usable prefix when the tail is incomplete or malformed.
    while !parts.is_empty() {
        if valid_figure(&parts, &header.steps)
            && (header.playback.is_none() || parts.iter().all(|part| part.step == 0))
            && header
                .assembly_plan
                .as_ref()
                .is_none_or(|plan| valid_assembly_plan(plan, &parts))
        {
            let labels = header
                .labels
                .into_iter()
                .filter(|label| {
                    label
                        .part
                        .as_ref()
                        .is_none_or(|id| parts.iter().any(|part| &part.id == id))
                })
                .collect();
            return Some(AgentPresentation {
                version: 1,
                title: None,
                elements: vec![PresentationElement::FigureScene {
                    connected: header.connected,
                    duration: header.duration,
                    playback: header.playback,
                    camera: header.camera,
                    cues: header.cues,
                    assembly_plan: header.assembly_plan,
                    label: header.label,
                    parts,
                    labels,
                    steps: header.steps,
                }],
            });
        }
        parts.pop();
    }
    None
}

#[cfg(test)]
#[test]
fn failed_session_68_near_hidden_head_keeps_all_later_parts() {
    let raw = include_str!("test-fixtures/session68-figure.txt");
    let (presentation, _) = extract_latest(raw);
    let Some(PresentationElement::FigureScene { parts, steps, .. }) = presentation
        .as_ref()
        .and_then(|presentation| presentation.elements.first())
    else {
        panic!("session 68 must recover its figure");
    };
    assert_eq!(parts.len(), 22);
    assert_eq!(steps.len(), 3);
    assert!(parts.iter().any(|part| part.id == "rear_wheel_r"));
}

#[cfg(test)]
#[test]
fn figure_stream_skips_invalid_record_and_preserves_independent_siblings() {
    let header = r#"{"label":"Assembly"}"#;
    let base = r##"{"id":"base","shape":{"kind":"box","size":[1,1,1]},"color":"#123456"}"##;
    let invalid = r##"{"id":"bad","shape":{"kind":"box","size":[-1,1,1]},"color":"#123456"}"##;
    let orphan = r##"{"id":"orphan","parent":"bad","shape":{"kind":"box","size":[1,1,1]},"color":"#123456"}"##;
    let sibling = r##"{"id":"sibling","shape":{"kind":"box","size":[1,1,1]},"color":"#123456"}"##;
    let raw = format!(
        "```island-figure\n{header}\n{base}\n{invalid}\n{orphan}\n{{broken JSON\n{sibling}\n```\n"
    );
    let (presentation, _) = extract_latest(&raw);
    let Some(PresentationElement::FigureScene { parts, .. }) = presentation
        .as_ref()
        .and_then(|presentation| presentation.elements.first())
    else {
        panic!("valid siblings must survive invalid records");
    };
    assert_eq!(
        parts
            .iter()
            .map(|part| part.id.as_str())
            .collect::<Vec<_>>(),
        ["base", "sibling"]
    );
}

#[cfg(test)]
#[test]
fn failed_session_67_caption_only_stages_recover_all_parts() {
    let raw = include_str!("test-fixtures/session67-figure.txt");
    let (presentation, prose) = extract_latest(raw);
    let Some(PresentationElement::FigureScene { parts, steps, .. }) = presentation
        .as_ref()
        .and_then(|presentation| presentation.elements.first())
    else {
        panic!("caption-only stage header must not discard the figure");
    };
    assert_eq!(parts.len(), 17);
    assert_eq!(steps.len(), 3);
    assert!(steps.iter().all(|step| step.marks.is_empty()));
    assert!(prose.contains("Optimus Prime"));
}

#[cfg(test)]
#[test]
fn partial_visual_fence_does_not_flash_as_prose() {
    assert_eq!(extract_latest("```island").1, "");
    assert_eq!(extract_latest("~~~island-fig").1, "");
    assert_eq!(extract_latest("```rust").1, "```rust");
}
#[cfg(test)]
#[test]
fn figure_stream_checks_motion_keys_against_its_stages() {
    let header = r#"{"label":"Hinged chassis","steps":[{"label":"Truck","marks":[]},{"label":"Fold","marks":[]}]}"#;
    let part = r##"{"id":"chassis","label":"Chassis","detail":"Folds around the cab hinge","shape":{"kind":"box","size":[3,1,2]},"color":"#253342","motion":{"pivot":[1,0,0],"keys":[{"step":1,"rotate":[0,0,90]}]}}"##;
    let figure = format!("{header}\n{part}\n");
    assert!(extract_figure_stream(&figure, true).is_some());
    assert!(extract_figure_stream(&figure.replace("\"step\":1", "\"step\":2"), true).is_none());
}

#[cfg(test)]
#[test]
fn figure_stream_keeps_transform_group_with_extra_pose_keys() {
    let header = r#"{"label":"Robot to truck","steps":[{"label":"Robot","marks":[]},{"label":"Truck","marks":[]}]}"#;
    let group = r##"{"id":"torso","label":"Torso","detail":"Cab fold","shape":{"kind":"group"},"motion":{"pivot":[0,1,0],"keys":[{"step":1,"rotate":[90,0,0]},{"step":2,"rotate":[90,0,0]}]}}"##;
    let body = r##"{"id":"cab","label":"Cab","detail":"Truck front","shape":{"kind":"box","size":[2,1,1]},"color":"#CC3322","parent":"torso"}"##;
    let stream = format!("```island-figure\n{header}\n{group}\n{body}\n");
    let (presentation, _) = extract_latest(&stream);
    let Some(PresentationElement::FigureScene { parts, .. }) = presentation
        .as_ref()
        .and_then(|presentation| presentation.elements.first())
    else {
        panic!("the complete transform group must remain visible");
    };
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].motion.as_ref().unwrap().keys.len(), 1);
    assert_eq!(parts[0].color, "#FFFFFF");
}
#[cfg(test)]
#[test]
fn figure_stream_repairs_thin_panels_and_missing_motion_pivots() {
    let header = r#"{"label":"Truck to robot","steps":[{"label":"Truck","marks":[]},{"label":"Robot","marks":[]}]}"#;
    let panel = r##"{"id":"windshield","label":"Windshield","detail":"Thin glass","shape":{"kind":"box","size":[2,0.8,0.06],"bevel":0.04},"transform":{"position":[0,1,0]},"color":"#88AACC","motion":{"keys":[{"step":1,"rotate":[80,0,0]}]}}"##;
    let body = r##"{"id":"chassis","label":"Chassis","detail":"Frame","shape":{"kind":"box","size":[3,0.5,1]},"color":"#333333"}"##;
    let stream = format!("```island-figure\n{header}\n{panel}\n{body}\n");
    let (presentation, _) = extract_latest(&stream);
    let Some(PresentationElement::FigureScene { parts, .. }) = presentation
        .as_ref()
        .and_then(|presentation| presentation.elements.first())
    else {
        panic!("a thin panel must not discard the following body");
    };
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].motion.as_ref().unwrap().pivot, [0.0, 1.0, 0.0]);
    let FigureShape::Box {
        bevel: Some(bevel), ..
    } = parts[0].shape
    else {
        panic!("box expected")
    };
    assert!(bevel < 0.03);
}
#[cfg(test)]
#[test]
fn figure_stream_accepts_bounded_numeric_stage_count_from_live_provider() {
    let header = r#"{"label":"Optimus Prime","steps":1}"#;
    let part = r##"{"id":"torso","label":"Torso","detail":"Red chest armor","shape":{"kind":"box","size":[1.65,1.45,0.72],"bevel":0.08},"transform":{"position":[0,3.25,0]},"color":"#C9342D","material":"paint","step":0}"##;
    let stream = format!("```island-figure\n{header}\n{part}\n");
    let (presentation, _) = extract_latest(&stream);
    let Some(PresentationElement::FigureScene { parts, steps, .. }) = presentation
        .as_ref()
        .and_then(|presentation| presentation.elements.first())
    else {
        panic!("the first streamed part must not be blocked by a numeric stage count");
    };
    assert_eq!(parts.len(), 1);
    assert_eq!(steps.len(), 1);
    assert!(
        extract_latest(&stream.replace("\"steps\":1", "\"steps\":5"))
            .0
            .is_none()
    );
}
#[cfg(test)]
#[test]
fn figure_stream_derives_missing_display_text_without_weakening_geometry_checks() {
    let header = r#"{"label":"Robot schematic"}"#;
    let part = r##"{"id":"chest_blue","shape":{"kind":"box","size":[2,1,1]},"color":"#253342"}"##;
    let stream = format!("```island-figure\n{header}\n{part}\n");
    let (presentation, _) = extract_latest(&stream);
    let Some(PresentationElement::FigureScene { parts, .. }) = presentation
        .as_ref()
        .and_then(|presentation| presentation.elements.first())
    else {
        panic!("a complete first part should be visible before the closing fence");
    };
    assert_eq!(parts[0].label, "chest blue");
    assert_eq!(parts[0].detail, "chest blue");
    assert!(
        extract_latest(&stream.replace("#253342", "invalid"))
            .0
            .is_none()
    );
}

#[cfg(test)]
#[test]
fn figure_stream_publishes_only_validated_complete_parts() {
    let header = "{\"label\":\"Sample assembly\"}";
    let part = r##"{"id":"base","label":"Base","detail":"Support","shape":{"kind":"box","size":[3,1,2]},"color":"#253342"}"##;
    let stream = format!("Assembly\n```island-figure\n{header}\n{part}\n{{\"id\":");
    let (presentation, prose) = extract_latest(&stream);
    let presentation = presentation.expect("the first complete part is live");
    assert_eq!(prose, "Assembly");
    let PresentationElement::FigureScene { parts, .. } = &presentation.elements[0] else {
        panic!("expected figure scene");
    };
    assert_eq!(parts.len(), 1);
    let duplicate = format!("\n```island-figure\n{header}\n{part}\n{part}\n```\nDone");
    let (presentation, prose) = extract_latest(&duplicate);
    assert_eq!(prose, "Done");
    let presentation = presentation.unwrap();
    let PresentationElement::FigureScene { parts, .. } = &presentation.elements[0] else {
        panic!("expected figure scene");
    };
    assert_eq!(parts.len(), 1);
}

/// Compatibility helper used by integration tests that include this module.
#[cfg(test)]
#[allow(
    dead_code,
    reason = "Used by integration tests, not by the application's own test module"
)]
pub fn extract(raw: &str) -> Option<(AgentPresentation, String)> {
    extract_mode(raw, true).map(|(presentation, prose, _)| (presentation, prose))
}

fn extract_mode(raw: &str, allow_partial: bool) -> Option<(AgentPresentation, String, bool)> {
    let mut offset = 0;
    let lines: Vec<_> = raw.split_inclusive('\n').collect();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        let marker = if trimmed.starts_with("```") {
            "```"
        } else if trimmed.starts_with("~~~") {
            "~~~"
        } else {
            ""
        };
        let language = trimmed.strip_prefix(marker).unwrap_or("").trim();
        if !marker.is_empty() && language.eq_ignore_ascii_case("island-figure") {
            let start = offset;
            let body_start = offset + line.len();
            offset = body_start;
            index += 1;
            while index < lines.len() && lines[index].trim() != marker {
                offset += lines[index].len();
                index += 1;
            }
            let closed = index < lines.len();
            if (closed || allow_partial)
                && offset - body_start <= MAX_BLOCK_BYTES
                && let Some(presentation) = extract_figure_stream(&raw[body_start..offset], closed)
            {
                let after = if closed {
                    offset + lines[index].len()
                } else {
                    offset
                };
                return Some((
                    presentation,
                    format!(
                        "{}\n\n{}",
                        raw[..start].trim_end(),
                        raw[after..].trim_start()
                    )
                    .trim()
                    .to_owned(),
                    closed,
                ));
            }
            if closed {
                offset += lines[index].len();
                index += 1;
            }
            continue;
        }
        if marker.is_empty()
            || !["island-ui", "island_ui", "json"]
                .iter()
                .any(|name| language.eq_ignore_ascii_case(name))
        {
            offset += line.len();
            index += 1;
            continue;
        }
        let start = offset;
        let body_start = offset + line.len();
        offset = body_start;
        index += 1;
        while index < lines.len() && lines[index].trim() != marker {
            // ACP models sometimes put the closing fence directly after the
            // JSON, or close a Markdown island-ui block with an XML-style tag.
            // Accept either only when the prefix is a validated presentation.
            let candidate = lines[index].trim_end();
            let closing = [marker, "</island-ui>", "</island_ui>"]
                .into_iter()
                .filter(|tag| *tag == marker || !language.eq_ignore_ascii_case("json"))
                .find_map(|tag| candidate.strip_suffix(tag).map(|prefix| (tag, prefix)));
            if let Some((tag, prefix)) = closing {
                let end = offset + prefix.len();
                if end >= body_start && end - body_start <= MAX_BLOCK_BYTES {
                    let repaired = remove_trailing_json_commas(raw[body_start..end].trim());
                    if let Some(presentation) = parse_presentation_json(&repaired) {
                        let before = raw[..start].trim_end();
                        let after = raw[end + tag.len()..].trim_start();
                        let prose = if before.is_empty() {
                            after.to_owned()
                        } else if after.is_empty() {
                            before.to_owned()
                        } else {
                            format!("{before}\n\n{after}")
                        };
                        return Some((presentation, prose, true));
                    }
                }
            }
            offset += lines[index].len();
            index += 1;
        }
        if index == lines.len() {
            return None;
        }
        let body = &raw[body_start..offset];
        offset += lines[index].len();
        index += 1;
        if body.len() > MAX_BLOCK_BYTES {
            continue;
        }
        let repaired = remove_trailing_json_commas(body.trim());
        let parsed = parse_presentation_json(&repaired);
        if let Some(presentation) = parsed {
            let before = raw[..start].trim_end();
            let after = raw[offset..].trim_start();
            let prose = if before.is_empty() {
                after.to_owned()
            } else if after.is_empty() {
                before.to_owned()
            } else {
                format!("{before}\n\n{after}")
            };
            return Some((presentation, prose, true));
        }
    }
    None
}

/// Review candidates are atomic, even when an initial build can stream prefixes.
pub fn extract_complete_visual(raw: &str) -> Option<AgentPresentation> {
    let mut remaining = raw.to_owned();
    let mut latest = None;
    for _ in 0..8 {
        let Some((presentation, prose, _)) = extract_mode(&remaining, false) else {
            break;
        };
        latest = Some(presentation);
        remaining = prose;
    }
    latest
}

fn compiled_figure_feedback(compiled: &str, header: &FigureStreamHeader) -> String {
    let mut parts = Vec::new();
    for (index, line) in compiled.lines().skip(1).enumerate() {
        let mut value: serde_json::Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => return format!("Compiled record {} is invalid JSON: {error}", index + 1),
        };
        let id = value["id"].as_str().unwrap_or("<missing>").to_owned();
        if normalize_figure_part(&mut value, &header.steps).is_none() {
            return format!("Compiled part {id} has no object or ID");
        }
        let part: FigurePart = match serde_json::from_value(value) {
            Ok(part) => part,
            Err(error) => return format!("Compiled part {id} schema error: {error}"),
        };
        if let Some(repeat) = part.repeat.as_ref()
            && !repeat.offset.iter().any(|offset| offset.abs() >= 0.005)
        {
            return format!(
                "Compiled part {id} repeats copies at the same position. repeat.offset must be nonzero. For radial grip ribs use around:{{axis,center,count}} with the actual axle; preserve the full recipe and repair this placement."
            );
        }
        parts.push(part);
        if !valid_figure_prefix(&parts, &header.steps) {
            return format!(
                "Compiled part {id} fails geometry, hierarchy or motion bounds at record {}. Preserve later records; repair this part and its parent rather than returning the displayed prefix.",
                index + 1
            );
        }
    }
    "The compiled construction program failed header or final scene validation. Check labels, camera, playback and assembly endpoints.".into()
}

pub fn figure_program_feedback(body: &str) -> Option<String> {
    if body.len() > MAX_BLOCK_BYTES {
        return None;
    }
    let raw_header = body.lines().next()?;
    let header = match parse_figure_header(raw_header) {
        Ok(header) => header,
        Err(error) => {
            return Some(format!(
                "Figure header JSON error at column {}: {error}",
                error.column()
            ));
        }
    };
    if header.get("program").and_then(serde_json::Value::as_bool) != Some(true) {
        return None;
    }
    Some(match figure_program::compile(body, true) {
        Err(error) => format!("Construction program error: {error}"),
        Ok(compiled) => {
            match serde_json::from_str::<FigureStreamHeader>(compiled.lines().next()?) {
                Err(error) => format!(
                    "Figure header schema error: {error}. Pose steps accept only label, caption and marks. Source/destination assembly roles belong inside assembly_plan, not in pose steps."
                ),
                Ok(header) => compiled_figure_feedback(&compiled, &header),
            }
        }
    })
}

/// Keep the newest complete scene while an ACP answer streams revisions.
/// Existing answer components still use `extract` when a single block is sent.
#[cfg(test)]
pub fn extract_latest(raw: &str) -> (Option<AgentPresentation>, String) {
    let (presentation, prose, _) = extract_latest_with_completion(raw);
    (presentation, prose)
}

pub fn extract_latest_with_completion(raw: &str) -> (Option<AgentPresentation>, String, bool) {
    let mut remaining = raw.to_owned();
    let mut latest = None;
    let mut complete = false;
    for _ in 0..8 {
        let Some((presentation, prose, closed)) = extract_mode(&remaining, true) else {
            break;
        };
        latest = Some(presentation);
        complete = closed;
        remaining = prose;
    }
    // An invalid or unfinished visual frame is not user-facing prose. Keep
    // ordinary code fences intact, but never expose scene JSON as an answer.
    let mut prose = String::with_capacity(remaining.len());
    let mut skipping: Option<&str> = None;
    for line in remaining.split_inclusive('\n') {
        let trimmed = line.trim();
        if let Some(marker) = skipping {
            if trimmed == marker {
                skipping = None;
            }
            continue;
        }
        if !line.ends_with('\n')
            && [
                "```island-ui",
                "```island_ui",
                "```island-figure",
                "~~~island-ui",
                "~~~island_ui",
                "~~~island-figure",
            ]
            .iter()
            .any(|marker| marker.starts_with(trimmed) && trimmed.len() >= 3)
        {
            // The opening fence can arrive token by token. Hide its partial
            // prefix until it is recognizable instead of flashing code in UI.
            continue;
        }
        if ["```island-ui", "```island_ui", "```island-figure"]
            .iter()
            .any(|marker| trimmed.eq_ignore_ascii_case(marker))
        {
            skipping = Some("```");
        } else if ["~~~island-ui", "~~~island_ui", "~~~island-figure"]
            .iter()
            .any(|marker| trimmed.eq_ignore_ascii_case(marker))
        {
            skipping = Some("~~~");
        } else {
            prose.push_str(line);
        }
    }
    (latest, prose, complete)
}

fn parse_presentation_json(raw: &str) -> Option<AgentPresentation> {
    // A provider occasionally inserts a quote and brace between the final
    // element and the root's closing `]}`. This is a narrow repair for that
    // observed suffix, not a general-purpose JSON correction. The repaired
    // value still passes the full schema and graph validation below.
    let repaired_suffix;
    let candidate = if let Some(prefix) = raw.strip_suffix("]}\"}]}") {
        repaired_suffix = [prefix, "]}]}"].concat();
        repaired_suffix.as_str()
    } else if raw.ends_with("]}]}]}")
        && serde_json::from_str::<serde_json::Value>(raw).is_err()
        && serde_json::from_str::<serde_json::Value>(
            &[raw.strip_suffix("]}]}]}")?, "]}}]}"].concat(),
        )
        .is_ok()
    {
        // Observed ACP response: parts was nested inside graph and the graph
        // was closed as an array. Repair only this terminal delimiter pattern;
        // wrapper normalization and the full graph validator still apply.
        repaired_suffix = [raw.strip_suffix("]}]}]}")?, "]}}]}"].concat();
        repaired_suffix.as_str()
    } else {
        raw
    };
    let mut value = match serde_json::from_str::<serde_json::Value>(raw)
        .or_else(|error| {
            recover_closed_root(raw)
                .or_else(|| recover_element_delimiters(raw))
                .ok_or(error)
        })
        .or_else(|_| serde_json::from_str::<serde_json::Value>(candidate))
    {
        Ok(value) => value,
        Err(_) => {
            // Some ACP responses close an otherwise complete island-ui object
            // twice. Consume exactly one JSON value and permit only a short
            // run of surplus closing delimiters after it. Never accept prose,
            // another object, or executable content after the first value.
            recover_element_delimiters(raw).or_else(|| recover_element_delimiters(candidate))?
        }
    };
    normalize_scene_fields(&mut value);
    serde_json::from_value::<AgentPresentation>(value.clone())
        .ok()
        .filter(valid)
        .or_else(|| recover_valid_elements(&value))
}

fn recover_closed_root(raw: &str) -> Option<serde_json::Value> {
    let mut stream = serde_json::Deserializer::from_str(raw).into_iter::<serde_json::Value>();
    let value = stream.next()?.ok()?;
    let tail = raw[stream.byte_offset()..].trim();
    (!tail.is_empty() && tail.len() <= 4 && tail.bytes().all(|byte| matches!(byte, b']' | b'}')))
        .then_some(value)
}

// Recover only separators outside complete JSON elements. Element contents are
// never repaired, and the reconstructed envelope still passes strict validation.
fn recover_element_delimiters(raw: &str) -> Option<serde_json::Value> {
    let key = raw.find("\"elements\"")?;
    let after_key = raw.get(key + "\"elements\"".len()..)?.trim_start();
    let after_colon = after_key.strip_prefix(':')?.trim_start();
    let mut rest = after_colon.strip_prefix('[')?;
    let prefix_end = raw.len() - rest.len();
    let mut envelope: serde_json::Value =
        serde_json::from_str(&format!("{}]}}", &raw[..prefix_end])).ok()?;
    let mut elements = Vec::new();
    let mut surplus = 0;
    let mut terminal_array = false;
    let mut terminal_root = false;
    loop {
        rest = rest.trim_start();
        if rest.starts_with('{') {
            let mut stream =
                serde_json::Deserializer::from_str(rest).into_iter::<serde_json::Value>();
            let element = stream.next()?.ok()?;
            element.get("type").and_then(serde_json::Value::as_str)?;
            rest = &rest[stream.byte_offset()..];
            terminal_array = false;
            terminal_root = false;
            elements.push(element);
            if elements.len() > 32 {
                return None;
            }
        } else if let Some(byte) = rest.bytes().next() {
            if !matches!(byte, b',' | b']' | b'}') {
                return None;
            }
            terminal_array |= byte == b']';
            terminal_root |= terminal_array && byte == b'}';
            surplus += 1;
            if surplus > 2 * elements.len() + 8 {
                return None;
            }
            rest = &rest[1..];
        } else {
            break;
        }
    }
    // Never manufacture truncated streams: a closed root must be present.
    if elements.is_empty() || !terminal_root || !raw.trim_end().ends_with('}') {
        return None;
    }
    envelope["elements"] = serde_json::Value::Array(elements);
    Some(envelope)
}

// Normalize observed provider aliases before the same strict bounds checks.
// Conflicting fields and unknown graph operations still fail validation.
fn normalize_motion_channels(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            let cycle = fields
                .get("motion")
                .and_then(|motion| motion.get("cycle"))
                .is_some_and(|cycle| !cycle.is_null());
            if let Some(keys) = fields
                .get_mut("motion")
                .and_then(|motion| motion.get_mut("keys"))
                .and_then(serde_json::Value::as_array_mut)
            {
                let mut translate = serde_json::json!([0, 0, 0]);
                let mut rotate = serde_json::json!([0, 0, 0]);
                let mut scale = None;
                for key in keys {
                    let Some(fields) = key.as_object_mut() else {
                        continue;
                    };
                    if let Some(value) = fields.get("translate") {
                        translate = value.clone();
                    } else {
                        fields.insert("translate".into(), translate.clone());
                    }
                    if let Some(value) = fields.get("rotate") {
                        rotate = value.clone();
                    } else {
                        fields.insert("rotate".into(), rotate.clone());
                    }
                    if cycle {
                        if let Some(value) = fields.get("scale") {
                            scale = value.as_f64();
                        } else if let Some(value) = scale {
                            fields.insert("scale".into(), value.into());
                        }
                    }
                }
            }
            for value in fields.values_mut() {
                normalize_motion_channels(value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                normalize_motion_channels(value);
            }
        }
        _ => {}
    }
}

fn normalize_scene_fields(value: &mut serde_json::Value) {
    normalize_motion_channels(value);
    let Some(elements) = value
        .get_mut("elements")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    for element in elements {
        let Some(fields) = element.as_object_mut() else {
            continue;
        };
        if fields.get("type").and_then(serde_json::Value::as_str) != Some("procedural_scene") {
            continue;
        }
        if !fields.contains_key("label")
            && let Some(title) = fields.remove("title")
        {
            fields.insert("label".into(), title);
        }
        if !fields.contains_key("ops")
            && let Some(steps) = fields.remove("steps")
        {
            fields.insert("ops".into(), steps);
        }
        let Some(ops) = fields
            .get_mut("ops")
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        for operation in ops {
            let Some(fields) = operation.as_object_mut() else {
                continue;
            };
            if fields.contains_key("op") {
                continue;
            }
            let inferred = if fields
                .get("shape")
                .and_then(serde_json::Value::as_str)
                .is_some()
            {
                Some("primitive")
            } else if fields
                .get("source")
                .and_then(serde_json::Value::as_str)
                .is_some()
            {
                Some("instance")
            } else {
                None
            };
            if let Some(kind) = inferred {
                fields.insert("op".into(), serde_json::Value::String(kind.into()));
            }
        }
    }
}

fn valid(presentation: &AgentPresentation) -> bool {
    if presentation.version != 1
        || presentation.elements.is_empty()
        || presentation.elements.len() > 24
    {
        return false;
    }
    if !optional_text_ok(presentation.title.as_deref(), 100) {
        return false;
    }
    let mut nodes = 0;
    presentation
        .elements
        .iter()
        .all(|element| valid_element(element, 1, &mut nodes))
        && nodes <= MAX_NODES
}

fn valid_scene_part(id: &str, label: &str, detail: &str, transform: &SceneTransform) -> bool {
    text_ok(id, 32)
        && text_ok(label, 60)
        && text_ok(detail, 400)
        && transform
            .position
            .iter()
            .all(|v| v.is_finite() && (-16.0..=16.0).contains(v))
        && transform
            .rotation
            .iter()
            .all(|v| v.is_finite() && (-360.0..=360.0).contains(v))
        && transform
            .scale
            .iter()
            .all(|v| v.is_finite() && (0.05..=16.0).contains(v))
}

fn valid_scene_attachment(
    attachment: &Option<SceneAttachment>,
    earlier: &std::collections::HashSet<&str>,
) -> bool {
    attachment.as_ref().is_none_or(|attach| {
        earlier.contains(attach.to.as_str())
            && attach
                .target_anchor
                .iter()
                .chain(attach.self_anchor.iter())
                .all(|value| value.is_finite() && (-1.0..=1.0).contains(value))
            && attach
                .offset
                .iter()
                .all(|value| value.is_finite() && (-4.0..=4.0).contains(value))
    })
}

fn valid_profile(points: &[[f64; 2]], depth: f64) -> bool {
    if !(3..=32).contains(&points.len()) || !depth.is_finite() || !(0.005..=16.0).contains(&depth) {
        return false;
    }
    let extent = (0..2)
        .map(|axis| {
            let low = points
                .iter()
                .map(|point| point[axis])
                .fold(f64::INFINITY, f64::min);
            let high = points
                .iter()
                .map(|point| point[axis])
                .fold(f64::NEG_INFINITY, f64::max);
            high - low
        })
        .fold(0.0_f64, f64::max);
    // Resource cost depends on vertex count, not a profile's physical size.
    // Small nondegenerate optics, vents and raised seams are valid geometry.
    let min_edge = (extent * 1e-6).max(1e-7);
    let min_area = (extent * extent * 1e-10).max(1e-12);
    let mut twice_area = 0.0;
    for i in 0..points.len() {
        let [x, y] = points[i];
        let [next_x, next_y] = points[(i + 1) % points.len()];
        if ![x, y, next_x, next_y]
            .iter()
            .all(|v| v.is_finite() && (-8.0..=8.0).contains(v))
            || (x - next_x).hypot(y - next_y) < min_edge
        {
            return false;
        }
        twice_area += x * next_y - next_x * y;
    }
    if twice_area.abs() < min_area {
        return false;
    }
    // A crossed outline cannot be triangulated into a solid. Reject it at the
    // gateway so a bad revision never replaces a scene already on screen.
    let orient = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let on_segment = |a: [f64; 2], b: [f64; 2], p: [f64; 2]| {
        p[0] >= a[0].min(b[0]) - 1e-8
            && p[0] <= a[0].max(b[0]) + 1e-8
            && p[1] >= a[1].min(b[1]) - 1e-8
            && p[1] <= a[1].max(b[1]) + 1e-8
    };
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        for j in i + 1..points.len() {
            if j == i + 1 || (i == 0 && j == points.len() - 1) {
                continue;
            }
            let c = points[j];
            let d = points[(j + 1) % points.len()];
            let ab_c = orient(a, b, c);
            let ab_d = orient(a, b, d);
            let cd_a = orient(c, d, a);
            let cd_b = orient(c, d, b);
            if (ab_c.abs() < 1e-8 && on_segment(a, b, c))
                || (ab_d.abs() < 1e-8 && on_segment(a, b, d))
                || (cd_a.abs() < 1e-8 && on_segment(c, d, a))
                || (cd_b.abs() < 1e-8 && on_segment(c, d, b))
                || (ab_c * ab_d < 0.0 && cd_a * cd_b < 0.0)
            {
                return false;
            }
        }
    }
    true
}

fn valid_extrusion_bevel(bevel: Option<f64>, depth: f64) -> bool {
    bevel.is_none_or(|bevel| (0.0..=0.5).contains(&bevel) && bevel * 2.0 < depth)
}

fn valid_holed_profile(outline: &[[f64; 2]], holes: &[Vec<[f64; 2]>], depth: f64) -> bool {
    if holes.len() > 3
        || !valid_profile(outline, depth)
        || holes.iter().any(|hole| !valid_profile(hole, depth))
    {
        return false;
    }
    let contains = |ring: &[[f64; 2]], [x, y]: [f64; 2]| {
        let mut inside = false;
        for (a, b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
            if (a[1] > y) != (b[1] > y) && x < a[0] + (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) {
                inside = !inside;
            }
        }
        inside
    };
    let crosses = |ring: &[[f64; 2]], other: &[[f64; 2]]| {
        let orient = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
            (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        };
        ring.iter()
            .zip(ring.iter().cycle().skip(1))
            .any(|(&a, &b)| {
                other
                    .iter()
                    .zip(other.iter().cycle().skip(1))
                    .any(|(&c, &d)| {
                        (0..2).all(|i| {
                            a[i].min(b[i]) <= c[i].max(d[i]) && c[i].min(d[i]) <= a[i].max(b[i])
                        }) && orient(a, b, c) * orient(a, b, d) <= 0.
                            && orient(c, d, a) * orient(c, d, b) <= 0.
                    })
            })
    };
    holes.iter().enumerate().all(|(index, hole)| {
        contains(outline, hole[0])
            && !crosses(outline, hole)
            && holes[..index].iter().all(|other| {
                !contains(other, hole[0]) && !contains(hole, other[0]) && !crosses(other, hole)
            })
    })
}

fn valid_loft(sections: &[SceneLoftSection]) -> bool {
    if !(2..=12).contains(&sections.len()) {
        return false;
    }
    let count = sections[0].profile.len();
    if !(3..=16).contains(&count) {
        return false;
    }
    let mut orientation = 0.0;
    sections.iter().enumerate().all(|(index, section)| {
        if !section.z.is_finite()
            || !(-8.0..=8.0).contains(&section.z)
            || (index > 0 && section.z - sections[index - 1].z < 0.05)
            || section.profile.len() != count
            || !valid_profile(&section.profile, 1.0)
        {
            return false;
        }
        let signed_area: f64 = section
            .profile
            .iter()
            .enumerate()
            .map(|(i, [x, y])| {
                let [nx, ny] = section.profile[(i + 1) % count];
                x * ny - nx * y
            })
            .sum();
        if index == 0 {
            orientation = signed_area.signum();
        }
        if signed_area.signum() != orientation {
            return false;
        }
        // Matching convex profiles keep every loft cap and side face well-defined.
        (0..count).all(|i| {
            let a = section.profile[i];
            let b = section.profile[(i + 1) % count];
            let c = section.profile[(i + 2) % count];
            let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
            cross * orientation >= -1e-8
        })
    })
}

fn valid_figure_label_fields(labels: &[FigureLabel]) -> bool {
    labels.len() <= 12
        && labels.iter().all(|label| {
            text_ok(&label.text, 72)
                && (label.part.is_some() || label.point.is_some())
                && label
                    .part
                    .as_deref()
                    .is_none_or(|id| text_ok(id, MAX_FIGURE_ID_BYTES))
                && label
                    .point
                    .is_none_or(|point| point.iter().all(|v| v.is_finite() && v.abs() <= 24.0))
        })
}

// A transformation of an existing figure changes the pose and may add attached
// geometry. Reuse saved bind records so a rewrite cannot quietly remodel them.
pub(crate) fn preserve_transformation_source(
    source: &AgentPresentation,
    candidate: &mut AgentPresentation,
) -> bool {
    let Some(PresentationElement::FigureScene {
        parts: original, ..
    }) = source
        .elements
        .iter()
        .find(|element| matches!(element, PresentationElement::FigureScene { .. }))
    else {
        return false;
    };
    let Some(figure) = candidate
        .elements
        .iter_mut()
        .find(|element| matches!(element, PresentationElement::FigureScene { .. }))
    else {
        return false;
    };
    let PresentationElement::FigureScene { parts, .. } = figure else {
        unreachable!()
    };
    let mut authored: std::collections::HashMap<_, _> =
        parts.iter().map(|part| (part.id.as_str(), part)).collect();
    let mut combined = Vec::with_capacity(parts.len().max(original.len()));
    for saved in original {
        let mut part = saved.clone();
        part.motion = authored
            .remove(saved.id.as_str())
            .and_then(|edit| edit.motion.clone());
        part.step = 0;
        combined.push(part);
    }
    // Both validated inputs are parent-first. Saved records come first and any
    // added descendants follow in their authored order.
    combined.extend(
        parts
            .iter()
            .filter(|part| authored.contains_key(part.id.as_str()))
            .cloned(),
    );
    *parts = combined;
    valid_element(figure, 0, &mut 0)
}

fn valid_figure(parts: &[FigurePart], steps: &[VisualFrame]) -> bool {
    valid_figure_parts(parts, steps, false)
}

fn valid_figure_prefix(parts: &[FigurePart], steps: &[VisualFrame]) -> bool {
    valid_figure_parts(parts, steps, true)
}

fn valid_figure_parts(
    parts: &[FigurePart],
    steps: &[VisualFrame],
    allow_meshless_prefix: bool,
) -> bool {
    if !(1..=MAX_FIGURE_PARTS).contains(&parts.len())
        || steps.len() > MAX_FIGURE_STEPS
        || parts[0].step != 0
        || (!allow_meshless_prefix
            && !parts
                .iter()
                .any(|part| !matches!(part.shape, FigureShape::Group)))
    {
        return false;
    }
    let bounded = |v: f64| v.is_finite() && v.abs() <= 24.0;
    let dimension = |v: f64| bounded(v) && v >= 0.005;
    let mut seen = std::collections::HashMap::new();
    let mut instances = 0;
    for part in parts {
        let mut validation_transform = part.transform.clone();
        if matches!(part.shape, FigureShape::Group) {
            // A mirrored reusable assembly is a signed bind frame, not a
            // second mesh or a negative geometry dimension. Both renderers
            // already apply this matrix to its descendants.
            validation_transform.scale = validation_transform.scale.map(f64::abs);
        }
        if part.id.len() > MAX_FIGURE_ID_BYTES
            || !text_ok(&part.id, MAX_FIGURE_ID_BYTES)
            || !valid_scene_part("part", &part.label, &part.detail, &validation_transform)
            || part.step >= steps.len().max(1)
            || part.cut.as_ref().is_some_and(|cut| !bounded(cut.at))
            || part.color.len() != 7
            || !part.color.starts_with('#')
            || !part.color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            return false;
        }
        if part.joint.as_ref().is_some_and(|joint| {
            part.parent.is_none()
                || !joint
                    .anchor
                    .iter()
                    .chain(&joint.parent_anchor)
                    .copied()
                    .all(bounded)
                || !joint.tolerance.is_finite()
                || !(0.001..=0.5).contains(&joint.tolerance)
        }) {
            return false;
        }
        let depth = match part.parent.as_deref() {
            Some(parent) => match seen.get(parent) {
                Some(&(depth, step)) if depth < 5 && step <= part.step => depth + 1,
                _ => return false,
            },
            None => 0,
        };
        if seen.insert(part.id.as_str(), (depth, part.step)).is_some() {
            return false;
        }
        let copies = match (&part.repeat, &part.around) {
            (Some(repeat), None)
                if (2..=12).contains(&repeat.count)
                    && repeat.offset.iter().copied().all(bounded)
                    && repeat.offset.iter().any(|value| value.abs() >= 0.005) =>
            {
                repeat.count
            }
            (None, Some(around))
                if (2..=24).contains(&around.count)
                    && around.center.iter().copied().all(bounded) =>
            {
                around.count
            }
            (None, None) => 1,
            _ => return false,
        };
        if part
            .thick
            .is_some_and(|value| !value.is_finite() || value <= 0.0 || value > 10.0)
        {
            return false;
        }
        if !matches!(part.shape, FigureShape::Group) {
            instances += copies * if part.mirror.is_some() { 2 } else { 1 };
        }
        if instances > MAX_FIGURE_INSTANCES {
            return false;
        }
        if let Some(motion) = &part.motion
            && ((motion.keys.is_empty() && motion.channels.is_empty() && !motion.is_periodic())
                || !motion.periodic_is_valid()
                || !motion.channels_are_valid(steps.len().saturating_sub(1) as f64)
                || (motion.cycle.is_some() && part.step != 0)
                || motion.keys.len() > 64
                || !motion.pivot.iter().copied().all(bounded)
                || motion
                    .keys
                    .windows(2)
                    .any(|pair| pair[0].time() >= pair[1].time())
                || motion.keys.iter().any(|key| {
                    (!key.time().is_finite()
                        || key.time() < part.step as f64
                        || key.time()
                            > if motion.cycle.is_some() {
                                1.
                            } else {
                                steps.len().saturating_sub(1) as f64
                            })
                        || !key.translate.iter().copied().all(bounded)
                        || !key
                            .rotate
                            .iter()
                            .all(|angle| angle.is_finite() && angle.abs() <= 360.0)
                        || key
                            .scale
                            .is_some_and(|scale| !(0.01..=4.0).contains(&scale))
                        || !key.appearance_is_valid()
                }))
        {
            return false;
        }
        let valid_shape = match &part.shape {
            FigureShape::Group => {
                part.mirror.is_none()
                    && part.repeat.is_none()
                    && part.around.is_none()
                    && part.cut.is_none()
                    && part.thick.is_none()
            }
            FigureShape::Box { size, bevel } => {
                size.iter().copied().all(dimension)
                    && bevel.is_none_or(|radius| {
                        radius.is_finite()
                            && radius >= 0.0
                            && size.iter().all(|size| 2.0 * radius <= *size)
                    })
            }
            FigureShape::Poly { radius, .. } => dimension(*radius),
            FigureShape::Sphere {
                radius, segments, ..
            } => dimension(*radius) && segments.is_none_or(|count| (8..=64).contains(&count)),
            FigureShape::Cylinder {
                radius,
                top_radius,
                height,
                arc,
                bevel,
                segments,
                open,
            } => {
                dimension(*radius)
                    && segments.is_none_or(|count| (3..=64).contains(&count))
                    && (!open || bevel.unwrap_or(0.) == 0.)
                    && arc.is_none_or(|arc| (1.0..=360.0).contains(&arc))
                    && dimension(*height)
                    && top_radius.is_none_or(|top| top == 0.0 || dimension(top))
                    && bevel.is_none_or(|edge| {
                        valid_extrusion_bevel(Some(edge), *height)
                            && edge < *radius
                            && top_radius.is_none_or(|top| top == 0.0 || edge < top)
                    })
            }
            FigureShape::Gear {
                root_radius,
                radius,
                depth,
                teeth,
                bore,
                bevel,
            } => {
                dimension(*root_radius)
                    && dimension(*radius)
                    && root_radius < radius
                    && dimension(*depth)
                    && (4..=32).contains(teeth)
                    && valid_extrusion_bevel(*bevel, *depth)
                    && bore.is_none_or(|bore| {
                        dimension(bore) && bore + 2. * bevel.unwrap_or(0.) < *root_radius
                    })
            }
            FigureShape::Spring {
                radius,
                wire,
                height,
                turns,
            } => {
                dimension(*radius)
                    && dimension(*wire)
                    && wire < radius
                    && dimension(*height)
                    && (1..=8).contains(turns)
            }
            FigureShape::Capsule { radius, length } => dimension(*radius) && dimension(*length),
            FigureShape::Torus {
                radius,
                thickness,
                arc,
                segments,
                radial_segments,
            } => {
                dimension(*radius)
                    && segments.is_none_or(|count| (8..=96).contains(&count))
                    && radial_segments.is_none_or(|count| (4..=32).contains(&count))
                    && arc.is_none_or(|arc| (1.0..=360.0).contains(&arc))
                    && dimension(*thickness)
                    && thickness < radius
            }
            FigureShape::Extrude {
                outline,
                depth,
                holes,
                bevel,
            } => {
                valid_extrusion_bevel(*bevel, *depth) && valid_holed_profile(outline, holes, *depth)
            }
            FigureShape::Arrow {
                path,
                radius,
                head_radius,
                head_length,
            } => {
                dimension(*radius)
                    && dimension(*head_radius)
                    && head_radius > radius
                    && dimension(*head_length)
                    && (2..=20).contains(&path.len())
                    && path.iter().flatten().copied().all(bounded)
                    && path.windows(2).all(|pair| pair[0] != pair[1])
            }
            FigureShape::Sleeve {
                radius,
                inner_radius,
                height,
                bevel,
                segments,
                arc,
            } => {
                segments.is_none_or(|count| (8..=64).contains(&count))
                    && arc.is_none_or(|angle| (1.0..=360.0).contains(&angle))
                    && dimension(*radius)
                    && dimension(*inner_radius)
                    && radius - inner_radius >= 0.005
                    && dimension(*height)
                    && bevel.is_none_or(|edge| {
                        valid_extrusion_bevel(Some(edge), *height)
                            && 2.0 * edge < radius - inner_radius
                    })
            }
            FigureShape::Blob {
                field,
                blend,
                radius,
                lumps,
                seed,
            } => {
                (0.0..=2.0).contains(blend)
                    && if field.is_empty() {
                        radius.is_some_and(|radius| (0.05..=8.0).contains(&radius))
                            && lumps.is_none_or(|lumps| (0.0..=0.22).contains(&lumps))
                            && seed.is_none_or(|seed| seed <= 65535)
                            && *blend == 0.0
                    } else {
                        field.len() <= 8
                            && radius.is_none()
                            && lumps.is_none()
                            && seed.is_none()
                            && field.iter().all(|element| match element {
                                FigureField::Ball { center, radius } => {
                                    center.iter().copied().all(bounded)
                                        && (0.05..=8.0).contains(radius)
                                }
                                FigureField::Stick {
                                    from,
                                    to,
                                    radius,
                                    to_radius,
                                } => {
                                    from != to
                                        && from.iter().chain(to).copied().all(bounded)
                                        && (0.05..=8.0).contains(radius)
                                        && to_radius
                                            .is_none_or(|radius| (0.05..=8.0).contains(&radius))
                                }
                            })
                    }
            }
            FigureShape::Surface { grid } => {
                (2..=12).contains(&grid.len())
                    && (2..=12).contains(&grid[0].len())
                    && grid.iter().all(|row| {
                        row.len() == grid[0].len() && row.iter().flatten().copied().all(bounded)
                    })
            }
            FigureShape::Lathe {
                profile,
                segments,
                arc,
                bevel,
            } => {
                (2..=24).contains(&profile.len())
                    && segments.is_none_or(|value| (8..=64).contains(&value))
                    && arc.is_none_or(|value| (1.0..=360.0).contains(&value))
                    && bevel.is_none_or(|value| (0.0..=0.5).contains(&value))
                    && profile
                        .iter()
                        .all(|[radius, y]| *radius >= 0.0 && bounded(*radius) && bounded(*y))
                    && profile.iter().any(|[radius, _]| dimension(*radius))
                    && profile
                        .windows(2)
                        .all(|pair| pair[0][1] <= pair[1][1] && pair[0] != pair[1])
            }
            FigureShape::Tube {
                path,
                radius,
                closed,
                segments,
                radial_segments,
            } => {
                dimension(*radius)
                    && segments.is_none_or(|count| (4..=96).contains(&count))
                    && radial_segments.is_none_or(|count| (4..=32).contains(&count))
                    && (!closed || (path.len() >= 3 && path.first() != path.last()))
                    && (2..=20).contains(&path.len())
                    && path.iter().flatten().copied().all(bounded)
                    && path.windows(2).all(|pair| pair[0] != pair[1])
            }
        };
        if !valid_shape {
            return false;
        }
    }
    steps.iter().all(|step| {
        text_ok(&step.label, 60)
            && optional_text_ok(step.caption.as_deref(), 180)
            && step.marks.len() <= 18
            && step.marks.iter().all(valid_visual_mark)
    })
}

fn valid_element(element: &PresentationElement, depth: usize, nodes: &mut usize) -> bool {
    *nodes += 1;
    if *nodes > MAX_NODES || depth > MAX_DEPTH {
        return false;
    }
    match element {
        PresentationElement::Heading { text, level } => {
            text_ok(text, 180) && level.is_none_or(|value| (1..=3).contains(&value))
        }
        PresentationElement::Text { text, .. } => text_ok(text, MAX_TEXT_BYTES),
        PresentationElement::Badge { label, .. } => text_ok(label, 80),
        PresentationElement::Stat {
            label,
            value,
            detail,
        } => text_ok(label, 80) && text_ok(value, 120) && optional_text_ok(detail.as_deref(), 180),
        PresentationElement::KeyValue { items } => {
            !items.is_empty()
                && items.len() <= 8
                && items
                    .iter()
                    .all(|item| text_ok(&item.key, 80) && text_ok(&item.value, 240))
        }
        PresentationElement::Callout { title, text, .. } => {
            optional_text_ok(title.as_deref(), 100) && text_ok(text, 800)
        }
        PresentationElement::List { items, .. } => {
            !items.is_empty() && items.len() <= 16 && items.iter().all(|item| text_ok(item, 500))
        }
        PresentationElement::Checklist { items } => {
            !items.is_empty()
                && items.len() <= 16
                && items.iter().all(|item| text_ok(&item.text, 300))
        }
        PresentationElement::Table { columns, rows } => {
            !columns.is_empty()
                && columns.len() <= 6
                && rows.len() <= 12
                && columns.iter().all(|cell| text_ok(cell, 80))
                && rows.iter().all(|row| {
                    row.len() == columns.len() && row.iter().all(|cell| text_ok(cell, 300))
                })
        }
        PresentationElement::Code { code, language } => {
            text_ok(code, MAX_TEXT_BYTES) && optional_text_ok(language.as_deref(), 40)
        }
        PresentationElement::Quote { text, attribution } => {
            text_ok(text, 1000) && optional_text_ok(attribution.as_deref(), 120)
        }
        PresentationElement::Progress {
            label,
            value,
            max,
            detail,
        } => {
            text_ok(label, 80)
                && value.is_finite()
                && (0.0..=1_000_000_000_000.0).contains(value)
                && max.is_none_or(|limit| {
                    limit.is_finite()
                        && (0.0..=1_000_000_000_000.0).contains(&limit)
                        && limit > 0.0
                        && *value <= limit
                })
                && optional_text_ok(detail.as_deref(), 180)
        }
        PresentationElement::Timeline { items } => {
            !items.is_empty()
                && items.len() <= 12
                && items.iter().all(|item| {
                    text_ok(&item.label, 120) && optional_text_ok(item.detail.as_deref(), 240)
                })
        }
        PresentationElement::BarChart { label, items } => {
            text_ok(label, 100)
                && !items.is_empty()
                && items.len() <= 8
                && items.iter().all(|item| {
                    text_ok(&item.label, 80)
                        && item.value.is_finite()
                        && (0.0..=1_000_000_000_000.0).contains(&item.value)
                })
        }
        PresentationElement::FileChange { path, summary, .. } => {
            text_ok(path, 500) && optional_text_ok(summary.as_deref(), 300)
        }
        PresentationElement::Section { title, elements } => {
            optional_text_ok(title.as_deref(), 100)
                && !elements.is_empty()
                && elements.len() <= 12
                && elements
                    .iter()
                    .all(|child| valid_element(child, depth + 1, nodes))
        }
        PresentationElement::Columns { columns } => {
            (2..=3).contains(&columns.len())
                && columns.iter().all(|column| {
                    !column.is_empty()
                        && column.len() <= 8
                        && column
                            .iter()
                            .all(|child| valid_element(child, depth + 1, nodes))
                })
        }
        PresentationElement::MetricGrid { items } => {
            (2..=4).contains(&items.len())
                && items.iter().all(|item| {
                    text_ok(&item.label, 80)
                        && text_ok(&item.value, 120)
                        && optional_text_ok(item.detail.as_deref(), 120)
                })
        }
        PresentationElement::StatusStrip { items } => {
            (1..=6).contains(&items.len()) && items.iter().all(|item| text_ok(&item.label, 100))
        }
        PresentationElement::PhaseCard { title, detail, .. } => {
            text_ok(title, 120) && optional_text_ok(detail.as_deref(), 280)
        }
        PresentationElement::Comparison { left, right } => [left, right]
            .iter()
            .all(|side| text_ok(&side.label, 80) && text_ok(&side.text, 320)),
        PresentationElement::BlackHole {} => true,
        PresentationElement::ProceduralScene {
            label,
            ops,
            contacts,
        } => {
            if !text_ok(label, 100) || !(1..=24).contains(&ops.len()) {
                return false;
            }
            let mut seen = std::collections::HashSet::<&str>::new();
            let ops_valid = ops.iter().all(|op| match op {
                SceneOp::Primitive {
                    id,
                    label,
                    detail,
                    shape: _,
                    transform,
                    attach,
                } => {
                    valid_scene_part(id, label, detail, transform)
                        && valid_scene_attachment(attach, &seen)
                        && seen.insert(id)
                }
                SceneOp::Extrude {
                    id,
                    label,
                    detail,
                    profile,
                    depth,
                    transform,
                    attach,
                } => {
                    valid_scene_part(id, label, detail, transform)
                        && valid_profile(profile, *depth)
                        && valid_scene_attachment(attach, &seen)
                        && seen.insert(id)
                }
                SceneOp::Loft {
                    id,
                    label,
                    detail,
                    sections,
                    transform,
                    attach,
                } => {
                    valid_scene_part(id, label, detail, transform)
                        && valid_loft(sections)
                        && valid_scene_attachment(attach, &seen)
                        && seen.insert(id)
                }
                SceneOp::Instance {
                    id,
                    label,
                    detail,
                    source,
                    transform,
                    attach,
                    ..
                } => {
                    if !valid_scene_part(id, label, detail, transform) {
                        return false;
                    }
                    if !seen.contains(source.as_str()) {
                        return false;
                    }
                    if !valid_scene_attachment(attach, &seen) {
                        return false;
                    }
                    seen.insert(id)
                }
            });
            ops_valid
                && contacts.len() <= 24
                && contacts.iter().all(|contact| {
                    contact.from != contact.to
                        && seen.contains(contact.from.as_str())
                        && seen.contains(contact.to.as_str())
                })
        }
        PresentationElement::FigureScene {
            label,
            parts,
            steps,
            labels,
            duration,
            assembly_plan,
            playback,
            camera,
            cues,
            ..
        } => {
            duration.is_none_or(|v| v.is_finite() && (0.25..=60.).contains(&v))
                && text_ok(label, MAX_FIGURE_LABEL_CHARS)
                && valid_figure(parts, steps)
                && assembly_plan
                    .as_ref()
                    .is_none_or(|plan| valid_assembly_plan(plan, parts))
                && valid_figure_label_fields(labels)
                && valid_figure_cues(cues)
                && camera.as_ref().is_none_or(FigureCamera::valid)
                && playback.as_ref().is_none_or(|looping| {
                    steps.len() >= 2
                        && duration.is_some_and(|duration| looping.valid(duration))
                        && parts.iter().all(|part| part.step == 0)
                })
                && labels.iter().all(|label| {
                    label
                        .part
                        .as_ref()
                        .is_none_or(|id| parts.iter().any(|part| &part.id == id))
                })
        }
        PresentationElement::SystemMap {
            label,
            nodes,
            links,
        } => {
            let ids: std::collections::HashSet<_> =
                nodes.iter().map(|node| node.id.as_str()).collect();
            text_ok(label, 100)
                && (2..=8).contains(&nodes.len())
                && (1..=12).contains(&links.len())
                && ids.len() == nodes.len()
                && nodes.iter().all(|node| {
                    text_ok(&node.id, 40) && text_ok(&node.label, 60) && text_ok(&node.detail, 400)
                })
                && links.iter().all(|link| {
                    ids.contains(link.from.as_str())
                        && ids.contains(link.to.as_str())
                        && link.from != link.to
                        && text_ok(&link.label, 80)
                })
        }
        PresentationElement::LineChart {
            label,
            unit,
            points,
        } => {
            text_ok(label, 100)
                && optional_text_ok(unit.as_deref(), 24)
                && (2..=24).contains(&points.len())
                && points.iter().all(|point| {
                    text_ok(&point.label, 60)
                        && point.value.is_finite()
                        && point.value.abs() <= 1_000_000_000_000.0
                })
        }
        PresentationElement::Waveform { label, values } => {
            text_ok(label, 100)
                && (8..=64).contains(&values.len())
                && values
                    .iter()
                    .all(|value| value.is_finite() && (-1.0..=1.0).contains(value))
        }
        PresentationElement::Heatmap {
            label,
            columns,
            rows,
        } => {
            text_ok(label, 100)
                && (2..=8).contains(&columns.len())
                && (2..=8).contains(&rows.len())
                && columns.iter().all(|item| text_ok(item, 40))
                && rows.iter().all(|row| {
                    text_ok(&row.label, 40)
                        && row.values.len() == columns.len()
                        && row
                            .values
                            .iter()
                            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
                })
        }
        PresentationElement::CrossSection { label, layers } => {
            text_ok(label, 100)
                && (2..=6).contains(&layers.len())
                && layers.iter().all(|layer| {
                    text_ok(&layer.label, 70) && optional_text_ok(layer.detail.as_deref(), 180)
                })
        }
        PresentationElement::VisualScene { label, frames } => {
            text_ok(label, 100)
                && (1..=4).contains(&frames.len())
                && frames.iter().all(|frame| {
                    text_ok(&frame.label, 40)
                        && optional_text_ok(frame.caption.as_deref(), 180)
                        && (2..=18).contains(&frame.marks.len())
                        && frame
                            .marks
                            .iter()
                            .filter(|mark| matches!(mark, VisualMark::Label { .. }))
                            .count()
                            <= 5
                        && frame
                            .marks
                            .iter()
                            .any(|mark| !matches!(mark, VisualMark::Label { .. }))
                        && frame.marks.iter().all(valid_visual_mark)
                })
        }
        PresentationElement::Divider {} => true,
    }
}

fn visual_point(x: f64, y: f64) -> bool {
    x.is_finite() && y.is_finite() && (0.0..=100.0).contains(&x) && (0.0..=70.0).contains(&y)
}

fn valid_visual_mark(mark: &VisualMark) -> bool {
    match mark {
        VisualMark::Dot { x, y, r, .. } | VisualMark::Ring { x, y, r, .. } => {
            visual_point(*x, *y) && r.is_finite() && (0.5..=35.0).contains(r)
        }
        VisualMark::Ellipse { x, y, rx, ry, .. } => {
            visual_point(*x, *y)
                && rx.is_finite()
                && (0.5..=48.0).contains(rx)
                && ry.is_finite()
                && (0.5..=34.0).contains(ry)
        }
        VisualMark::Line { x1, y1, x2, y2, .. } | VisualMark::Arrow { x1, y1, x2, y2, .. } => {
            visual_point(*x1, *y1) && visual_point(*x2, *y2)
        }
        VisualMark::Curve {
            x1,
            y1,
            cx,
            cy,
            x2,
            y2,
            ..
        } => visual_point(*x1, *y1) && visual_point(*cx, *cy) && visual_point(*x2, *y2),
        VisualMark::Label { x, y, text } => visual_point(*x, *y) && text_ok(text, 40),
    }
}

// Accept the common trailing-comma mistake without changing commas inside text.
fn remove_trailing_json_commas(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut quoted = false;
    let mut escaped = false;
    let mut chars = raw.chars().peekable();
    while let Some(character) = chars.next() {
        if quoted {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
        } else if character == '"' {
            quoted = true;
            output.push(character);
        } else if character == ',' {
            if !matches!(
                chars.clone().find(|next| !next.is_whitespace()),
                Some(']' | '}')
            ) {
                output.push(character);
            }
        } else {
            output.push(character);
        }
    }
    output
}

// A single unsupported component must not discard valid siblings. Every
// recovered element still goes through the same bounded renderer contract.
fn recover_valid_elements(value: &serde_json::Value) -> Option<AgentPresentation> {
    if value.get("version")?.as_u64()? != 1 {
        return None;
    }
    let candidates = value.get("elements")?.as_array()?;
    if candidates.is_empty() || candidates.len() > 24 {
        return None;
    }
    let title = value
        .get("title")
        .and_then(|item| item.as_str())
        .filter(|title| optional_text_ok(Some(title), 100))
        .map(str::to_owned);
    let elements = candidates
        .iter()
        .filter_map(|candidate| {
            serde_json::from_value::<PresentationElement>(candidate.clone()).ok()
        })
        .filter(|element| {
            let mut nodes = 0;
            valid_element(element, 1, &mut nodes)
        })
        .collect::<Vec<_>>();
    let presentation = AgentPresentation {
        version: 1,
        title,
        elements,
    };
    valid(&presentation).then_some(presentation)
}

// Field limits are user-facing character counts; MAX_TEXT_BYTES remains the
// separate allocation bound for UTF-8 text in every language.
fn text_ok(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && value.len() <= MAX_TEXT_BYTES
}

fn optional_text_ok(value: Option<&str>, max: usize) -> bool {
    value.is_none_or(|text| text.chars().count() <= max && text.len() <= MAX_TEXT_BYTES)
}

impl FigurePose {
    pub fn time(&self) -> f64 {
        self.at.unwrap_or(self.step as f64)
    }
    pub fn appearance_is_valid(&self) -> bool {
        self.tint
            .is_none_or(|tint| tint.iter().all(|channel| (0.0..=1.0).contains(channel)))
            && self.glow.is_none_or(|glow| (0.0..=1.0).contains(&glow))
    }
}

#[cfg(test)]
#[test]
fn session74_missing_array_closures_preserve_complete_assemblies() {
    let (scene, _) = extract_latest(include_str!("test-fixtures/session74-raw.txt"));
    let Some(PresentationElement::FigureScene { parts, .. }) =
        scene.as_ref().and_then(|s| s.elements.first())
    else {
        panic!("missing figure")
    };
    assert_eq!(parts.len(), 22);
    for id in ["head", "rear", "legL", "legR", "bumper"] {
        assert!(parts.iter().any(|p| p.id == id), "lost {id}");
    }
    assert_eq!(
        repair_figure_array_closures(r#"{"text":"}] literal"}"#),
        r#"{"text":"}] literal"}"#
    );
}

#[cfg(test)]
#[test]
fn session75_mixed_coordinates_report_detached_local_details() {
    let scene: AgentPresentation =
        serde_json::from_str(include_str!("test-fixtures/session75-presentation.json")).unwrap();
    let issues = scene_diagnostics(&scene).join("\n");
    assert!(issues.contains("windshield"), "{issues}");
    assert!(issues.contains("world coordinates mistakenly"), "{issues}");
}

#[cfg(test)]
#[test]
fn session76_review_candidates_accept_caption_only_pose_steps() {
    for raw in [
        include_str!("test-fixtures/session76-review-1.json"),
        include_str!("test-fixtures/session76-review-2.json"),
        include_str!("test-fixtures/session76-review-3.json"),
        include_str!("test-fixtures/session76-review-4.json"),
        include_str!("test-fixtures/session76-review-5.json"),
    ] {
        let decoded: AgentPresentation = serde_json::from_str(raw).expect("review schema");
        assert!(valid(&decoded), "review geometry invalid");
        assert!(parse_presentation_json(raw).is_some());
    }
}

#[cfg(test)]
#[test]
fn partial_motion_keys_preserve_channels_and_explicit_zero_resets() {
    let mut value = serde_json::json!({"motion":{"keys":[
        {"step":0,"translate":[0,1,0]},
        {"step":1,"rotate":[0,0,90]},
        {"step":2,"translate":[0,0,0]}
    ]}});
    normalize_motion_channels(&mut value);
    assert_eq!(
        value["motion"]["keys"][1]["translate"],
        serde_json::json!([0, 1, 0])
    );
    assert_eq!(
        value["motion"]["keys"][2]["rotate"],
        serde_json::json!([0, 0, 90])
    );
    assert_eq!(
        value["motion"]["keys"][2]["translate"],
        serde_json::json!([0, 0, 0])
    );
    let once = value.clone();
    normalize_motion_channels(&mut value);
    assert_eq!(value, once);
}

#[cfg(test)]
#[test]
fn scoped_program_streams_groups_reuses_templates_and_compiles_shared_time() {
    let program = r##"```island-figure
{"program":true,"label":"Folding frame","duration":4,"templates":{"panel":[{"op":"part","id":"plate","shape":{"kind":"box","size":[1,1,0.2]},"color":"#223355"}]}}
{"op":"group","id":"frame","transform":{"position":[2,0,0]}}
{"op":"build_many","template":"panel","instances":[{"id":"left","transform":{"position":[-1,0,0]}},{"id":"right","transform":{"position":[1,0,0]}}]}
{"op":"end"}
{"op":"beat","start":0.5,"end":2,"moves":[{"target":"left","rotate":[0,0,90]},{"target":"right","rotate":[0,0,-90]}]}
{"op":"move","target":"left","start":2,"end":4,"translate":[0,1,0]}
```"##;
    let scene = extract_latest(program).0.expect("valid program");
    let PresentationElement::FigureScene {
        parts, duration, ..
    } = &scene.elements[0]
    else {
        panic!("figure expected")
    };
    assert_eq!(*duration, Some(4.));
    assert_eq!(parts.len(), 5);
    assert_eq!(parts[2].id, "left.plate");
    assert_eq!(parts[2].parent.as_deref(), Some("left"));
    assert!(parts[2].parent_local);
    let keys = &parts[1].motion.as_ref().unwrap().keys;
    assert_eq!(
        keys.iter().map(FigurePose::time).collect::<Vec<_>>(),
        vec![0.125, 0.5, 1.]
    );
    assert_eq!(keys[2].rotate, [0., 0., 90.]);
    let right = &parts[3].motion.as_ref().unwrap().keys;
    assert_eq!(
        right.iter().map(FigurePose::time).collect::<Vec<_>>(),
        vec![0.125, 0.5]
    );
    assert_eq!(right[1].rotate, [0., 0., -90.]);
    assert!(
        extract_latest(&program.replace("\"end\":4", "\"end\":5"))
            .0
            .is_none()
    );
    assert!(
        extract_latest(&program.replace("#223355", "invalid"))
            .0
            .is_none()
    );
    let prefix = program.split("{\"op\":\"end\"}").next().unwrap();
    assert!(extract_latest(prefix).0.is_some());
    assert!(extract_complete_visual(prefix).is_none());
    assert!(extract_complete_visual(program).is_some());
}

#[cfg(test)]
#[test]
fn bounded_curves_and_cyclic_keys_compile_in_a_construction_scene() {
    let raw = r##"```island-figure
{"program":true,"label":"Curved gate","steps":[{"label":"Rest"},{"label":"Review"}]}
{"op":"part","id":"gate","color":"#224477","shape":{"kind":"extrude","depth":0.2,"curved_outline":[["M",-1,0],["Q",0,2,1,0],["L",1,-1],["L",-1,-1],["Z"]]},"motion":{"cycle":{"period":4,"ping_pong":true},"keys":[{"at":0,"scale":1.2},{"at":0.5,"rotate":[0,0,45]},{"at":1,"rotate":[0,0,70]}]}}
```"##;
    let figure = extract_complete_visual(raw).expect("compiled curved profile");
    let PresentationElement::FigureScene { parts, .. } = &figure.elements[0] else {
        panic!("expected figure")
    };
    assert!(matches!(&parts[0].shape, FigureShape::Extrude { outline, .. } if outline.len() == 9));
    let motion = parts[0].motion.as_ref().unwrap();
    assert!(motion.is_periodic());
    assert_eq!(motion.keys[1].scale, Some(1.2));
    assert!(
        extract_complete_visual(&raw.replace(r#""at":1,"rotate""#, r#""at":0.9,"rotate""#))
            .is_none()
    );
}

#[cfg(test)]
#[test]
fn session80_first_recovery_expands_a_template_declared_after_its_use() {
    let raw = include_str!("test-fixtures/session80-first-recovery.txt");
    let scene = extract_complete_visual(raw).expect("session 80 recovery should compile");
    let PresentationElement::FigureScene { parts, .. } = &scene.elements[0] else {
        panic!("expected figure")
    };
    let wheel = parts
        .iter()
        .find(|part| part.id == "right_leg.wheel")
        .expect("template wheel");
    assert_eq!(wheel.parent.as_deref(), Some("right_leg"));
    assert!(wheel.parent_local);
}

#[cfg(test)]
#[test]
fn assembly_plan_compiles_a_fixed_hinge_and_an_endpoint_span() {
    let raw = r##"```island-figure
{"program":true,"label":"Folding linkage","duration":2,"assembly_plan":{"source":"Flat linkage","destination":"Raised linkage","assemblies":[{"id":"base","source":"Base","destination":"Support"},{"id":"arm","source":"Flat link","destination":"Raised link"}]}}
{"op":"group","id":"base","transform":{"position":[2,0,0]}}
{"op":"part","id":"foot","shape":{"kind":"box","size":[2,0.4,2]},"color":"#445566"}
{"op":"hinge","id":"arm","at":[0,0.2,0],"axis":"z"}
{"op":"part","id":"shaft","shape":{"kind":"cylinder","radius":0.08},"span":{"from":[0,0,0],"to":[2,1,2]},"color":"#557799"}
{"op":"end"}
{"op":"end"}
{"op":"move","target":"arm","start":0.5,"end":2,"angle":90}
```"##;
    let scene = extract_complete_visual(raw).expect("valid articulated program");
    let PresentationElement::FigureScene {
        parts,
        assembly_plan,
        ..
    } = &scene.elements[0]
    else {
        panic!("expected figure")
    };
    assert_eq!(assembly_plan.as_ref().unwrap().assemblies[1].id, "arm");
    let hinge = &parts[2];
    assert_eq!(hinge.parent.as_deref(), Some("base"));
    let joint = hinge.joint.as_ref().unwrap();
    assert_eq!(joint.anchor, [0.; 3]);
    assert_eq!(joint.parent_anchor, [0., 0.2, 0.]);
    let motion = hinge.motion.as_ref().unwrap();
    assert_eq!(motion.pivot, [0., 0.2, 0.]);
    assert_eq!(motion.keys.last().unwrap().rotate, [0., 0., 90.]);
    let span = &parts[3];
    assert_eq!(span.transform.position, [1., 0.5, 1.]);
    assert!(
        matches!(span.shape, FigureShape::Cylinder { height, .. } if (height - 3.).abs() < 1e-8)
    );
    let [pitch, _, roll] = span.transform.rotation.map(f64::to_radians);
    let axis = [
        -roll.sin() * pitch.cos(),
        roll.cos() * pitch.cos(),
        pitch.sin(),
    ];
    for (actual, expected) in axis.into_iter().zip([2. / 3., 1. / 3., 2. / 3.]) {
        assert!((actual - expected).abs() < 1e-8);
    }
    let aimed = raw.replace(r#""angle":90"#, r#""aim":{"point":[1,0,0],"to":[0,1.2,0]}"#);
    let aimed = extract_complete_visual(&aimed).expect("reachable hinge target");
    let PresentationElement::FigureScene { parts, .. } = &aimed.elements[0] else {
        panic!("expected figure")
    };
    assert_eq!(
        parts[2]
            .motion
            .as_ref()
            .unwrap()
            .keys
            .last()
            .unwrap()
            .rotate,
        [0., 0., 90.]
    );
    let unreachable = raw.replace(r#""angle":90"#, r#""aim":{"point":[1,0,0],"to":[0,2.2,0]}"#);
    assert!(extract_complete_visual(&unreachable).is_none());
    assert!(
        extract_complete_visual(&raw.replace("\"angle\":90", "\"translate\":[1,0,0]")).is_none()
    );
    assert!(
        extract_complete_visual(
            &raw.replace("\"id\":\"arm\",\"source", "\"id\":\"missing\",\"source")
        )
        .is_none()
    );
}

#[cfg(test)]
#[test]
fn session81_initial_and_recovery_headers_preserve_the_validated_assembly() {
    let raw = [
        include_str!("test-fixtures/session81-candidate-0.txt"),
        include_str!("test-fixtures/session81-candidate-1.txt"),
    ];
    for (index, candidate) in raw.into_iter().enumerate() {
        let scene = extract_complete_visual(candidate)
            .unwrap_or_else(|| panic!("session 81 candidate {index} should compile"));
        let PresentationElement::FigureScene {
            parts,
            assembly_plan: Some(plan),
            ..
        } = &scene.elements[0]
        else {
            panic!("expected a validated assembly plan")
        };
        assert!(parts.len() >= 25);
        assert!(plan.assemblies.iter().any(|role| role.id == "left_leg"));
        assert!(
            parts
                .iter()
                .any(|part| part.id == "left_leg_fold" && part.joint.is_some())
        );
    }
}

#[cfg(test)]
#[test]
fn session82_complete_program_keeps_wheels_and_motion_after_streamed_prefix() {
    let raw = include_str!("test-fixtures/session82-candidate-0.txt");
    let scene = extract_complete_visual(raw).expect("session 82 complete program");
    let PresentationElement::FigureScene {
        parts,
        assembly_plan,
        ..
    } = &scene.elements[0]
    else {
        panic!("expected figure")
    };
    assert!(assembly_plan.is_some());
    assert_eq!(parts.len(), 37);
    assert!(parts.iter().any(|part| part.id == "leftWheelTire"));
    let wheel = parts.iter().find(|part| part.id == "leftWheel").unwrap();
    assert!(wheel.joint.is_none());
    assert_eq!(
        wheel
            .motion
            .as_ref()
            .unwrap()
            .keys
            .last()
            .unwrap()
            .translate,
        [-0.5, 0.55, 0.]
    );
    let leg = parts.iter().find(|part| part.id == "leftLeg").unwrap();
    assert!(leg.joint.is_some());
    assert_eq!(
        leg.motion.as_ref().unwrap().keys.last().unwrap().rotate,
        [0., 0., 90.]
    );
    assert!(extract_complete_visual(&raw.replace("[0,0,90]}", "[45,0,90]}")).is_none());
}

#[cfg(test)]
#[test]
fn session85_reports_its_unclosed_scopes_and_session86_keeps_nested_placements() {
    let unclosed = include_str!("test-fixtures/session85-original-figure.txt");
    let fence = unclosed.find("island-figure\n").unwrap() + "island-figure\n".len();
    let body = unclosed[fence..].split("```").next().unwrap();
    let error = figure_program_feedback(body).unwrap();
    assert!(
        error.contains(
            "Unclosed group scopes: root, cab, head, left_leg, right_leg, left_arm, right_arm"
        ),
        "{error}"
    );
    assert!(extract_complete_visual(unclosed).is_none());
    let raw = include_str!("test-fixtures/session86-original-figure.txt");
    let scene = extract_complete_visual(raw).expect("valid geometry with nested placement");
    let PresentationElement::FigureScene { parts, .. } = &scene.elements[0] else {
        panic!("expected figure")
    };
    for id in ["chestWindows", "helmetCrests", "trailerWheels"] {
        let part = parts
            .iter()
            .find(|part| part.id == id)
            .expect("original repeated mesh");
        assert!(part.repeat.is_some());
    }
    assert_eq!(
        parts
            .iter()
            .filter(|part| !matches!(part.shape, FigureShape::Group))
            .count(),
        23
    );
    assert_eq!(parts.iter().filter(|part| part.motion.is_some()).count(), 6);
}

#[cfg(test)]
#[test]
fn program_places_a_group_origin_in_its_parents_frame() {
    let body = r##"{"program":true,"label":"Sliding latch","duration":4}
{"op":"group","id":"base","transform":{"position":[5,0,0]}}
{"op":"group","id":"slider","transform":{"position":[1,2,3]}}
{"op":"part","id":"pin","shape":{"kind":"box","size":[1,1,1]},"color":"#ffffff"}
{"op":"end"}
{"op":"end"}
{"op":"beat","start":0,"end":4,"moves":[{"target":"slider","place":[4,2,1],"rotate":[0,90,0]}]}"##;
    let compiled = figure_program::compile(body, true).unwrap();
    let part: serde_json::Value = serde_json::from_str(compiled.lines().nth(2).unwrap()).unwrap();
    assert_eq!(
        part["motion"]["keys"][1]["translate"],
        serde_json::json!([3., 0., -2.])
    );
    assert_eq!(
        part["motion"]["keys"][1]["rotate"],
        serde_json::json!([0, 90, 0])
    );
    let conflicting = body.replace(
        r#""place":[4,2,1]"#,
        r#""place":[4,2,1],"translate":[0,0,0]"#,
    );
    assert!(
        figure_program::compile(&conflicting, true)
            .unwrap_err()
            .contains("cannot be combined")
    );
}

#[test]
fn session89_keeps_the_plan_when_landmark_tolerances_are_omitted() {
    let raw = include_str!("test-fixtures/session89-original-figure.txt");
    let body = raw
        .split("\n")
        .skip(1)
        .take_while(|line| *line != "```")
        .collect::<Vec<_>>()
        .join("\n");
    let scene = extract_complete_visual(raw)
        .unwrap_or_else(|| panic!("{:?}", figure_program_feedback(&body)));
    let PresentationElement::FigureScene {
        parts,
        assembly_plan: Some(plan),
        ..
    } = &scene.elements[0]
    else {
        panic!("expected a complete figure and assembly plan")
    };
    assert_eq!(parts.len(), 35);
    assert_eq!(parts.iter().filter(|part| part.motion.is_some()).count(), 5);
    assert_eq!(plan.assemblies.len(), 5);
    assert!(
        plan.assemblies
            .iter()
            .flat_map(|role| &role.landmarks)
            .all(|point| point.tolerance == 0.05)
    );
}

#[test]
fn session87_keeps_symmetric_parts_motion_and_annotations() {
    let raw = include_str!("test-fixtures/session87-original-figure.txt");
    let scene = extract_complete_visual(raw).expect("group-local names should compile");
    let PresentationElement::FigureScene {
        parts,
        labels,
        assembly_plan,
        ..
    } = &scene.elements[0]
    else {
        panic!("expected figure")
    };
    assert_eq!(
        parts
            .iter()
            .filter(|part| !matches!(part.shape, FigureShape::Group))
            .count(),
        21
    );
    assert_eq!(parts.iter().filter(|part| part.motion.is_some()).count(), 6);
    assert_eq!(labels.len(), 3);
    assert_eq!(assembly_plan.as_ref().unwrap().assemblies.len(), 4);
    assert!(parts.iter().any(|part| part.id == "root.legs.right.thigh"));
    assert!(parts.iter().any(|part| part.id == "root.arms.right.wheel"));
    assert!(
        extract_complete_visual(&raw.replace(r#""target":"arms.right""#, r#""target":"right""#))
            .is_none()
    );
}

#[cfg(test)]
#[test]
fn phased_playback_and_six_captioned_steps_are_validated() {
    let raw = r##"```island-figure
{"program":true,"label":"Folding linkage","duration":10,"playback":{"hold_start":2,"hold_end":4,"return_seconds":6},"camera":{"yaw":30,"pitch":16,"orbit_degrees_per_second":0},"steps":[{"label":"Start"},{"label":"Release"},{"label":"Fold"},{"label":"Clear"},{"label":"Seat"},{"label":"End"}],"cues":[{"at":0,"text":"Rest"},{"at":0.5,"text":"Swing the linkage"}]}
{"op":"group","id":"base"}
{"op":"part","id":"panel","shape":{"kind":"box","size":[1,1,1]},"color":"#234567"}
{"op":"end"}
{"op":"move","target":"panel","start":2,"end":8,"rotate":[0,0,90]}
```"##;
    let scene = extract_complete_visual(raw).expect("phased playback should compile");
    let PresentationElement::FigureScene {
        steps,
        playback,
        camera,
        cues,
        ..
    } = &scene.elements[0]
    else {
        panic!("expected figure")
    };
    assert_eq!(steps.len(), 6);
    assert_eq!(camera.as_ref().unwrap().pitch, Some(16.));
    assert_eq!(cues.len(), 2);
    assert!(playback.as_ref().unwrap().valid(10.));
    assert!(
        extract_complete_visual(&raw.replace(r#""return_seconds":6"#, r#""return_seconds":0"#))
            .is_none()
    );
    assert!(
        extract_complete_visual(&raw.replace(
            r#""orbit_degrees_per_second":0"#,
            r#""orbit_degrees_per_second":91"#
        ))
        .is_none()
    );
}

#[cfg(all(test, windows))]
#[test]
fn native_phase_progress_matches_the_documented_cycle() {
    let playback = FigurePlayback {
        hold_start: 2.,
        hold_end: 4.,
        return_seconds: 6.,
    };
    for (elapsed, expected) in [
        (0., 0.),
        (1., 0.),
        (2., 0.),
        (7., 0.5),
        (12., 1.),
        (15., 1.),
        (19., 0.5),
        (22., 0.),
    ] {
        assert!((playback.progress(elapsed, 10.) - expected).abs() < 1e-9);
    }
}

#[cfg(test)]
#[test]
fn explicit_group_parents_do_not_require_nested_end_records() {
    let raw = r##"```island-figure
{"program":true,"label":"Bracket"}
{"op":"group","id":"root","scope":false}
{"op":"group","id":"wing","parent":"root","scope":false}
{"op":"part","id":"panel","parent":"root.wing","shape":{"kind":"box","size":[1,1,1]},"color":"#123456"}
{"op":"move","target":"root.wing","start":0,"end":2,"rotate":[0,30,0]}
```"##;
    let scene = extract_complete_visual(raw).expect("explicit parents should compile");
    let PresentationElement::FigureScene { parts, .. } = &scene.elements[0] else {
        panic!("expected figure")
    };
    assert_eq!(parts[2].parent.as_deref(), Some("wing"));
    assert!(parts[2].parent_local);
    assert!(parts[1].motion.is_some());
}

#[cfg(test)]
#[test]
fn session92_exposes_overlap_then_empty_animation_groups() {
    let raw = include_str!("test-fixtures/session92-original-figure.txt");
    let body = raw
        .split("island-figure\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    let overlap = figure_program::compile(body, true).unwrap_err();
    assert!(overlap.contains("target arms"), "{overlap}");
    let sequenced = body.replace(r#""start":1.6,"end":3.8"#, r#""start":2.2,"end":3.8"#);
    let attachment = figure_program::compile(&sequenced, true).unwrap_err();
    assert!(attachment.contains("no mesh descendants"), "{attachment}");
    for id in ["head", "arms", "legs"] {
        assert!(attachment.contains(id), "{attachment}");
    }
}

#[test]
fn session104_saved_model_replay() {
    let raw = include_str!("test-fixtures/session104-original-figure.txt");
    let body = raw
        .split("island-figure")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap()
        .trim();
    let scene = extract_complete_visual(raw)
        .unwrap_or_else(|| panic!("{:?}", figure_program_feedback(body)));
    let PresentationElement::FigureScene { parts, steps, .. } = &scene.elements[0] else {
        panic!("figure expected")
    };
    assert_eq!(parts.len(), 46);
    for expected in [
        "torso_core",
        "helmet",
        "foot_left",
        "foot_right",
        "arm_left_forearm",
        "arm_right_forearm",
    ] {
        assert!(
            parts.iter().any(|part| part.id.ends_with(expected)),
            "missing {expected}"
        );
    }
    assert!(
        parts
            .iter()
            .any(|part| matches!(part.shape, FigureShape::Box { bevel: Some(v), .. } if v > 0.))
    );
    assert!(steps.iter().all(|step| {
        step.caption
            .as_deref()
            .is_some_and(|caption| caption.starts_with("Broad blue shoulders"))
    }));
    if let Ok(path) = std::env::var("PLUTO_REPLAY_OUTPUT") {
        std::fs::write(path, serde_json::to_string(&scene).unwrap()).unwrap();
    }
}

#[test]
fn source_caption_alias_keeps_strict_pose_validation() {
    let raw = include_str!("test-fixtures/session104-original-figure.txt");
    let unknown = raw.replace("\"source\":", "\"unsupported_pose_field\":");
    assert!(extract_complete_visual(&unknown).is_none());
    let too_long = raw.replace("Broad blue shoulders, red cab chest, silver grille abdomen, blue legs, red forearms, and a helmet with twin side fins.", &"x".repeat(181));
    assert!(extract_complete_visual(&too_long).is_none());
}

#[test]
fn session106_followup_recipe_replays() {
    let raw = include_str!("test-fixtures/session106-followup-figure.txt");
    let fence = raw
        .lines()
        .next()
        .unwrap()
        .strip_suffix("island-figure")
        .unwrap();
    let body = raw
        .lines()
        .skip(1)
        .take_while(|line| *line != fence)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        extract_complete_visual(raw).is_some(),
        "{:?}",
        figure_program_feedback(&body)
    );
}

#[test]
fn recent_static_figures_preserve_complete_geometry() {
    for (id, count, raw) in [
        (
            108,
            52,
            include_str!("test-fixtures/session108-original-figure.txt"),
        ),
        (
            109,
            42,
            include_str!("test-fixtures/session109-original-figure.txt"),
        ),
        (
            110,
            31,
            include_str!("test-fixtures/session110-original-figure.txt"),
        ),
    ] {
        let fence = raw
            .lines()
            .next()
            .unwrap()
            .strip_suffix("island-figure")
            .unwrap();
        let body = raw
            .lines()
            .skip(1)
            .take_while(|line| *line != fence)
            .collect::<Vec<_>>()
            .join("\n");
        let scene = extract_complete_visual(raw)
            .unwrap_or_else(|| panic!("session {id}: {:?}", figure_program_feedback(&body)));
        let PresentationElement::FigureScene { parts, .. } = &scene.elements[0] else {
            panic!()
        };
        assert_eq!(parts.len(), count, "session {id} lost geometry");
        if std::env::var_os("PLUTO_EXPORT_REPLAY").is_some() {
            std::fs::write(
                format!("target/session{id}-complete.json"),
                serde_json::to_string(&scene).unwrap(),
            )
            .unwrap();
        }
        if id == 108 {
            let eyes = parts.iter().find(|part| part.id == "eyes").unwrap();
            assert_eq!(eyes.transform.position, [0.0, 0.15, 0.59]);
        }
    }
}

#[cfg(test)]
#[test]
fn session111_full_model_survives_redundant_carrier_annotation() {
    let raw = include_str!("test-fixtures/session111-original-figure.txt");
    let scene = extract_complete_visual(raw).expect("complete session111 figure");
    let PresentationElement::FigureScene { parts, .. } = &scene.elements[0] else {
        panic!("figure")
    };
    assert_eq!(parts.len(), 44);
    for id in [
        "abdomen_shell",
        "left_leg_shell",
        "right_leg_shell",
        "left_hub",
        "right_hub",
    ] {
        assert!(parts.iter().any(|part| part.id == id), "lost {id}");
    }
    assert_eq!(
        parts
            .iter()
            .find(|part| part.id == "abdomen")
            .unwrap()
            .parent
            .as_deref(),
        Some("torso")
    );
    let conflict = raw.replace("\"group\":\"torso\"", "\"group\":\"head\"");
    assert!(extract_complete_visual(&conflict).is_none());
    let malformed = raw.replace("\"group\":\"torso\",", "\"unexpected_field\":true,");
    let body = malformed.split("\n").skip(1).collect::<Vec<_>>().join("\n");
    let feedback = figure_program_feedback(body.split("\x60\x60\x60").next().unwrap()).unwrap();
    assert!(
        feedback.contains("abdomen") && feedback.contains("unexpected_field"),
        "{feedback}"
    );
    std::fs::write(
        "target/session111-complete.json",
        serde_json::to_string(&scene).unwrap(),
    )
    .unwrap();
}

#[cfg(test)]
#[test]
fn session112_exact_recipe_diagnostics() {
    let raw = include_str!("test-fixtures/session112-original-figure.txt");
    let body = raw
        .split("island-figure\n")
        .nth(1)
        .unwrap()
        .split("\x60\x60\x60")
        .next()
        .unwrap()
        .trim();
    let scene = extract_complete_visual(raw);
    assert!(scene.is_some(), "{:?}", figure_program_feedback(body));
}

#[cfg(test)]
#[test]
#[ignore = "local live-provider artifact export; run explicitly with PLUTO_CHECK_QUALITY"]
fn export_live_quality_candidates_through_production_compiler() {
    if std::env::var_os("PLUTO_CHECK_QUALITY").is_none() {
        return;
    }
    for name in ["microscope", "character", "sol"] {
        let path = format!("target/quality-{name}-output.txt");
        if !std::path::Path::new(&path).is_file() {
            continue;
        }
        let raw = std::fs::read_to_string(path).unwrap();
        let body = raw
            .split("island-figure\n")
            .nth(1)
            .unwrap()
            .split("\x60\x60\x60")
            .next()
            .unwrap()
            .trim();
        let Some(scene) = extract_complete_visual(&raw) else {
            let feedback =
                figure_program_feedback(body).unwrap_or_else(|| "Incomplete figure".into());
            std::fs::write(format!("target/quality-{name}-feedback.txt"), &feedback).unwrap();
            println!("{name}: {feedback}");
            continue;
        };
        let PresentationElement::FigureScene { parts, .. } = &scene.elements[0] else {
            panic!()
        };
        println!(
            "{name}: {} records, {} mesh records",
            parts.len(),
            parts
                .iter()
                .filter(|p| !matches!(p.shape, FigureShape::Group))
                .count()
        );
        std::fs::write(
            format!("target/quality-{name}-scene.json"),
            serde_json::to_string(&scene).unwrap(),
        )
        .unwrap();
    }
}

#[cfg(test)]
#[test]
fn connected_static_header_survives_the_compiler_and_json_roundtrip() {
    let raw = "\x60\x60\x60island-figure\n{\"program\":true,\"connected\":true,\"label\":\"Housing\",\"steps\":[{\"label\":\"Model\"}]}\n{\"op\":\"part\",\"id\":\"shell\",\"shape\":{\"kind\":\"box\",\"size\":[1,1,1]},\"color\":\"#ffffff\"}\n\x60\x60\x60";
    let scene = extract_complete_visual(raw).unwrap();
    assert!(matches!(
        scene.elements[0],
        PresentationElement::FigureScene {
            connected: true,
            ..
        }
    ));
    let json = serde_json::to_string(&scene).unwrap();
    assert!(matches!(
        parse_presentation_json(&json).unwrap().elements[0],
        PresentationElement::FigureScene {
            connected: true,
            ..
        }
    ));
}

#[cfg(test)]
#[test]
fn detailed_live_model_preserves_templates_and_signed_mirrored_carriers() {
    let raw = include_str!("test-fixtures/quality-sol-first-figure.txt");
    let scene = extract_complete_visual(raw).expect("all detailed geometry retained");
    let PresentationElement::FigureScene { parts, .. } = &scene.elements[0] else {
        panic!()
    };
    assert_eq!(parts.len(), 83);
    assert_eq!(
        parts
            .iter()
            .find(|part| part.id == "left_leg")
            .unwrap()
            .transform
            .scale,
        [-1., 1., 1.]
    );
    assert!(parts.iter().any(|part| part.id == "faceplate_ridge"));
    let mut oversized = String::from(
        "{\"program\":true,\"label\":\"Budget test\",\"steps\":[{\"label\":\"Model\"}]}\n",
    );
    for i in 0..=MAX_FIGURE_PARTS {
        oversized.push_str(&format!("{{\"op\":\"part\",\"id\":\"p{i}\",\"shape\":{{\"kind\":\"box\",\"size\":[1,1,1]}},\"color\":\"#ffffff\"}}\n"));
    }
    assert!(
        figure_program_feedback(&oversized)
            .unwrap()
            .contains("record budget exceeded")
    );
    assert!(
        extract_complete_visual(&raw.replace("\"scale\":[-1,1,1]", "\"scale\":[0,1,1]")).is_none()
    );
}

#[cfg(test)]
#[test]
fn small_functional_profiles_are_scale_aware_but_degenerate_and_crossed_outlines_fail() {
    let panel = [
        [-0.205, 0.065],
        [0.205, 0.035],
        [0.165, -0.065],
        [-0.165, -0.065],
    ];
    assert!(valid_profile(&panel, 0.035));
    assert!(valid_profile(&panel.map(|p| p.map(|v| v * 0.1)), 0.005));
    assert!(!valid_profile(&[[0., 0.], [0.1, 0.], [0.2, 0.]], 0.01));
    assert!(!valid_profile(
        &[[0., 0.], [0.1, 0.1], [0., 0.1], [0.1, 0.]],
        0.01
    ));
    assert!(!valid_profile(&panel, 0.));
    assert!(!valid_profile(
        &[[f64::NAN, 0.], [0.1, 0.], [0.2, 0.]],
        0.01
    ));
}

#[cfg(test)]
#[test]
fn fresh_authoring_failures_keep_precise_causes_without_simplifying_geometry() {
    for (raw, expected) in [
        (
            include_str!("test-fixtures/quality-luna-first-figure.txt"),
            "Duplicate local part ID",
        ),
        (
            include_str!("test-fixtures/quality-microscope-first-figure.txt"),
            "repeat.offset must be nonzero",
        ),
    ] {
        let body = raw
            .split("island-figure\n")
            .nth(1)
            .unwrap()
            .split("\x60\x60\x60")
            .next()
            .unwrap()
            .trim();
        assert!(extract_complete_visual(raw).is_none());
        assert!(figure_program_feedback(body).unwrap().contains(expected));
    }
}

#[cfg(test)]
#[test]
fn session113_full_transformation_survives_compilation() {
    let raw = include_str!("test-fixtures/session113-original-figure.txt");
    let body = raw
        .split("island-figure\n")
        .nth(1)
        .unwrap()
        .split("\x60\x60\x60")
        .next()
        .unwrap()
        .trim();
    let scene = extract_complete_visual(raw)
        .unwrap_or_else(|| panic!("{:?}", figure_program_feedback(body)));
    let PresentationElement::FigureScene { parts, .. } = &scene.elements[0] else {
        panic!()
    };
    assert_eq!(parts.len(), 125);
    assert_eq!(
        parts
            .iter()
            .filter_map(|part| part.motion.as_ref())
            .map(|motion| motion.channels.len())
            .sum::<usize>(),
        16
    );
    assert!(parts.iter().any(|part| part.id.len() > 32));
    let decoded: AgentPresentation =
        serde_json::from_str(&serde_json::to_string(&scene).unwrap()).unwrap();
    assert!(valid(&decoded));
    std::fs::write(
        "target/session113-complete.json",
        serde_json::to_string(&scene).unwrap(),
    )
    .unwrap();
}

#[cfg(test)]
#[test]
fn qualified_figure_ids_roundtrip_with_labels_but_stay_bounded() {
    let raw = include_str!("test-fixtures/session113-original-figure.txt");
    let mut scene = extract_complete_visual(raw).unwrap();
    let PresentationElement::FigureScene { parts, labels, .. } = &mut scene.elements[0] else {
        panic!()
    };
    let id = parts
        .iter()
        .find(|part| part.id.len() > 32)
        .unwrap()
        .id
        .clone();
    labels.push(
        serde_json::from_value(serde_json::json!({"text":"Scoped detail","part":id})).unwrap(),
    );
    assert!(parse_presentation_json(&serde_json::to_string(&scene).unwrap()).is_some());
    let PresentationElement::FigureScene { parts, .. } = &mut scene.elements[0] else {
        panic!()
    };
    parts.last_mut().unwrap().id = "x".repeat(MAX_FIGURE_ID_BYTES + 1);
    assert!(!valid(&scene));
    let invalid = raw.replacen(
        "\"id\":\"cab\"",
        &format!("\"id\":\"{}\"", "x".repeat(33)),
        1,
    );
    assert!(extract_complete_visual(&invalid).is_none());
}
