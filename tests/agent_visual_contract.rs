#[path = "../app/agent_presentation.rs"]
#[allow(dead_code)]
mod agent_presentation;

#[test]
fn saved_f16_presentation_still_loads_as_a_complete_scene() {
    let saved = include_str!("../app/test-fixtures/session114-presentation.json");
    let (presentation, _) =
        agent_presentation::extract(&format!("```island-ui\n{saved}\n```")).unwrap();
    let json = serde_json::to_value(presentation).unwrap();
    assert_eq!(json["elements"][0]["type"], "figure_scene");
    assert!(json["elements"][0]["parts"].as_array().unwrap().len() > 20);
}

fn parse(elements: serde_json::Value) -> Option<(agent_presentation::AgentPresentation, String)> {
    agent_presentation::extract(&format!(
        "Answer\n```island-ui\n{}\n```",
        serde_json::json!({"version":1,"elements":elements})
    ))
}

fn simple_scene() -> serde_json::Value {
    serde_json::json!({"type":"procedural_scene","label":"Physical layout · schematic", "ops":[
        {"op":"primitive","id":"core","label":"Core","detail":"The central component.","shape":"box","transform":{"position":[0,0,0],"scale":[4,2,1]}},
        {"op":"primitive","id":"mast","label":"Mast","detail":"Extends above the core.","shape":"cylinder","transform":{"position":[0,0,1],"scale":[0.2,0.2,2]}},
        {"op":"instance","id":"reflected-mast","label":"Reflected mast","detail":"A mirrored instance of the mast.","source":"mast","mirror":"x","transform":{"position":[2,0,1]}}
    ]})
}

#[test]
fn attachments_require_earlier_parts_and_solve_contact_placement() {
    let scene = serde_json::json!({"type":"procedural_scene","label":"Connected assembly","ops":[
        {"op":"primitive","id":"body","label":"Body","detail":"Parent.","shape":"box","transform":{"position":[3,0,0],"scale":[2,1,1]}},
        {"op":"primitive","id":"child","label":"Child","detail":"Attached part.","shape":"box","transform":{"scale":[1,1,1]},
         "attach":{"to":"body","target_anchor":[1,0,0],"self_anchor":[-1,0,0]}}
    ]});
    let (presentation, _) = parse(serde_json::json!([scene.clone()])).unwrap();
    assert!(agent_presentation::scene_diagnostics(&presentation).is_empty());
    let mut invalid = scene.clone();
    invalid["ops"][1]["attach"]["to"] = "missing".into();
    assert!(parse(serde_json::json!([invalid])).is_none());
    let mut invalid_anchor = scene;
    invalid_anchor["ops"][1]["attach"]["self_anchor"][0] = 2.into();
    assert!(parse(serde_json::json!([invalid_anchor])).is_none());
}

#[test]
fn loft_contract_accepts_model_sections_and_rejects_invalid_profiles() {
    let scene = serde_json::json!({"type":"procedural_scene","label":"Tapered body", "ops":[
        {"op":"loft","id":"body","label":"Body","detail":"A changing cross section.","sections":[
            {"z":-0.5,"profile":[[-1,-1],[1,-1],[1,1],[-1,1]]},
            {"z":0.5,"profile":[[-0.6,-0.4],[0.6,-0.4],[0.6,0.4],[-0.6,0.4]]}
        ]},
        {"op":"instance","id":"copy","label":"Copy","detail":"Reused shape.","source":"body","mirror":"x"}
    ]});
    assert!(parse(serde_json::json!([scene.clone()])).is_some());
    let mut unequal = scene.clone();
    unequal["ops"][0]["sections"][1]["profile"] = serde_json::json!([[0, 0], [1, 0], [0, 1]]);
    assert!(parse(serde_json::json!([unequal])).is_none());
    let mut nonconvex = scene;
    nonconvex["ops"][0]["sections"][1]["profile"] =
        serde_json::json!([[-1, -1], [1, -1], [0, -0.5], [-1, 1]]);
    assert!(parse(serde_json::json!([nonconvex])).is_none());
}

#[test]
fn model_fence_variations_and_unrelated_json_do_not_hide_the_visual() {
    for fence in ["```ISLAND-UI", "```island_ui", "~~~json"] {
        let closing = if fence.starts_with('~') { "~~~" } else { "```" };
        let raw = format!(
            "```json\n{{\"ordinary\":true}}\n```\nBefore\r\n  {fence}  \r\n{{\"version\":1,\"elements\":[{{\"type\":\"text\",\"text\":\"Visible\"}}]}}\r\n  {closing}  \r\nAfter"
        );
        let result = agent_presentation::extract(&raw).unwrap();
        assert!(result.1.ends_with("Before\n\nAfter"));
        assert!(result.1.contains("ordinary"));
    }
}

#[test]
fn procedural_graph_is_validated_as_data_and_keeps_text_fallback() {
    let scene = simple_scene();
    let (presentation, prose) = parse(serde_json::json!([scene.clone()])).unwrap();
    assert_eq!(prose, "Answer");
    assert_eq!(presentation.elements.len(), 1);
    let visual = agent_presentation::apply_visual_policy(
        presentation,
        &agent_presentation::VisualPolicy::for_request("Explain the physical layout"),
    )
    .unwrap();
    assert!(matches!(
        visual.elements[0],
        agent_presentation::PresentationElement::ProceduralScene { .. }
    ));
    let text_only = agent_presentation::apply_visual_policy(
        parse(serde_json::json!([scene.clone()])).unwrap().0,
        &agent_presentation::VisualPolicy::for_request("Show a 3D view, text only"),
    )
    .unwrap();
    assert!(matches!(
        text_only.elements[0],
        agent_presentation::PresentationElement::Text { .. }
    ));

    let mut invalid_source = scene.clone();
    invalid_source["ops"][2]["source"] = "later-id".into();
    let mut duplicate_id = scene.clone();
    duplicate_id["ops"][2]["id"] = "mast".into();
    let mut invalid_shape = scene.clone();
    invalid_shape["ops"][0]["shape"] = "script".into();
    let mut out_of_range = scene.clone();
    out_of_range["ops"][0]["transform"]["position"][0] = 1000.into();
    let mut invalid_scale = scene;
    invalid_scale["ops"][1]["transform"]["scale"][2] = 0.into();
    for invalid in [
        invalid_source,
        duplicate_id,
        invalid_shape,
        out_of_range,
        invalid_scale,
    ] {
        assert!(parse(serde_json::json!([invalid])).is_none());
    }
}

#[test]
fn common_model_scene_field_aliases_are_normalized_before_strict_validation() {
    let mut scene = simple_scene();
    let fields = scene.as_object_mut().unwrap();
    let label = fields.remove("label").unwrap();
    let ops = fields.remove("ops").unwrap();
    fields.insert("title".into(), label);
    fields.insert("steps".into(), ops);
    for operation in scene["steps"].as_array_mut().unwrap() {
        operation.as_object_mut().unwrap().remove("op");
    }
    let (presentation, prose) = parse(serde_json::json!([scene])).unwrap();
    assert_eq!(prose, "Answer");
    assert!(matches!(
        presentation.elements[0],
        agent_presentation::PresentationElement::ProceduralScene { ref label, ref ops, .. }
            if label == "Physical layout · schematic" && ops.len() == 3
    ));
}

#[test]
fn streamed_scene_revisions_keep_the_newest_complete_valid_scene() {
    let first = serde_json::json!({"version":1,"elements":[simple_scene()]}).to_string();
    let second = serde_json::json!({"version":1,"elements":[{"type":"procedural_scene","label":"Revised shape","ops":[
        {"op":"extrude","id":"outline","label":"Frame","detail":"A generated structural profile.","profile":[[-2,-1],[2,-1],[2,1],[1,1],[1,0],[-1,0],[-1,1],[-2,1]],"depth":0.6}
    ]}]}).to_string();
    let stream = format!(
        "Answer\n```island-ui\n{first}\n```\n```island-ui\n{second}\n```\n```island-ui\n{{\"version\":1"
    );
    let (presentation, prose) = agent_presentation::extract_latest(&stream);
    assert_eq!(prose.trim(), "Answer");
    assert!(matches!(
        presentation.unwrap().elements[0],
        agent_presentation::PresentationElement::ProceduralScene { ref label, ref ops, .. }
            if label == "Revised shape" && ops.len() == 1
    ));
    let invalid_followup =
        format!("Answer\n```island-ui\n{first}\n```\n```island-ui\n{{bad json}}\n```\nFinal prose");
    let (previous, prose) = agent_presentation::extract_latest(&invalid_followup);
    assert!(previous.is_some());
    assert!(prose.contains("Final prose"));
    assert!(!prose.contains("bad json"));
    let malformed = serde_json::json!({"type":"procedural_scene","label":"Bad","ops":[
        {"op":"extrude","id":"bad","label":"Bad","detail":"Invalid profile.","profile":[[0,0],[1,0],[0,0]],"depth":0.6}
    ]});
    assert!(parse(serde_json::json!([malformed])).is_none());
    let crossed = serde_json::json!({"type":"procedural_scene","label":"Crossed","ops":[
        {"op":"extrude","id":"crossed","label":"Crossed","detail":"Self intersection.","profile":[[0,0],[2,0],[0,2],[2,2],[1,-1]],"depth":0.6}
    ]});
    assert!(parse(serde_json::json!([crossed])).is_none());
}

#[test]
fn declared_contacts_report_clear_gaps_without_rejecting_the_scene() {
    let scene = serde_json::json!({"type":"procedural_scene","label":"Assembly","ops":[
        {"op":"primitive","id":"left","label":"Left","detail":"Left part","shape":"box","transform":{"position":[-4,0,0],"scale":[1,1,1]}},
        {"op":"primitive","id":"right","label":"Right","detail":"Right part","shape":"box","transform":{"position":[4,0,0],"scale":[1,1,1]}}
    ],"contacts":[{"from":"left","to":"right"}]});
    let (presentation, _) = parse(serde_json::json!([scene])).unwrap();
    assert_eq!(
        agent_presentation::scene_diagnostics(&presentation).len(),
        1
    );
    let bad_relation = serde_json::json!({"type":"procedural_scene","label":"Assembly","ops":[
        {"op":"primitive","id":"left","label":"Left","detail":"Left part","shape":"box"}
    ],"contacts":[{"from":"left","to":"missing"}]});
    assert!(parse(serde_json::json!([bad_relation])).is_none());
}

#[test]
fn pasted_c_clamp_scene_is_accepted_for_rendering() {
    let scene = serde_json::json!({
        "version": 1,
        "title": "Manual C-clamp — schematic, not to scale",
        "elements": [{
            "type": "procedural_scene",
            "title": "Clamp parts",
            "steps": [
                {"id":"frame","label":"C-frame","detail":"Rigid frame supports the jaws and screw.","shape":"box","transform":{"position":[0,0,0],"scale":[3,0.45,0.6]}},
                {"id":"fixedjaw","label":"Fixed jaw","detail":"Stationary contact point on one end of the frame.","shape":"box","transform":{"position":[1.15,0.25,0],"scale":[0.65,0.2,0.7]}},
                {"id":"screw","label":"Threaded screw","detail":"Turns through the opposite end of the frame to move the pad.","shape":"cylinder","transform":{"position":[-1.05,0.25,0],"rotation":[0,0,90],"scale":[0.16,1.2,0.16]}},
                {"id":"pad","label":"Swivel pad","detail":"Contacts the workpiece and can swivel as the screw advances.","shape":"cylinder","transform":{"position":[-0.38,0.25,0],"scale":[0.38,0.12,0.38]}},
                {"id":"handle","label":"Handle","detail":"Crossbar turned by hand to rotate the screw.","shape":"cylinder","transform":{"position":[-1.05,-0.55,0],"rotation":[90,0,0],"scale":[0.12,0.9,0.12]}}
            ]
        }]
    });
    let response = format!("Answer\n```island-ui\n{scene}\n```");
    let (presentation, prose) = agent_presentation::extract(&response).unwrap();
    assert_eq!(prose, "Answer");
    assert!(matches!(
        presentation.elements[0],
        agent_presentation::PresentationElement::ProceduralScene { ref label, ref ops, .. }
            if label == "Clamp parts" && ops.len() == 5
    ));
}

#[test]
fn procedural_scenes_and_other_registered_answer_components_coexist() {
    let result = parse(serde_json::json!([
        simple_scene(),
        {"type":"system_map","label":"Request","nodes":[{"id":"a","label":"Client","detail":"Sends request."},{"id":"b","label":"Server","detail":"Responds."}],"links":[{"from":"a","to":"b","label":"Request"}]},
        {"type":"line_chart","label":"Temperatures","unit":"°C","points":[{"label":"Monday","value":-5},{"label":"Tuesday","value":0}]}
    ])).unwrap();
    assert_eq!(result.0.elements.len(), 3);
    assert_eq!(result.1, "Answer");
}

#[test]
fn bad_graphs_do_not_break_valid_siblings() {
    let result = parse(serde_json::json!([
        {"type":"system_map","label":"Invalid","nodes":[{"id":"a","label":"A","detail":"A"},{"id":"a","label":"B","detail":"B"}],"links":[{"from":"a","to":"missing","label":"Unknown endpoint"}]},
        {"type":"text","text":"The verified explanation is retained."}
    ])).unwrap();
    assert_eq!(result.0.elements.len(), 1);
    assert!(matches!(
        result.0.elements[0],
        agent_presentation::PresentationElement::Text { .. }
    ));
}

#[test]
fn unknown_fields_cannot_smuggle_markup_into_scene_data() {
    let mut scene = simple_scene();
    scene["ops"][0]["html"] = "<script>alert(1)</script>".into();
    assert!(parse(serde_json::json!([scene])).is_none());
}

#[test]
fn a_common_trailing_comma_is_repaired_without_rewriting_text() {
    let result = agent_presentation::extract("```island-ui\n{\"version\":1,\"elements\":[{\"type\":\"procedural_scene\",\"label\":\"Physical parts\",\"ops\":[{\"op\":\"primitive\",\"id\":\"part\",\"label\":\"Part\",\"detail\":\"Keep, this\",\"shape\":\"box\",},],},],}\n```").unwrap();
    assert_eq!(result.0.elements.len(), 1);
}

#[test]
fn teaching_arrows_and_scientific_colors_are_bounded() {
    let mut drawing = serde_json::json!({"type":"visual_scene","label":"Light path","frames":[{"label":"Entry","caption":"A schematic direction of travel.","marks":[{"kind":"arrow","x1":10,"y1":20,"x2":40,"y2":20,"tone":"violet"},{"kind":"ring","x":50,"y":35,"r":20}]}]});
    assert!(parse(serde_json::json!([drawing.clone()])).is_some());
    drawing["frames"][0]["marks"][0]["x2"] = serde_json::json!(101);
    assert!(parse(serde_json::json!([drawing.clone()])).is_none());
    drawing["frames"][0]["marks"][0]["x2"] = serde_json::json!(40);
    drawing["frames"][0]["marks"][0]["tone"] = serde_json::json!("url(external)");
    assert!(parse(serde_json::json!([drawing])).is_none());
}

#[test]
fn figure_bevels_keep_old_shapes_readable_and_reject_collapsed_walls() {
    let mut scene = serde_json::json!({"type":"figure_scene","label":"Machined part","parts":[{
        "id":"body","label":"Body","detail":"A machined shell","color":"#aabbcc",
        "shape":{"kind":"cylinder","radius":2,"height":4}
    }]});
    for shape in [
        serde_json::json!({"kind":"cylinder","radius":2,"height":4}),
        serde_json::json!({"kind":"cylinder","radius":2,"height":4,"bevel":0.3}),
        serde_json::json!({"kind":"cylinder","radius":2,"top_radius":0,"height":4,"bevel":0.3}),
        serde_json::json!({"kind":"sleeve","radius":2,"inner_radius":1,"height":4}),
        serde_json::json!({"kind":"sleeve","radius":2,"inner_radius":1,"height":4,"bevel":0.3}),
    ] {
        scene["parts"][0]["shape"] = shape;
        scene["parts"][0]["cut"] = serde_json::json!({"axis":"y","at":0,"cap":true});
        assert!(parse(serde_json::json!([scene.clone()])).is_some());
    }
    for shape in [
        serde_json::json!({"kind":"cylinder","radius":0.2,"height":4,"bevel":0.3}),
        serde_json::json!({"kind":"cylinder","radius":2,"top_radius":0.2,"height":4,"bevel":0.3}),
        serde_json::json!({"kind":"cylinder","radius":2,"height":0.4,"bevel":0.3}),
        serde_json::json!({"kind":"sleeve","radius":2,"inner_radius":1.5,"height":4,"bevel":0.3}),
        serde_json::json!({"kind":"sleeve","radius":2,"inner_radius":1,"height":4,"bevel":-0.1}),
    ] {
        scene["parts"][0]["shape"] = shape;
        assert!(parse(serde_json::json!([scene.clone()])).is_none());
    }
}

#[test]
fn figure_thickness_is_optional_bounded_and_excluded_from_groups() {
    let mut scene = serde_json::json!({"type":"figure_scene","label":"Panel","parts":[{
        "id":"panel","label":"Panel","detail":"A thin shell","color":"#aabbcc",
        "shape":{"kind":"surface","grid":[[[-1,0,-1],[1,0,-1]],[[-1,0,1],[1,0,1]]]}
    }]});
    assert!(parse(serde_json::json!([scene.clone()])).is_some());
    scene["parts"][0]["thick"] = serde_json::json!(0.2);
    assert!(parse(serde_json::json!([scene.clone()])).is_some());
    for invalid in [-0.1, 0.0, 10.1] {
        scene["parts"][0]["thick"] = serde_json::json!(invalid);
        assert!(parse(serde_json::json!([scene.clone()])).is_none());
    }
    scene["parts"][0]["thick"] = serde_json::json!(0.2);
    scene["parts"][0]["shape"] = serde_json::json!({"kind":"group"});
    assert!(parse(serde_json::json!([scene])).is_none());
}

#[test]
fn figure_blob_modes_are_bounded_and_preserve_existing_field_recipes() {
    let mut scene = serde_json::json!({"type":"figure_scene","label":"Organic form","parts":[{
        "id":"body","label":"Body","detail":"A procedural surface","color":"#aabbcc"
    }]});
    for shape in [
        serde_json::json!({"kind":"blob","field":[{"kind":"ball","center":[0,0,0],"radius":1}],"blend":0.2}),
        serde_json::json!({"kind":"blob","field":[{"kind":"stick","from":[0,0,0],"to":[0,2,0],"radius":0.8,"to_radius":0.2}]}),
        serde_json::json!({"kind":"blob","radius":2}),
        serde_json::json!({"kind":"blob","radius":2,"lumps":0.22,"seed":65535}),
    ] {
        scene["parts"][0]["shape"] = shape;
        assert!(parse(serde_json::json!([scene.clone()])).is_some());
    }
    for shape in [
        serde_json::json!({"kind":"blob","radius":2,"lumps":0.23}),
        serde_json::json!({"kind":"blob","radius":2,"seed":65536}),
        serde_json::json!({"kind":"blob","radius":0}),
        serde_json::json!({"kind":"blob","field":[]}),
        serde_json::json!({"kind":"blob","radius":2,"blend":0.2}),
        serde_json::json!({"kind":"blob","radius":2,"field":[{"kind":"ball","center":[0,0,0],"radius":1}]}),
        serde_json::json!({"kind":"blob","field":[{"kind":"stick","from":[0,0,0],"to":[0,2,0],"radius":0.8,"to_radius":0}]}),
    ] {
        scene["parts"][0]["shape"] = shape;
        assert!(parse(serde_json::json!([scene.clone()])).is_none());
    }
}

#[test]
fn figure_annotations_validate_part_references_and_fixed_points() {
    let mut scene = serde_json::json!({"type":"figure_scene","label":"Assembly","parts":[{
        "id":"cab","label":"Cab","detail":"Main body","color":"#aabbcc",
        "shape":{"kind":"box","size":[1,1,1]}
    }],"labels":[{"text":"Cab","part":"cab"},{"text":"Origin","point":[0,0,0]}]});
    assert!(parse(serde_json::json!([scene.clone()])).is_some());
    for invalid in [
        serde_json::json!({"text":"Missing","part":"absent"}),
        serde_json::json!({"text":"Missing position"}),
        serde_json::json!({"text":"Outside","point":[25,0,0]}),
        serde_json::json!({"text":"x".repeat(73),"point":[0,0,0]}),
    ] {
        scene["labels"] = serde_json::json!([invalid]);
        assert!(parse(serde_json::json!([scene.clone()])).is_none());
    }
}

#[test]
fn streamed_annotations_wait_for_their_part_without_blocking_geometry() {
    let header = serde_json::json!({"label":"Assembly","labels":[{"text":"Wheel","part":"wheel"}]});
    let base = serde_json::json!({"id":"base","label":"Base","detail":"Body","color":"#aabbcc",
        "shape":{"kind":"box","size":[1,1,1]}});
    let mut wheel = base.clone();
    wheel["id"] = serde_json::json!("wheel");
    let mut stream = format!("\n```island-figure\n{header}\n{base}\n");
    let before = agent_presentation::extract_latest(&stream).0.unwrap();
    let before = serde_json::to_value(before).unwrap();
    assert_eq!(before["elements"][0]["parts"].as_array().unwrap().len(), 1);
    assert!(before["elements"][0].get("labels").is_none());
    stream.push_str(&format!("{wheel}\n"));
    let after =
        serde_json::to_value(agent_presentation::extract_latest(&stream).0.unwrap()).unwrap();
    assert_eq!(after["elements"][0]["labels"].as_array().unwrap().len(), 1);
}
