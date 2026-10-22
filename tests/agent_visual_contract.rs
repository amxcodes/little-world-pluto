#[path = "../app/agent_presentation.rs"]
#[allow(dead_code)]
mod agent_presentation;

fn parse(elements: serde_json::Value) -> Option<(agent_presentation::AgentPresentation, String)> {
    agent_presentation::extract(&format!("Answer\n```island-ui\n{}\n```", serde_json::json!({"version":1,"elements":elements})))
}

#[test]
fn model_fence_variations_and_unrelated_json_do_not_hide_the_visual() {
    for fence in ["```ISLAND-UI", "```island_ui", "~~~json"] {
        let closing = if fence.starts_with('~') { "~~~" } else { "```" };
        let raw = format!("```json\n{{\"ordinary\":true}}\n```\nBefore\r\n  {fence}  \r\n{{\"version\":1,\"elements\":[{{\"type\":\"text\",\"text\":\"Visible\"}}]}}\r\n  {closing}  \r\nAfter");
        let result = agent_presentation::extract(&raw).unwrap();
        assert!(result.1.ends_with("Before\n\nAfter"));
        assert!(result.1.contains("ordinary"));
    }
}

#[test]
fn whiteboard_paint_and_proportions_have_a_bounded_contract() {
    let board = serde_json::json!({"type":"whiteboard_sketch","label":"Spatial view","background":"#101820",
        "items":[{"id":"world","label":"World","shape":"circle","fill":"info","aspect":1}],
        "strokes":[{"on":"world","points":[[10,10],[80,20],[60,80]],"closed":true,"fill":"#228855"}]});
    assert!(parse(serde_json::json!([board.clone()])).is_some());
    for paint in ["url(https://example.com/a.svg)", "<script>", "#fff", "red;display:none"] {
        let mut invalid = board.clone(); invalid["background"] = paint.into();
        assert!(parse(serde_json::json!([invalid])).is_none());
    }
    let mut invalid = board.clone(); invalid["strokes"][0]["closed"] = false.into();
    assert!(parse(serde_json::json!([invalid])).is_none());
    let mut invalid = board; invalid["items"][0]["aspect"] = 100.into();
    assert!(parse(serde_json::json!([invalid])).is_none());
}

#[test]
fn one_complete_filled_contour_does_not_require_four_redundant_strokes() {
    assert!(parse(serde_json::json!([{"type":"whiteboard_sketch","label":"Outline",
        "strokes":[{"points":[[10,10],[90,10],[90,60],[10,60]],"closed":true,"fill":"primary"}]}])).is_some());
}

#[test]
fn semantic_visuals_survive_the_gateway() {
    let result = parse(serde_json::json!([
        {"type":"space_station"},
        {"type":"assembly","label":"Parts","parts":[{"shape":"solar_array","label":"Panel","detail":"Converts light to electricity."}]},
        {"type":"system_map","label":"Request","nodes":[{"id":"a","label":"Client","detail":"Sends request."},{"id":"b","label":"Server","detail":"Responds."}],"links":[{"from":"a","to":"b","label":"Request"}]},
        {"type":"line_chart","label":"Temperatures","unit":"°C","points":[{"label":"Monday","value":-5},{"label":"Tuesday","value":0}]}
    ])).unwrap();
    assert_eq!(result.0.elements.len(), 4);
    assert_eq!(result.1, "Answer");
}

#[test]
fn bad_graphs_do_not_break_valid_siblings() {
    for nodes in [serde_json::json!([{"id":"a","label":"A","detail":"A"},{"id":"a","label":"B","detail":"B"}]), serde_json::json!([{"id":"a","label":"A","detail":"A"},{"id":"b","label":"B","detail":"B"}])] {
        let result = parse(serde_json::json!([
            {"type":"system_map","label":"Invalid","nodes":nodes,"links":[{"from":"a","to":"missing","label":"Unknown endpoint"}]},
            {"type":"text","text":"The verified explanation is retained."}
        ])).unwrap();
        assert_eq!(result.0.elements.len(), 1);
        assert!(matches!(result.0.elements[0], agent_presentation::PresentationElement::Text { .. }));
    }
}

#[test]
fn assemblies_reject_unbounded_or_mixed_geometry() {
    for parts in [
        serde_json::json!([{"shape":"block","label":"A","detail":"A","size":[10000,1,1]}]),
        serde_json::json!([{"shape":"block","label":"A","detail":"A","position":[0,0,0]},{"shape":"block","label":"B","detail":"B"}]),
        serde_json::json!([{"shape":"script","label":"A","detail":"A"}]),
    ] { assert!(parse(serde_json::json!([{"type":"assembly","label":"Unsafe","parts":parts}])).is_none()); }
}

#[test]
fn unknown_fields_cannot_smuggle_markup_into_registered_shapes() {
    assert!(parse(serde_json::json!([{"type":"assembly","label":"Parts","parts":[{"shape":"block","label":"A","detail":"A","html":"<script>alert(1)</script>"}]}])).is_none());
}

#[test]
fn a_common_trailing_comma_is_repaired_without_rewriting_text() {
    let result = agent_presentation::extract("```island-ui\n{\"version\":1,\"elements\":[{\"type\":\"assembly\",\"label\":\"Parts, details\",\"parts\":[{\"shape\":\"module\",\"label\":\"A\",\"detail\":\"Keep, this\",},],},],}\n```").unwrap();
    assert_eq!(result.0.elements.len(), 1);
}

#[test]
fn human_scenes_require_complete_bounded_pose_references() {
    let valid = serde_json::json!({"type":"human_scene","label":"Suit stages","figures":[
        {"id":"pilot","label":"Pilot","outfit":"armor","palette":"crimson","proportions":[1.1,1.0,0.9],"regions":[{"region":"hands","label":"Emitters","detail":"Fictional maneuvering system."}]}
    ],"steps":[
        {"label":"Base","detail":"Person inside the suit.","poses":[{"figure":"pilot","pose":"standing","layers":[]}]},
        {"label":"Flight","detail":"A fictional flight pose.","poses":[{"figure":"pilot","pose":"flight","focus":["hands","feet"]}]}
    ]});
    assert!(parse(serde_json::json!([valid.clone()])).is_some());
    let mut bad_reference = valid.clone();
    bad_reference["steps"][0]["poses"][0]["figure"] = serde_json::json!("missing");
    let mut bad_size = valid.clone();
    bad_size["figures"][0]["proportions"] = serde_json::json!([1,100,1]);
    let mut missing_pose = valid.clone();
    missing_pose["steps"][1]["poses"] = serde_json::json!([]);
    let mut bad_pose = valid.clone();
    bad_pose["steps"][0]["poses"][0]["pose"] = serde_json::json!("javascript:alert(1)");
    let mut duplicate_region = valid.clone();
    duplicate_region["steps"][0]["poses"][0]["layers"] = serde_json::json!(["arms","arms"]);
    for invalid in [bad_reference, bad_size, missing_pose, bad_pose, duplicate_region] {
        let result = parse(serde_json::json!([invalid,{"type":"text","text":"Preserved explanation"}])).unwrap();
        assert_eq!(result.0.elements.len(), 1);
        assert!(matches!(result.0.elements[0], agent_presentation::PresentationElement::Text { .. }));
    }
}
