//! A bounded, provider-neutral presentation contract for compact agent answers.
//!
//! The model can suggest data for registered Island components, but this module
//! never accepts markup, scripts, styles, arbitrary actions, or remote assets.
use serde::{Deserialize, Serialize};

const MAX_BLOCK_BYTES: usize = 32_000;
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
    SpaceStation {},
    HumanScene {
        label: String,
        figures: Vec<HumanFigure>,
        steps: Vec<HumanStep>,
    },
    Assembly {
        label: String,
        parts: Vec<AssemblyPart>,
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
    WhiteboardSketch {
        #[serde(alias = "title")]
        label: String,
        #[serde(default)]
        background: Option<String>,
        #[serde(default)]
        base: Option<SketchBase>,
        #[serde(default)]
        pose: Option<HumanPose>,
        #[serde(default)]
        outfit: Option<HumanOutfit>,
        #[serde(default)]
        strokes: Vec<SketchStroke>,
        #[serde(default)]
        items: Vec<SketchItem>,
        #[serde(default)]
        links: Vec<SketchLink>,
        #[serde(default)]
        caption: Option<String>,
    },
    Divider {},
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyPart {
    pub label: String,
    pub detail: String,
    pub shape: AssemblyShape,
    #[serde(default)]
    pub position: Option<[f64; 3]>,
    #[serde(default)]
    pub size: Option<[f64; 3]>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyShape {
    Block,
    Cylinder,
    Sphere,
    Panel,
    SolarArray,
    Truss,
    Module,
    Radiator,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HumanFigure {
    pub id: String,
    pub label: String,
    pub outfit: HumanOutfit,
    #[serde(default)]
    pub palette: Option<HumanPalette>,
    #[serde(default)]
    pub proportions: Option<[f64; 3]>,
    #[serde(default)]
    pub regions: Vec<HumanRegionDetail>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanOutfit {
    Body,
    Clothing,
    Sportswear,
    Protective,
    Armor,
    Spacesuit,
    Exoskeleton,
    Superhero,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanPalette {
    Theme,
    Steel,
    Crimson,
    Gold,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HumanRegion {
    Head,
    Torso,
    Arms,
    Hands,
    Legs,
    Feet,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HumanRegionDetail {
    pub region: HumanRegion,
    pub label: String,
    pub detail: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HumanStep {
    pub label: String,
    pub detail: String,
    pub poses: Vec<HumanPoseState>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HumanPoseState {
    pub figure: String,
    pub pose: HumanPose,
    #[serde(default)]
    pub focus: Vec<HumanRegion>,
    #[serde(default)]
    pub layers: Option<Vec<HumanRegion>>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanPose {
    Standing,
    Reaching,
    Walking,
    Running,
    Kneeling,
    Flight,
    Braced,
    Seated,
    Explaining,
    Launch,
    Hover,
    Landing,
    Prone,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchBase {
    Blank,
    Human,
    Airfoil,
    Connector,
    Airplane,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchRole {
    Contour,
    Motion,
    Annotation,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchShape {
    #[serde(alias = "rect")]
    Rectangle,
    #[serde(alias = "oval")]
    Ellipse,
    Circle,
    Diamond,
}

impl Default for SketchShape {
    fn default() -> Self {
        Self::Rectangle
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchPlacement {
    Inside,
    Above,
    Below,
    Left,
    Right,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchScale {
    Small,
    Medium,
    Large,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SketchItem {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub fill: Option<String>,
    #[serde(default)]
    pub aspect: Option<f64>,
    #[serde(default)]
    pub shape: SketchShape,
    #[serde(default)]
    pub outline: Option<Vec<[f64; 2]>>,
    #[serde(default)]
    pub smooth: bool,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub place: Option<SketchPlacement>,
    #[serde(default)]
    pub scale: Option<SketchScale>,
    #[serde(default)]
    pub emphasis: bool,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SketchLink {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: String,
}

// A conservative guard complements the provider's semantic routing instructions.
// Keep this on the response, not globally: switching sessions must not reuse the
// most recently submitted request's visual preferences.
#[derive(Clone, Debug, Default)]
pub struct VisualPolicy {
    text_only: bool,
    human_subject: bool,
    organic_subject: bool,
    depth_allowed: bool,
    explicit_visual: bool,
    whiteboard: bool,
}

impl VisualPolicy {
    pub fn for_request(request: &str, whiteboard: bool) -> Self {
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
            depth_allowed: has(&[
                "iss",
                "station",
                "tv",
                "television",
                "remote",
                "computer",
                "laptop",
                "phone",
                "device",
                "devices",
                "electronics",
                "electronic",
                "circuit",
                "hardware",
                "machine",
                "motor",
                "engine",
                "sensor",
                "connector",
                "usb",
                "hdmi",
                "satellite",
                "robot",
                "router",
                "battery",
                "pcb",
                "processor",
            ]),
            explicit_visual: has(&[
                "visualize",
                "visualise",
                "draw",
                "diagram",
                "sketch",
                "illustrate",
                "visually",
            ]),
            whiteboard,
        }
    }
}

pub fn apply_visual_policy(
    mut presentation: AgentPresentation,
    policy: &VisualPolicy,
) -> Option<AgentPresentation> {
    fn plain_visual(element: &PresentationElement) -> String {
        match element {
            PresentationElement::Assembly { label, parts } => format!(
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
            PresentationElement::WhiteboardSketch { label, caption, .. } => {
                caption.clone().unwrap_or_else(|| label.clone())
            }
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
        // Preserve explanations from retired rigs as text rather than discarding
        // them or showing raw JSON in responses created by older sessions.
        *elements = std::mem::take(elements)
            .into_iter()
            .flat_map(|element| match element {
                PresentationElement::HumanScene { steps, .. } => steps
                    .into_iter()
                    .map(|step| PresentationElement::Text {
                        text: format!("{}: {}", step.label, step.detail),
                        tone: None,
                    })
                    .collect(),
                PresentationElement::WhiteboardSketch {
                    base: Some(SketchBase::Human),
                    caption,
                    label,
                    ..
                } => vec![PresentationElement::Text {
                    text: caption.filter(|s| !s.trim().is_empty()).unwrap_or(label),
                    tone: None,
                }],
                other => vec![other],
            })
            .collect();
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
                PresentationElement::Assembly { .. }
                    | PresentationElement::SpaceStation { .. }
                    | PresentationElement::BlackHole { .. }
                    | PresentationElement::SystemMap { .. }
                    | PresentationElement::VisualScene { .. }
                    | PresentationElement::WhiteboardSketch { .. }
                    | PresentationElement::LineChart { .. }
                    | PresentationElement::BarChart { .. }
                    | PresentationElement::Heatmap { .. }
                    | PresentationElement::CrossSection { .. }
                    | PresentationElement::Waveform { .. }
            );
            let suppress = (graphical
                && (policy.text_only || (policy.human_subject && !policy.explicit_visual)))
                || (policy.human_subject
                    && matches!(
                        element,
                        PresentationElement::Assembly { .. }
                            | PresentationElement::VisualScene { .. }
                            | PresentationElement::WhiteboardSketch { .. }
                            | PresentationElement::SpaceStation { .. }
                    ))
                || ((policy.organic_subject || !policy.depth_allowed)
                    && matches!(
                        element,
                        PresentationElement::Assembly { .. }
                            | PresentationElement::SpaceStation { .. }
                    ))
                || (policy.organic_subject
                    && matches!(
                        element,
                        PresentationElement::VisualScene { .. }
                            | PresentationElement::WhiteboardSketch { .. }
                    ))
                || (matches!(element, PresentationElement::WhiteboardSketch { .. })
                    && !policy.whiteboard);
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
pub struct SketchStroke {
    #[serde(default)]
    pub fill: Option<String>,
    pub points: Vec<[f64; 2]>,
    /// Optional parent item: points then use 0..100 local coordinates.
    #[serde(default)]
    pub on: Option<String>,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub smooth: bool,
    #[serde(default)]
    pub role: Option<SketchRole>,
}

/// Coordinates are percentages of a fixed 100 x 70 canvas. These registered
/// marks give providers a small visual vocabulary without accepting SVG paths,
/// markup, executable code, arbitrary colors, or external resources.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VisualFrame {
    pub label: String,
    #[serde(default)]
    pub caption: Option<String>,
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

/// Extract one complete island-ui block. Invalid/unknown blocks are left in
/// ordinary answer text so a provider response can never break the workspace.
pub fn extract(raw: &str) -> Option<(AgentPresentation, String)> {
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
        let parsed = serde_json::from_str::<AgentPresentation>(&repaired)
            .ok()
            .filter(valid)
            .or_else(|| recover_valid_elements(&repaired));
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
            return Some((presentation, prose));
        }
    }
    None
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
        PresentationElement::BlackHole {} | PresentationElement::SpaceStation {} => true,
        PresentationElement::HumanScene {
            label,
            figures,
            steps,
        } => {
            text_ok(label, 100)
                && (1..=3).contains(&figures.len())
                && (1..=6).contains(&steps.len())
                && figures.iter().enumerate().all(|(index, figure)| {
                    text_ok(&figure.id, 40)
                        && figures[..index].iter().all(|other| other.id != figure.id)
                        && text_ok(&figure.label, 60)
                        && figure.proportions.map_or(true, |values| {
                            values
                                .iter()
                                .all(|v| v.is_finite() && (0.8..=1.2).contains(v))
                        })
                        && figure.regions.len() <= 6
                        && figure.regions.iter().enumerate().all(|(i, region)| {
                            text_ok(&region.label, 60)
                                && text_ok(&region.detail, 400)
                                && figure.regions[..i]
                                    .iter()
                                    .all(|other| other.region != region.region)
                        })
                })
                && steps.iter().all(|step| {
                    text_ok(&step.label, 60)
                        && text_ok(&step.detail, 600)
                        && step.poses.len() == figures.len()
                        && step.poses.iter().enumerate().all(|(i, pose)| {
                            figures.iter().any(|figure| figure.id == pose.figure)
                                && step.poses[..i]
                                    .iter()
                                    .all(|other| other.figure != pose.figure)
                                && pose.focus.len() <= 6
                                && pose
                                    .focus
                                    .iter()
                                    .enumerate()
                                    .all(|(j, region)| !pose.focus[..j].contains(region))
                                && pose.layers.as_ref().map_or(true, |layers| {
                                    layers.len() <= 6
                                        && layers
                                            .iter()
                                            .enumerate()
                                            .all(|(j, region)| !layers[..j].contains(region))
                                })
                        })
                })
        }
        PresentationElement::Assembly { label, parts } => {
            text_ok(label, 100)
                && (1..=12).contains(&parts.len())
                && (parts.iter().all(|part| part.position.is_none())
                    || parts.iter().all(|part| part.position.is_some()))
                && parts.iter().all(|part| {
                    text_ok(&part.label, 60)
                        && text_ok(&part.detail, 400)
                        && part.position.is_none_or(|position| {
                            position
                                .iter()
                                .all(|v| v.is_finite() && (-8.0..=8.0).contains(v))
                        })
                        && part.size.is_none_or(|size| {
                            size.iter()
                                .all(|v| v.is_finite() && (0.15..=8.0).contains(v))
                        })
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
        PresentationElement::WhiteboardSketch {
            label,
            background,
            base,
            pose,
            outfit,
            strokes,
            items,
            links,
            caption,
        } => {
            text_ok(label, 100)
                && valid_paint(background.as_deref())
                && optional_text_ok(caption.as_deref(), 280)
                && valid_sketch_items(items, links)
                && strokes.len() <= 24
                && (matches!(base, Some(SketchBase::Human)) || (pose.is_none() && outfit.is_none()))
                && (!items.is_empty()
                    || !matches!(base, None | Some(SketchBase::Blank))
                    || !strokes.is_empty())
                && strokes.iter().all(|stroke| {
                    (2..=24).contains(&stroke.points.len())
                        && stroke
                            .on
                            .as_ref()
                            .is_none_or(|id| items.iter().any(|item| item.id == *id))
                        && valid_paint(stroke.fill.as_deref())
                        && (stroke.fill.as_deref().is_none_or(|fill| fill == "none")
                            || stroke.closed)
                        && (!stroke.closed || stroke.points.len() >= 3)
                        && stroke.points.iter().all(|point| {
                            point[0].is_finite()
                                && point[1].is_finite()
                                && (0.0..=100.0).contains(&point[0])
                                && (0.0..=if stroke.on.is_some() { 100.0 } else { 70.0 })
                                    .contains(&point[1])
                        })
                        && stroke.points.windows(2).any(|pair| {
                            (pair[0][0] - pair[1][0]).abs() + (pair[0][1] - pair[1][1]).abs() >= 0.5
                        })
                })
                && (!items.is_empty()
                    || !matches!(base, None | Some(SketchBase::Blank))
                    || sketch_covers_subject(strokes))
        }
        PresentationElement::Divider {} => true,
    }
}

fn visual_point(x: f64, y: f64) -> bool {
    x.is_finite() && y.is_finite() && (0.0..=100.0).contains(&x) && (0.0..=70.0).contains(&y)
}

fn valid_paint(value: Option<&str>) -> bool {
    value.is_none_or(|paint| {
        matches!(
            paint,
            "none" | "surface" | "primary" | "neutral" | "info" | "success" | "warning" | "danger"
        ) || (paint.len() == 7
            && paint.starts_with('#')
            && paint[1..].bytes().all(|byte| byte.is_ascii_hexdigit()))
    })
}

fn valid_sketch_items(items: &[SketchItem], links: &[SketchLink]) -> bool {
    items.len() <= 12
        && links.len() <= 16
        && items.iter().enumerate().all(|(i, item)| {
            text_ok(&item.id, 40)
                && text_ok(&item.label, 36)
                && valid_paint(item.fill.as_deref())
                && item
                    .aspect
                    .is_none_or(|aspect| aspect.is_finite() && (0.2..=5.0).contains(&aspect))
                && optional_text_ok(item.detail.as_deref(), 180)
                && item.outline.as_ref().is_none_or(|points| {
                    (3..=24).contains(&points.len())
                        && points.iter().all(|point| {
                            point
                                .iter()
                                .all(|value| value.is_finite() && (0.0..=100.0).contains(value))
                        })
                        && points
                            .iter()
                            .map(|point| point[0])
                            .fold(100.0_f64, f64::min)
                            + 25.0
                            <= points.iter().map(|point| point[0]).fold(0.0_f64, f64::max)
                        && points
                            .iter()
                            .map(|point| point[1])
                            .fold(100.0_f64, f64::min)
                            + 25.0
                            <= points.iter().map(|point| point[1]).fold(0.0_f64, f64::max)
                })
                && [item.x, item.y, item.width, item.height]
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite())
                && item.x.is_none_or(|v| (0.0..=100.0).contains(&v))
                && item.y.is_none_or(|v| (0.0..=70.0).contains(&v))
                && item.width.is_none_or(|v| (1.0..=90.0).contains(&v))
                && item.height.is_none_or(|v| (1.0..=65.0).contains(&v))
                && items[..i].iter().all(|other| other.id != item.id)
                && item.parent.as_ref().is_none_or(|parent| {
                    parent != &item.id
                        && items.iter().any(|candidate| candidate.id == *parent)
                        && items
                            .iter()
                            .find(|candidate| candidate.id == *parent)
                            .and_then(|candidate| candidate.parent.as_ref())
                            .is_none()
                })
        })
        && links.iter().all(|link| {
            link.from != link.to
                && optional_text_ok(Some(&link.label), 60)
                && items.iter().any(|item| item.id == link.from)
                && items.iter().any(|item| item.id == link.to)
        })
}

fn sketch_covers_subject(strokes: &[SketchStroke]) -> bool {
    let contour_count = strokes
        .iter()
        .filter(|stroke| matches!(stroke.role.as_ref(), None | Some(SketchRole::Contour)))
        .count();
    if contour_count == 0 {
        return false;
    }
    let points = strokes
        .iter()
        .filter(|stroke| matches!(stroke.role.as_ref(), None | Some(SketchRole::Contour)))
        .flat_map(|stroke| stroke.points.iter());
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (100.0_f64, 0.0_f64, 70.0_f64, 0.0_f64);
    for point in points {
        min_x = min_x.min(point[0]);
        max_x = max_x.max(point[0]);
        min_y = min_y.min(point[1]);
        max_y = max_y.max(point[1]);
    }
    max_x - min_x >= 25.0 && max_y - min_y >= 20.0
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
        VisualMark::Line { x1, y1, x2, y2, .. } => visual_point(*x1, *y1) && visual_point(*x2, *y2),
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
fn recover_valid_elements(raw: &str) -> Option<AgentPresentation> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
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

fn text_ok(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && value.len() <= MAX_TEXT_BYTES
}

fn optional_text_ok(value: Option<&str>, max: usize) -> bool {
    value.is_none_or(|text| text.len() <= max && text.len() <= MAX_TEXT_BYTES)
}
