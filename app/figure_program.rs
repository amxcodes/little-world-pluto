//! Bounded construction compiler. No generated code is executed.
use serde_json::{Value, json};
use std::collections::HashMap;

pub(super) fn compile(body: &str, closed: bool) -> Result<String, String> {
    let mut lines = body.split_inclusive('\n');
    let mut header: Value = super::parse_figure_header(lines.next().ok_or("Missing header")?)
        .map_err(|e| e.to_string())?;
    let duration = header["duration"].as_f64().unwrap_or(4.);
    if !duration.is_finite() || !(0.25..=60.).contains(&duration) {
        return Err("Duration must be 0.25–60 seconds".into());
    }
    if header.get("steps").is_none() {
        header["steps"] = json!([{"label":"Start","marks":[]},{"label":"End","marks":[]}]);
    }
    let last = header["steps"]
        .as_array()
        .filter(|v| (1..=super::MAX_FIGURE_STEPS).contains(&v.len()))
        .ok_or("Use 1–8 pose labels; timed motion requires at least two")?
        .len()
        - 1;
    let mut templates = match header.get("templates") {
        Some(value) => value
            .as_object()
            .ok_or("Templates must be a named object")?
            .clone(),
        None => serde_json::Map::new(),
    };
    let mut commands = Vec::new();
    let mut definition: Option<(String, Vec<Value>)> = None;
    for (index, line) in lines.enumerate() {
        if index >= 512 {
            return Err("Construction record limit exceeded".into());
        }
        if !closed && !line.ends_with('\n') {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let mut command: Value = serde_json::from_str(line)
            .or_else(|_| serde_json::from_str(&super::repair_figure_array_closures(line)))
            .map_err(|error| {
                format!("Invalid construction record on line {}: {error}", index + 2)
            })?;
        // Recover unambiguous placement/ID aliases without dropping geometry.
        // Explicit canonical fields take precedence by leaving conflicts invalid.
        if command["op"] == "group"
            && let Some(fields) = command.as_object_mut()
            && !fields.contains_key("id")
            && fields.get("group").is_some_and(Value::is_string)
        {
            let id = fields.remove("group").expect("checked alias");
            fields.insert("id".into(), id);
        }
        if command["op"] == "part"
            && let Some(fields) = command.as_object_mut()
            && !fields.contains_key("transform")
            && let Some(shape) = fields.get_mut("shape").and_then(Value::as_object_mut)
            && shape.get("transform").is_some_and(Value::is_object)
        {
            let transform = shape.remove("transform").expect("checked alias");
            fields.insert("transform".into(), transform);
        }
        match command["op"].as_str() {
            Some("template") => {
                if definition.is_some() {
                    return Err("Templates cannot be nested".into());
                }
                let name = command["name"]
                    .as_str()
                    .filter(|name| !name.is_empty() && name.len() <= 32)
                    .ok_or("A template needs a name of at most 32 bytes")?;
                definition = Some((name.to_owned(), Vec::new()));
            }
            Some("endtemplate") => {
                let (name, body) = definition.take().ok_or("Endtemplate without template")?;
                if templates.insert(name.clone(), Value::Array(body)).is_some() {
                    return Err(format!("Duplicate template: {name}"));
                }
            }
            _ => match &mut definition {
                Some((_, body)) => body.push(command),
                None => commands.push(command),
            },
        }
    }
    if closed && definition.is_some() {
        return Err("Unclosed template".into());
    }
    let mut compiler = Compiler {
        parts: vec![],
        ids: HashMap::new(),
        paths: HashMap::new(),
        part_paths: Vec::new(),
        scopes: vec![],
        hinges: HashMap::new(),
        templates: Value::Object(templates),
        expanded: 0,
        duration,
        last: last as f64,
    };
    let last_content = commands.iter().rposition(|command| command["op"] != "end");
    for (index, command) in commands.into_iter().enumerate() {
        // A redundant close after the entire root is complete has no effect.
        // Mid-program closes still fail because they could change ancestry.
        if command["op"] == "end"
            && compiler.scopes.is_empty()
            && last_content.is_some_and(|last| index > last)
        {
            continue;
        }
        if !closed
            && matches!(command["op"].as_str(), Some("build" | "build_many"))
            && command["template"]
                .as_str()
                .is_some_and(|name| compiler.templates.get(name).is_none())
        {
            // A later template definition may still be streaming. Keep the built prefix.
            break;
        }
        compiler.command(command, "", 0)?;
    }
    if closed && !compiler.scopes.is_empty() {
        return Err(format!(
            "Unclosed group scopes: {}. Emit one end for each open group or hinge.",
            compiler.scopes.join(", ")
        ));
    }
    if closed {
        compiler.validate_moving_groups()?;
    }
    let fields = header.as_object_mut().ok_or("Invalid header")?;
    fields.remove("program");
    fields.remove("templates");
    if !closed {
        fields.remove("assembly_plan");
    }
    if let Some(labels) = fields.get_mut("labels").and_then(Value::as_array_mut) {
        for label in labels {
            if let Some(id) = label["part"]
                .as_str()
                .and_then(|reference| compiler.resolve_reference(reference, "").ok())
            {
                label["part"] = json!(id);
            }
        }
    }
    if let Some(roles) = fields
        .get_mut("assembly_plan")
        .and_then(|plan| plan.get_mut("assemblies"))
        .and_then(Value::as_array_mut)
    {
        for role in roles {
            if let Some(reference) = role["id"].as_str() {
                role["id"] = json!(compiler.resolve_reference(reference, "")?);
            }
        }
    }
    fields.insert("duration".into(), json!(duration));
    let mut output = serde_json::to_string(&header).map_err(|e| e.to_string())?;
    output.push('\n');
    for part in compiler.parts {
        output.push_str(&part.to_string());
        output.push('\n');
    }
    Ok(output)
}

struct Compiler {
    parts: Vec<Value>,
    ids: HashMap<String, usize>,
    paths: HashMap<String, String>,
    part_paths: Vec<String>,
    scopes: Vec<String>,
    hinges: HashMap<String, Option<usize>>,
    templates: Value,
    expanded: usize,
    duration: f64,
    last: f64,
}
impl Compiler {
    fn validate_moving_groups(&self) -> Result<(), String> {
        let mut occupied = std::collections::HashSet::new();
        for part in &self.parts {
            if part["shape"]["kind"] == "group" {
                continue;
            }
            let mut parent = part["parent"].as_str();
            // Parents are declared before children, so this graph is acyclic.
            while let Some(id) = parent {
                occupied.insert(id);
                parent = self.parts[self.ids[id]]["parent"].as_str();
            }
        }
        let empty: Vec<_> = self
            .parts
            .iter()
            .filter(|part| {
                part["shape"]["kind"] == "group"
                    && part.get("motion").is_some()
                    && !occupied.contains(part["id"].as_str().unwrap_or(""))
            })
            .filter_map(|part| part["id"].as_str())
            .collect();
        if empty.is_empty() {
            return Ok(());
        }
        Err(format!(
            "Animated groups have no mesh descendants: {}. Their motion cannot move any geometry. A scope:false group does not capture following parts: set parent on every child, or use a scoped group and end. Preserve the geometry and repair its parent links.",
            empty.join(", ")
        ))
    }

    fn resolve_reference(&self, reference: &str, prefix: &str) -> Result<String, String> {
        if !prefix.is_empty() {
            let scoped = format!("{prefix}{reference}");
            if self.ids.contains_key(&scoped) {
                return Ok(scoped);
            }
        }
        if let Some(id) = self.paths.get(reference) {
            return Ok(id.clone());
        }
        // An explicitly named carrier can also contain a same-named mesh.
        // The global carrier ID remains unambiguous when all suffix matches
        // are within that carrier. Unrelated reused local names still fail.
        if let Some(&index) = self.ids.get(reference)
            && self.parts[index]["shape"]["kind"] == "group"
        {
            let owner = &self.part_paths[index];
            if self
                .paths
                .keys()
                .filter(|path| {
                    path.strip_suffix(reference)
                        .is_some_and(|head| head.ends_with('.'))
                })
                .all(|path| {
                    path == owner
                        || path
                            .strip_prefix(owner)
                            .is_some_and(|tail| tail.starts_with('.'))
                })
            {
                return Ok(reference.to_owned());
            }
        }
        let mut matches = self.paths.iter().filter(|(path, _)| {
            path.strip_suffix(reference)
                .is_some_and(|head| head.ends_with('.'))
        });
        if let Some((_, id)) = matches.next() {
            if matches.next().is_some() {
                return Err(format!(
                    "Ambiguous part reference {reference}. Use its parent path."
                ));
            }
            return Ok(id.clone());
        }
        if self.ids.contains_key(reference) {
            return Ok(reference.to_owned());
        }
        Err(format!(
            "Unknown part reference {reference}. Use an existing ID or parent path."
        ))
    }

    fn command(&mut self, mut value: Value, prefix: &str, depth: usize) -> Result<(), String> {
        self.expanded += 1;
        if depth > 5 || self.expanded > 512 {
            return Err("Construction expansion limit exceeded".into());
        }
        let op = value["op"]
            .as_str()
            .ok_or("Missing construction op")?
            .to_owned();
        match op.as_str() {
            "group" | "hinge" | "part" => {
                let raw = value["id"].as_str().ok_or("Missing part id")?;
                let parent = match value.get("parent") {
                    Some(parent) => {
                        let reference =
                            parent.as_str().ok_or("Parent must be a group reference")?;
                        let id = self.resolve_reference(reference, prefix)?;
                        if self.parts[self.ids[&id]]["shape"]["kind"] != "group" {
                            return Err(format!("Parent {reference} must be a group or hinge"));
                        }
                        Some(id)
                    }
                    None => self.scopes.last().cloned(),
                };
                // Some authors redundantly annotate the current carrier as group.
                // Accept it only when it agrees with the already resolved parent;
                // never silently reparent a component from an ambiguous alias.
                if let Some(alias) = value.get("group") {
                    let reference = alias.as_str().ok_or("group must name the current parent")?;
                    let resolved = self.resolve_reference(reference, prefix)?;
                    if parent.as_deref() != Some(resolved.as_str()) {
                        return Err(format!("Part {raw} has conflicting group and parent/scope"));
                    }
                }
                let open_scope = match value.get("scope") {
                    None => op != "part",
                    Some(value) if op != "part" => {
                        value.as_bool().ok_or("Scope must be true or false")?
                    }
                    Some(_) => return Err("Only groups and hinges can open a scope".into()),
                };
                let path = parent.as_ref().map_or_else(
                    || raw.to_owned(),
                    |parent| format!("{}.{raw}", self.part_paths[self.ids[parent]]),
                );
                let mut id = format!("{prefix}{raw}");
                if self.parts.len() >= super::MAX_FIGURE_PARTS {
                    return Err(format!(
                        "Named record budget exceeded at {path}: limit {} including groups. Reuse repeated geometry; do not remove defining features.",
                        super::MAX_FIGURE_PARTS
                    ));
                }
                if raw.is_empty() || raw.len() > 32 {
                    return Err(format!("Part {path} needs an ID of 1–32 characters"));
                }
                if self.paths.contains_key(&path) {
                    return Err(format!(
                        "Duplicate local part ID at {path}. Give distinct physical components distinct IDs and update their labels or motion references; preserve both geometries."
                    ));
                }
                if self.ids.contains_key(&id) {
                    id = path.clone();
                }
                if id.len() > super::MAX_FIGURE_ID_BYTES || self.ids.contains_key(&id) {
                    return Err(format!(
                        "Qualified part ID {path} exceeds {} bytes or collides with an existing ID. Local names may be reused only in distinct scopes.",
                        super::MAX_FIGURE_ID_BYTES
                    ));
                }
                let fields = value.as_object_mut().ok_or("Invalid part")?;
                fields.remove("op");
                fields.remove("group");
                if fields.contains_key("parent_local") {
                    return Err(
                        "Construction records use parent-local coordinates. Omit parent_local."
                            .into(),
                    );
                }
                fields.remove("parent");
                fields.remove("scope");
                fields.insert("id".into(), json!(id));
                if let Some(parent) = &parent {
                    fields.insert("parent".into(), json!(parent));
                    fields.insert("parent_local".into(), json!(true));
                }
                if op == "hinge" {
                    if parent.is_none() {
                        return Err("A hinge must be inside its supporting group".into());
                    }
                    let axis = match fields.remove("axis") {
                        Some(value) => match value.as_str() {
                            Some("x") => Some(0),
                            Some("y") => Some(1),
                            Some("z") => Some(2),
                            _ => return Err("A hinge axis must be x, y or z".into()),
                        },
                        // Earlier recipes used axis-less mounts for translation.
                        // Keep their authored location without declaring fixed contact.
                        None => None,
                    };
                    let anchor = fields.remove("at").ok_or("A hinge requires an at anchor")?;
                    point(&anchor)?;
                    if fields.contains_key("transform")
                        || fields.contains_key("motion")
                        || fields.contains_key("joint")
                    {
                        return Err("A hinge derives its transform, joint and motion from at, axis and moves".into());
                    }
                    fields.insert("transform".into(), json!({"position":anchor}));
                    if axis.is_some() {
                        fields.insert(
                            "joint".into(),
                            json!({"anchor":[0,0,0],"parent_anchor":anchor,"tolerance":0.01}),
                        );
                    }
                    self.hinges.insert(id.clone(), axis);
                }
                if op != "part" {
                    fields.insert("shape".into(), json!({"kind":"group"}));
                } else {
                    if let Some(span) = fields.remove("span") {
                        apply_span(fields, &span)?;
                    }
                    if let Some(shape) = fields.get_mut("shape").and_then(Value::as_object_mut) {
                        compile_curved_profile(shape)?;
                    }
                    for key in ["repeat", "mirror", "around"] {
                        let nested = fields
                            .get_mut("shape")
                            .and_then(Value::as_object_mut)
                            .and_then(|shape| shape.remove(key));
                        if let Some(nested) = nested {
                            if let Some(explicit) = fields.get(key) {
                                if explicit != &nested {
                                    return Err(format!(
                                        "Part {id} has conflicting {key} values. Put {key} next to shape."
                                    ));
                                }
                            } else {
                                fields.insert(key.into(), nested);
                            }
                        }
                    }
                }
                self.ids.insert(id.clone(), self.parts.len());
                self.paths.insert(path.clone(), id.clone());
                self.part_paths.push(path);
                self.parts.push(value);
                if open_scope {
                    self.scopes.push(id);
                }
            }
            "end" => {
                self.scopes.pop().ok_or("End without group")?;
            }
            "build_many" => {
                let instances = value["instances"]
                    .as_array()
                    .filter(|instances| (1..=16).contains(&instances.len()))
                    .ok_or("build_many needs 1–16 instances")?;
                for instance in instances {
                    let mut instance = instance.clone();
                    let fields = instance
                        .as_object_mut()
                        .ok_or("Invalid assembly instance")?;
                    fields.insert("op".into(), json!("build"));
                    fields.insert("template".into(), value["template"].clone());
                    self.command(instance, prefix, depth + 1)?;
                }
            }
            "build" => {
                let name = value["template"].as_str().ok_or("Missing template")?;
                let commands = self.templates[name]
                    .as_array()
                    .ok_or_else(|| format!("Unknown template: {name}"))?
                    .clone();
                let id = value["id"].as_str().ok_or("Missing instance id")?;
                self.command(json!({"op":"group","id":id,"label":value.get("label").cloned().unwrap_or(json!(id)),"transform":value.get("transform").cloned().unwrap_or(json!({}))}),prefix,depth+1)?;
                let instance = self
                    .scopes
                    .last()
                    .ok_or("Missing template instance")?
                    .clone();
                let level = self.scopes.len();
                for command in commands {
                    self.command(command, &format!("{instance}."), depth + 1)?;
                    if self.scopes.len() < level {
                        return Err("Template escaped its instance scope".into());
                    }
                }
                if self.scopes.len() != level {
                    return Err("Unclosed template group".into());
                }
                self.scopes.pop();
            }
            "beat" => {
                let moves = value["moves"]
                    .as_array()
                    .filter(|moves| (1..=32).contains(&moves.len()))
                    .ok_or("A beat needs 1–32 moves")?;
                for movement in moves {
                    let mut movement = movement.clone();
                    let fields = movement.as_object_mut().ok_or("Invalid beat move")?;
                    fields.insert("op".into(), json!("move"));
                    fields.insert("start".into(), value["start"].clone());
                    fields.insert("end".into(), value["end"].clone());
                    self.command(movement, prefix, depth + 1)?;
                }
            }
            "channel" => {
                if self.last == 0. {
                    return Err("Timed motion requires at least two pose labels".into());
                }
                let reference = value["target"].as_str().ok_or("Channel needs a target")?;
                let target = self.resolve_reference(reference, prefix)?;
                let index = self.ids[&target];
                if self.hinges.contains_key(&target) {
                    return Err(
                        "Channel tracks require a group; use move angle for a fixed hinge".into(),
                    );
                }
                let name = value["channel"]
                    .as_str()
                    .ok_or("Channel needs translate.x/y/z or rotate.x/y/z")?;
                let mut track: super::FigureChannel = serde_json::from_value(json!({
                    "channel":name, "start":value["start"], "end":value["end"],
                    "from":value["from"], "to":value["to"]
                }))
                .map_err(|e| format!("Invalid channel track: {e}"))?;
                track.start = track.start / self.duration * self.last;
                track.end = track.end / self.duration * self.last;
                let part = &mut self.parts[index];
                if part.get("motion").is_none() {
                    part["motion"] = json!({"pivot":part.pointer("/transform/position").cloned().unwrap_or(json!([0,0,0])),"keys":[],"channels":[]});
                }
                if part["motion"].get("channels").is_none() {
                    part["motion"]["channels"] = json!([]);
                }
                part["motion"]["channels"]
                    .as_array_mut()
                    .ok_or("Invalid channels")?
                    .push(serde_json::to_value(track).map_err(|e| e.to_string())?);
                let motion: super::FigureMotion = serde_json::from_value(part["motion"].clone())
                    .map_err(|e| format!("Invalid channel motion: {e}"))?;
                if !motion.channels_are_valid(self.last) {
                    return Err(format!(
                        "Invalid channel schedule for {target}: use finite bounded offsets, initial from:0, increasing nonoverlapping intervals on each channel and continuous from/to values. Different channels may overlap. Do not mix channels with move, keys or periodic motion."
                    ));
                }
            }
            "move" => {
                if self.last == 0. {
                    return Err("Timed motion requires at least two pose labels".into());
                }
                let reference = value["target"].as_str().ok_or("Missing move target")?;
                let target = self.resolve_reference(reference, prefix)?;
                let index = self.ids[&target];
                if self.parts[index]
                    .pointer("/motion/channels")
                    .and_then(Value::as_array)
                    .is_some_and(|tracks| !tracks.is_empty())
                {
                    return Err("Do not mix move and channel tracks on one target".into());
                }
                let start = value["start"].as_f64().ok_or("Move needs start seconds")?;
                let end = value["end"].as_f64().ok_or("Move needs end seconds")?;
                if !start.is_finite()
                    || !end.is_finite()
                    || start < 0.
                    || end <= start
                    || end > self.duration
                {
                    return Err("Invalid move interval".into());
                }
                if let Some(destination) = value.get("place") {
                    if value.get("translate").is_some()
                        || value.get("aim").is_some()
                        || value.get("angle").is_some()
                        || self.hinges.contains_key(&target)
                    {
                        return Err(format!(
                            "Move {target} place requires a non-hinge target and cannot be combined with translate, aim or angle"
                        ));
                    }
                    let part = &self.parts[index];
                    let origin = part
                        .pointer("/transform/position")
                        .map(point)
                        .transpose()?
                        .unwrap_or([0.; 3]);
                    if part
                        .pointer("/motion/pivot")
                        .map(point)
                        .transpose()?
                        .is_some_and(|pivot| pivot != origin)
                    {
                        return Err(format!(
                            "Move {target} place requires the motion pivot at the target's bind origin"
                        ));
                    }
                    let destination = point(destination)?;
                    value["translate"] = json!(std::array::from_fn::<_, 3, _>(
                        |i| destination[i] - origin[i]
                    ));
                }
                if let Some(axis) = self.hinges.get(&target).copied() {
                    match axis {
                        Some(axis) => {
                            if value.get("translate").is_some() {
                                return Err(format!(
                                    "Fixed hinge {target} cannot translate. Move its supporting group instead."
                                ));
                            }
                            if let Some(aim) = value.get("aim") {
                                if value.get("angle").is_some() || value.get("rotate").is_some() {
                                    return Err(format!(
                                        "Hinge {target} aim cannot be combined with angle or rotate"
                                    ));
                                }
                                let anchor = point(&self.parts[index]["transform"]["position"])?;
                                let from = point(&aim["point"])?;
                                let to = point(&aim["to"])?;
                                let to: [f64; 3] = std::array::from_fn(|i| to[i] - anchor[i]);
                                let u = (axis + 1) % 3;
                                let v = (axis + 2) % 3;
                                let start_radius = from[u].hypot(from[v]);
                                let end_radius = to[u].hypot(to[v]);
                                if start_radius < 0.01
                                    || (start_radius - end_radius).hypot(from[axis] - to[axis])
                                        > 0.05
                                {
                                    return Err(format!(
                                        "Hinge {target} cannot reach aim.to on its rotation circle"
                                    ));
                                }
                                let cross = from[u] * to[v] - from[v] * to[u];
                                let dot = from[u] * to[u] + from[v] * to[v];
                                value["angle"] = json!(cross.atan2(dot).to_degrees());
                            }
                            if let Some(angle) = value.get("angle") {
                                if value.get("rotate").is_some() {
                                    return Err(format!(
                                        "Hinge {target} move must use either angle or rotate"
                                    ));
                                }
                                let angle = angle
                                    .as_f64()
                                    .filter(|angle| (-360. ..=360.).contains(angle))
                                    .ok_or("A hinge move requires a bounded angle in degrees")?;
                                let mut rotate = [0.; 3];
                                rotate[axis] = angle;
                                value["rotate"] = json!(rotate);
                            }
                            // Legacy rotate vectors are explicit pose offsets about the
                            // same fixed anchor. Keep a single rotation axis per track.
                            let axes = value["rotate"]
                                .as_array()
                                .filter(|values| values.len() == 3)
                                .ok_or_else(|| {
                                    format!("Hinge {target} requires an angle or a rotation vector")
                                })?;
                            let mut changed = [false; 3];
                            for (i, channel) in axes.iter().enumerate() {
                                changed[i] = channel
                                    .as_f64()
                                    .filter(|n| (-360. ..=360.).contains(n))
                                    .ok_or("Hinge rotation is outside the supported range")?
                                    != 0.;
                            }
                            if let Some(keys) = self.parts[index]
                                .pointer("/motion/keys")
                                .and_then(Value::as_array)
                            {
                                for key in keys {
                                    for (i, axis) in changed.iter_mut().enumerate() {
                                        *axis |= key["rotate"][i].as_f64().is_some_and(|n| n != 0.);
                                    }
                                }
                            }
                            if changed.into_iter().filter(|axis| *axis).count() > 1 {
                                return Err(format!(
                                    "Hinge {target} changes rotation axes across its track. Use one axis."
                                ));
                            }
                        }
                        None if value.get("translate").is_some()
                            && value.get("aim").is_none()
                            && value.get("angle").is_none()
                            && value.get("rotate").is_none() => {}
                        None => {
                            return Err(format!(
                                "Mount {target} has no axis. Use translate, or declare an axis for rotation."
                            ));
                        }
                    }
                } else if value.get("angle").is_some() || value.get("aim").is_some() {
                    return Err("Angle and aim moves require a hinge target".into());
                }
                if value.get("translate").is_none() && value.get("rotate").is_none() {
                    return Err("Move requires translate or rotate".into());
                }
                let part = &mut self.parts[index];
                if part.get("motion").is_none() {
                    part["motion"] = json!({"pivot":part.pointer("/transform/position").cloned().unwrap_or(json!([0,0,0])),"keys":[]});
                }
                let keys = part["motion"]["keys"]
                    .as_array_mut()
                    .ok_or("Invalid motion keys")?;
                let a = start / self.duration * self.last;
                let b = end / self.duration * self.last;
                let mut pose = keys
                    .last()
                    .cloned()
                    .unwrap_or(json!({"at":0.,"translate":[0,0,0],"rotate":[0,0,0]}));
                let previous = pose["at"]
                    .as_f64()
                    .ok_or("Program moves require timeline keys")?;
                if previous > a + 1e-9 {
                    return Err(format!(
                        "Overlapping or out-of-order moves for target {target}: previous move ends at {:.3}s, next starts at {start:.3}s. Combine simultaneous channels in one move or start after the previous end.",
                        previous / self.last * self.duration
                    ));
                }
                if keys.is_empty() || previous < a {
                    pose["at"] = json!(a);
                    keys.push(pose.clone());
                }
                pose["at"] = json!(b);
                for channel in ["translate", "rotate"] {
                    if let Some(v) = value.get(channel) {
                        pose[channel] = v.clone();
                    }
                }
                keys.push(pose);
                if keys.len() > 64 {
                    return Err("Timeline key limit exceeded".into());
                }
            }
            _ => return Err(format!("Unknown construction op: {op}")),
        }
        Ok(())
    }
}

fn compile_curved_profile(shape: &mut serde_json::Map<String, Value>) -> Result<(), String> {
    let outer = shape.remove("curved_outline");
    let holes = shape.remove("curved_holes");
    if outer.is_none() && holes.is_none() {
        return Ok(());
    }
    if shape.get("kind").and_then(Value::as_str) != Some("extrude") {
        return Err("Curved profiles require an extrude shape".into());
    }
    let samples = match shape.remove("curve_samples") {
        Some(value) => value
            .as_u64()
            .filter(|count| (2..=12).contains(count))
            .ok_or("curve_samples must be 2–12")? as usize,
        None => 6,
    };
    if let Some(outer) = outer {
        if shape.contains_key("outline") {
            return Err("Use either outline or curved_outline".into());
        }
        shape.insert("outline".into(), json!(sample_curve_ring(&outer, samples)?));
    }
    if let Some(holes) = holes {
        if shape.contains_key("holes") {
            return Err("Use either holes or curved_holes".into());
        }
        let holes = holes
            .as_array()
            .filter(|rings| rings.len() <= 3)
            .ok_or("A curved profile supports up to three holes")?;
        let holes = holes
            .iter()
            .map(|ring| sample_curve_ring(ring, samples))
            .collect::<Result<Vec<_>, _>>()?;
        shape.insert("holes".into(), json!(holes));
    }
    Ok(())
}

fn sample_curve_ring(value: &Value, samples: usize) -> Result<Vec<[f64; 2]>, String> {
    let commands = value
        .as_array()
        .filter(|commands| (3..=33).contains(&commands.len()))
        .ok_or("A curved ring needs a move, edges and a closing Z")?;
    let point_at = |args: &[Value], i: usize| -> Result<[f64; 2], String> {
        let coord = |i| {
            args.get(i)
                .and_then(Value::as_f64)
                .filter(|n| (-8.0..=8.0).contains(n))
                .ok_or("Curve coordinates must be within -8 to 8")
        };
        Ok([coord(i)?, coord(i + 1)?])
    };
    let first = commands[0]
        .as_array()
        .filter(|args| args.len() == 3 && args[0] == "M")
        .ok_or("Start each curved ring with [M,x,y]")?;
    if commands.last() != Some(&json!(["Z"])) {
        return Err("Close each curved ring with [Z]".into());
    }
    let first = point_at(first, 1)?;
    let mut current = first;
    let mut points = vec![first];
    for command in &commands[1..commands.len() - 1] {
        let args = command.as_array().ok_or("Curve commands must be arrays")?;
        let end = match (args.first().and_then(Value::as_str), args.len()) {
            (Some("L"), 3) => {
                let end = point_at(args, 1)?;
                points.push(end);
                end
            }
            (Some("Q"), 5) | (Some("C"), 7) => {
                let control = point_at(args, 1)?;
                let end = point_at(args, args.len() - 2)?;
                let second = if args.len() == 7 {
                    Some(point_at(args, 3)?)
                } else {
                    None
                };
                for step in 1..=samples {
                    let t = step as f64 / samples as f64;
                    let u = 1. - t;
                    points.push(std::array::from_fn(|i| match second {
                        Some(second) => {
                            u.powi(3) * current[i]
                                + 3. * u * u * t * control[i]
                                + 3. * u * t * t * second[i]
                                + t.powi(3) * end[i]
                        }
                        None => u * u * current[i] + 2. * u * t * control[i] + t * t * end[i],
                    }));
                }
                end
            }
            _ => return Err("Use [L,x,y], [Q,cx,cy,x,y], or [C,c1x,c1y,c2x,c2y,x,y]".into()),
        };
        current = end;
        if points.len() > 33 {
            return Err("Curved ring exceeds 32 sampled points".into());
        }
    }
    if points.last() == Some(&first) {
        points.pop();
    }
    if !(3..=32).contains(&points.len()) {
        return Err("Curved ring needs 3–32 sampled points".into());
    }
    Ok(points)
}

fn point(value: &Value) -> Result<[f64; 3], String> {
    let values = value
        .as_array()
        .filter(|values| values.len() == 3)
        .ok_or("An anchor must be an XYZ point")?;
    let mut result = [0.; 3];
    for (output, input) in result.iter_mut().zip(values) {
        *output = input
            .as_f64()
            .filter(|value| (-24. ..=24.).contains(value))
            .ok_or("An anchor coordinate is outside the supported range")?;
    }
    Ok(result)
}

fn apply_span(fields: &mut serde_json::Map<String, Value>, span: &Value) -> Result<(), String> {
    if fields.contains_key("transform") {
        return Err("A span derives its transform from its two endpoints".into());
    }
    let from = point(&span["from"])?;
    let to = point(&span["to"])?;
    let delta: [f64; 3] = std::array::from_fn(|i| to[i] - from[i]);
    let length = delta[0].hypot(delta[1]).hypot(delta[2]);
    if !(0.005..=24.).contains(&length) {
        return Err("Span endpoints must be distinct and at most 24 units apart".into());
    }
    let shape = fields
        .get_mut("shape")
        .and_then(Value::as_object_mut)
        .ok_or("A span requires a shape")?;
    let dimension = match shape.get("kind").and_then(Value::as_str) {
        Some("cylinder" | "sleeve") => "height",
        Some("capsule") => "length",
        _ => return Err("Spans support cylinders, sleeves and capsules".into()),
    };
    if shape.contains_key(dimension) {
        return Err(format!("A span derives {dimension} from its endpoints"));
    }
    shape.insert(dimension.into(), json!(length));
    let center: [f64; 3] = std::array::from_fn(|i| (from[i] + to[i]) * 0.5);
    // Meshes use Y as their long axis. For the viewer's ZYX Euler order,
    // Rz * Rx maps Y to the unit vector between the two endpoints.
    let pitch = (delta[2] / length).clamp(-1., 1.).asin().to_degrees();
    let roll = (-delta[0]).atan2(delta[1]).to_degrees();
    fields.insert(
        "transform".into(),
        json!({"position":center, "rotation":[pitch,0.,roll]}),
    );
    Ok(())
}

#[cfg(test)]
mod attachment_regressions {
    use super::compile;

    #[test]
    fn authoring_example_keeps_details_in_moving_assembly() {
        let body = include_str!("../skills/pluto-3d-authoring/examples/folding-panel.jsonl");
        let output = compile(body, true).unwrap();
        let records: Vec<serde_json::Value> = output
            .lines()
            .skip(1)
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        for id in ["panel", "ribs", "pin"] {
            let part = records.iter().find(|part| part["id"] == id).unwrap();
            assert_eq!(part["parent"], "pivot");
            assert_eq!(part["parent_local"], true);
            assert!(part["material"].is_string());
        }
        let pivot = records.iter().find(|part| part["id"] == "pivot").unwrap();
        assert_eq!(pivot["parent"], "root");
        assert_eq!(pivot["motion"]["keys"][1]["rotate"][0], -90.);
    }

    #[test]
    fn non_scoped_group_requires_explicit_mesh_parent_for_motion() {
        let body = r##"{"program":true}
{"op":"group","id":"root"}
{"op":"group","id":"slider","scope":false}
{"op":"part","id":"plate","shape":{"kind":"box","size":[1,1,1]},"color":"#ffffff"}
{"op":"end"}
{"op":"move","target":"slider","start":0,"end":2,"translate":[0,1,0]}
"##;
        let error = compile(body, true).unwrap_err();
        assert!(error.contains("no mesh descendants: slider"), "{error}");
        assert!(error.contains("scope:false"), "{error}");
        // Streaming may still be waiting for children; do not reject a prefix.
        assert!(compile(body, false).is_ok());
        let fixed = body.replace(r#""id":"plate","#, r#""id":"plate","parent":"slider","#);
        assert!(compile(&fixed, true).is_ok());
        // Mesh descendants may be nested several levels below the moved group.
        let nested = fixed.replace(
            r#"{"op":"part","id":"plate","parent":"slider","#,
            r#"{"op":"group","id":"mount","parent":"slider","scope":false}
{"op":"part","id":"plate","parent":"mount","#,
        );
        assert!(compile(&nested, true).is_ok());
    }

    #[test]
    fn overlapping_moves_identify_target_and_conflicting_time() {
        let body = include_str!("../skills/pluto-3d-authoring/examples/folding-panel.jsonl");
        let overlapping = format!(
            "{body}{{\"op\":\"move\",\"target\":\"pivot\",\"start\":2,\"end\":3,\"angle\":0}}\n"
        );
        let error = compile(&overlapping, true).unwrap_err();
        assert!(error.contains("target pivot"), "{error}");
        assert!(
            error.contains("ends at 3.000s, next starts at 2.000s"),
            "{error}"
        );
    }
}

#[cfg(test)]
#[test]
fn channel_program_compiles_overlapping_axes_and_rejects_conflicts() {
    let body = r##"{"program":true,"label":"Coordinated mechanism","duration":5}
{"op":"group","id":"g"}
{"op":"part","id":"p","shape":{"kind":"box","size":[1,1,1]},"color":"#ffffff"}
{"op":"end"}
{"op":"channel","target":"g","channel":"translate.y","start":0,"end":3,"from":0,"to":2}
{"op":"channel","target":"g","channel":"rotate.x","start":1,"end":5,"from":0,"to":90}
"##;
    let compiled = compile(body, true).unwrap();
    let scene = super::extract_complete_visual(&format!("```island-figure\n{body}```"))
        .expect("channel scene survives full validation");
    assert!(!scene.elements.is_empty());
    let part: serde_json::Value = serde_json::from_str(compiled.lines().nth(1).unwrap()).unwrap();
    let motion: super::FigureMotion = serde_json::from_value(part["motion"].clone()).unwrap();
    assert!(motion.channels_are_valid(1.));
    let (position, rotation) = motion.channel_pose(0.4);
    assert!((position[1] - 40. / 27.).abs() < 1e-9);
    assert!((rotation[0] - 14.0625).abs() < 1e-9);
    let duplicate = format!(
        "{body}{{\"op\":\"channel\",\"target\":\"g\",\"channel\":\"translate.y\",\"start\":2,\"end\":4,\"from\":2,\"to\":3}}\n"
    );
    assert!(compile(&duplicate, true).is_err());
    let mixing = format!(
        "{body}{{\"op\":\"move\",\"target\":\"g\",\"start\":3,\"end\":5,\"translate\":[1,0,0]}}\n"
    );
    assert!(compile(&mixing, true).unwrap_err().contains("mix"));
    for invalid in [
        body.replace("\"from\":0", "\"from\":1"),
        body.replace("\"end\":5", "\"end\":6"),
        body.replace("rotate.x", "rotate.bad"),
    ] {
        assert!(compile(&invalid, true).is_err());
    }
}
