//! Script blocks (M-S1): a bespoke object harbouring a script.
//!
//! The I/O contract lives here so `ElementKind::ScriptObject` validation
//! needs no interpreter: the front-matter header declares typed inputs,
//! and [`reconcile`] maps keyed script outputs onto stable element ids
//! (Grasshopper-solver semantics: same key, same element, across runs).

use serde::{Deserialize, Serialize};

use crate::{Element, ElementId, Model, Transaction};

/// A script input value stored on the element.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParamValue {
    Float(f64),
    Int(i64),
    Bool(bool),
    Text(String),
    Element(ElementId),
}

/// A declared input: `# in: width: float = 3.0`.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamDecl {
    pub name: String,
    pub kind: ParamKind,
    pub default: Option<ParamValue>,
}

/// Input kinds the header understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    Float,
    Int,
    Bool,
    Text,
    Element,
}

impl ParamKind {
    fn parse(self, raw: &str) -> Result<ParamValue, String> {
        let raw = raw.trim();
        match self {
            ParamKind::Float => raw
                .parse::<f64>()
                .map(ParamValue::Float)
                .map_err(|_| format!("{raw:?} is not a float")),
            ParamKind::Int => raw
                .parse::<i64>()
                .map(ParamValue::Int)
                .map_err(|_| format!("{raw:?} is not an integer")),
            ParamKind::Bool => match raw {
                "true" => Ok(ParamValue::Bool(true)),
                "false" => Ok(ParamValue::Bool(false)),
                _ => Err(format!("{raw:?} is not true/false")),
            },
            ParamKind::Text => {
                let unquoted = raw
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .unwrap_or(raw);
                Ok(ParamValue::Text(unquoted.to_string()))
            }
            // Element inputs have no literal default: the value lives in
            // `inputs`, naming the referenced element.
            ParamKind::Element => Err("element inputs take no default".into()),
        }
    }
}

/// A per-instance input value, overriding the header default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParamInput {
    pub name: String,
    pub value: ParamValue,
}

/// A script output: stable key, current element id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScriptOutput {
    pub key: String,
    pub id: ElementId,
}

/// One registered disc module: a name that script blocks link
/// against, and its path relative to the project file's directory
/// (always under `scripts/`, by convention).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScriptLink {
    pub name: String,
    pub path: String,
}

/// The project's script registry (M-S6): the link table between
/// discrete script blocks and disc modules. One per model, held as a
/// singleton element so add/remove ride undo and travel with the
/// project file. Blocks name an entry via [`ScriptObject::link`];
/// several blocks may share one entry, and entries outlive any block
/// until explicitly removed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ScriptRegistry {
    pub links: Vec<ScriptLink>,
}

impl ScriptRegistry {
    /// The entry `name` points at, if registered.
    pub fn path_of(&self, name: &str) -> Option<&str> {
        self.links
            .iter()
            .find(|l| l.name == name)
            .map(|l| l.path.as_str())
    }

    /// Add `name -> path` or repoint it; returns whether the table changed.
    pub fn upsert(&mut self, name: &str, path: &str) -> bool {
        match self.links.iter_mut().find(|l| l.name == name) {
            Some(l) if l.path == path => false,
            Some(l) => {
                l.path = path.to_string();
                true
            }
            None => {
                self.links.push(ScriptLink {
                    name: name.to_string(),
                    path: path.to_string(),
                });
                self.links.sort_by(|a, b| a.name.cmp(&b.name));
                true
            }
        }
    }

    /// Drop the entry; returns whether one was there.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.links.len();
        self.links.retain(|l| l.name != name);
        self.links.len() != before
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut seen = std::collections::HashSet::new();
        for l in &self.links {
            if l.name.is_empty() {
                return Err("a registry entry needs a name".into());
            }
            if !seen.insert(&l.name) {
                return Err(format!("registry entry {:?} is listed twice", l.name));
            }
            if l.path.is_empty() {
                return Err(format!("registry entry {:?} needs a path", l.name));
            }
        }
        Ok(())
    }
}

/// Which script block manages `id` as one of its outputs, if any:
/// the managing block's id and name. Direct edits to a managed
/// element are locked (see the commit guard): change the script or
/// its inputs instead. Deleting the block hands the geometry back —
/// orphaned outputs are ordinary elements again.
pub fn manager_of(
    model: &crate::Model,
    id: crate::ElementId,
) -> Option<(crate::ElementId, String)> {
    model.iter().find_map(|e| match &e.kind {
        crate::ElementKind::ScriptObject(s) if s.outputs.iter().any(|o| o.id == id) => {
            Some((e.id, e.name.clone()))
        }
        _ => None,
    })
}

/// The singleton registry element in `model`, with its id.
pub fn registry_of(model: &crate::Model) -> Option<(crate::ElementId, ScriptRegistry)> {
    model.iter().find_map(|e| match &e.kind {
        crate::ElementKind::ScriptRegistry(r) => Some((e.id, r.clone())),
        _ => None,
    })
}

/// Point block `script_id` at registry entry `name -> path`, creating
/// the registry and the entry as needed. One transaction.
pub fn set_link_tx(
    model: &crate::Model,
    script_id: crate::ElementId,
    name: &str,
    path: &str,
) -> Result<crate::Transaction, String> {
    let before = model.get(script_id).cloned().ok_or("script vanished")?;
    if !matches!(before.kind, crate::ElementKind::ScriptObject(_)) {
        return Err("not a script block".into());
    }
    let mut tx = crate::Transaction::new(format!("link script {name}"));
    match registry_of(model) {
        Some((id, mut registry)) => {
            if registry.upsert(name, path) {
                let before = model.get(id).cloned().ok_or("registry vanished")?;
                let mut after = before.clone();
                after.kind = crate::ElementKind::ScriptRegistry(registry);
                tx = tx.replace(before, after);
            }
        }
        None => {
            let mut registry = ScriptRegistry::default();
            registry.upsert(name, path);
            tx = tx.insert(crate::Element::new(
                "Script registry",
                crate::ElementKind::ScriptRegistry(registry),
            ));
        }
    }
    let mut after = before.clone();
    if let crate::ElementKind::ScriptObject(s) = &mut after.kind {
        s.link = Some(name.to_string());
    }
    Ok(tx.replace(before, after))
}

/// Clear block `script_id`'s link; also drops the registry entry when
/// no other block names it. One transaction.
pub fn drop_link_tx(
    model: &crate::Model,
    script_id: crate::ElementId,
) -> Result<crate::Transaction, String> {
    let before = model.get(script_id).cloned().ok_or("script vanished")?;
    let entry = match &before.kind {
        crate::ElementKind::ScriptObject(s) => s.link.clone(),
        _ => return Err("not a script block".into()),
    };
    let mut after = before.clone();
    if let crate::ElementKind::ScriptObject(s) = &mut after.kind {
        s.link = None;
    }
    let mut tx = crate::Transaction::new("embed script").replace(before, after);
    if let Some(name) = entry {
        let still_used = model.iter().any(|e| {
            e.id != script_id
                && matches!(&e.kind, crate::ElementKind::ScriptObject(s) if s.link.as_deref() == Some(&name))
        });
        if !still_used {
            if let Some((id, mut registry)) = registry_of(model) {
                if registry.remove(&name) {
                    let before = model.get(id).cloned().ok_or("registry vanished")?;
                    if registry.links.is_empty() {
                        // The last entry went: the registry element goes
                        // too, so "remove" converges to no registry at all.
                        tx = tx.remove_cascade(model, id);
                        let _ = before;
                    } else {
                        let mut after = before.clone();
                        after.kind = crate::ElementKind::ScriptRegistry(registry);
                        tx = tx.replace(before, after);
                    }
                }
            }
        }
    }
    Ok(tx)
}

/// The script block itself: source plus engine, inputs and outputs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScriptObject {
    pub source: String,
    pub engine: String,
    pub inputs: Vec<ParamInput>,
    pub outputs: Vec<ScriptOutput>,
    /// pip requirements installed into the app-managed site-packages
    /// before running (M-S3; e.g. `geometrykit`, which pulls numpy).
    /// Absent in older files.
    #[serde(default)]
    pub requirements: Vec<String>,
    /// The block's registry entry (M-S6): the name it is registered
    /// under in the project's [`ScriptRegistry`], which maps the name
    /// onto a path relative to the project file's directory —
    /// conventionally [`entrance_path`]. When the entry resolves to a
    /// file it outranks the embedded `source`; the embedded copy is
    /// the offline fallback and is kept in step. Absent in older
    /// files and in unlinked blocks.
    #[serde(default)]
    pub link: Option<String>,
}

/// The element name as a module file name: lowercase ASCII letters,
/// digits, `_` and `-`, spaces and anything else collapsed to `-`
/// (`Huis Wand 2` -> `huis-wand`). Empty names fall back to `script`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "script".into()
    } else {
        trimmed
    }
}

/// A script block's conventional disc entrance point: `scripts/<slug>.py`
/// under the project directory (Django's `main.app` logic: registration
/// in the model, module by convention).
pub fn entrance_path(name: &str) -> String {
    format!("scripts/{}.py", slug(name))
}

impl ScriptObject {
    pub fn new(source: impl Into<String>, engine: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            engine: engine.into(),
            inputs: vec![],
            outputs: vec![],
            requirements: vec![],
            link: None,
        }
    }

    /// The header-declared inputs, in order.
    pub fn declared(&self) -> Result<Vec<ParamDecl>, String> {
        parse_front_matter(&self.source)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.engine.trim().is_empty() {
            return Err("a script needs an engine".into());
        }
        self.declared().map(|_| ())
    }
}

/// Parse `# in: name: kind = default` headers; anything else is ignored.
/// Element inputs are declared without a default: `# in: wall: element`.
pub fn parse_front_matter(source: &str) -> Result<Vec<ParamDecl>, String> {
    let mut out = Vec::new();
    for (n, line) in source.lines().enumerate() {
        let at = format!("line {}", n + 1);
        let rest = match line.trim().strip_prefix('#') {
            Some(r) => r.trim_start(),
            None => continue,
        };
        let rest = match rest
            .strip_prefix("in:")
            .or_else(|| rest.strip_prefix("in "))
        {
            Some(r) => r.trim_start(),
            None => continue,
        };
        let (name, rest) = rest
            .split_once(':')
            .ok_or_else(|| format!("{at}: expected `# in: name: kind = default`"))?;
        let name = name.trim();
        if name.is_empty() {
            return Err(format!("{at}: input needs a name"));
        }
        if out.iter().any(|d: &ParamDecl| d.name == name) {
            return Err(format!("{at}: duplicate input {name:?}"));
        }
        let (kind, default) = match rest.split_once('=') {
            Some((k, d)) => (k, d.trim()),
            // Element inputs declare no default, so no `=` is needed.
            None => (rest, ""),
        };
        let kind: ParamKind = match kind.trim() {
            "float" => ParamKind::Float,
            "int" => ParamKind::Int,
            "bool" => ParamKind::Bool,
            "string" => ParamKind::Text,
            "element" => ParamKind::Element,
            other => {
                return Err(format!(
                    "{at}: unknown kind {other:?} (float, int, bool, string, element)"
                ))
            }
        };
        let default = default.trim();
        let default = if kind == ParamKind::Element {
            if default.is_empty() {
                None
            } else {
                return Err(format!("{at}: element inputs take no default"));
            }
        } else if default.is_empty() {
            return Err(format!("{at}: {name:?} needs a default"));
        } else {
            Some(kind.parse(default).map_err(|e| format!("{at}: {e}"))?)
        };
        out.push(ParamDecl {
            name: name.to_string(),
            kind,
            default,
        });
    }
    Ok(out)
}

/// Rewrite references among fresh script outputs onto stored ids.
/// A hosted op points at the fresh id its host was materialized with,
/// but [`reconcile`] reuses the stored id for known keys — without
/// this, a re-run dangles (the door's host points at an id that was
/// never inserted). JSON round-trip, as `fresh_copies` does
/// (`transform.rs`); uuid-shaped strings only ever match ids.
pub fn remap_fresh_ids(fresh: &mut [(String, Element)], prev: &[ScriptOutput]) {
    let map: std::collections::HashMap<String, String> = fresh
        .iter()
        .filter_map(|(key, el)| {
            prev.iter()
                .find(|o| o.key == *key)
                .map(|o| (el.id.0.to_string(), o.id.0.to_string()))
        })
        .collect();
    if map.is_empty() {
        return;
    }
    fn remap(v: &mut serde_json::Value, map: &std::collections::HashMap<String, String>) {
        match v {
            serde_json::Value::String(s) => {
                if let Some(n) = map.get(s.as_str()) {
                    *s = n.clone();
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(|x| remap(x, map)),
            serde_json::Value::Object(o) => o.values_mut().for_each(|x| remap(x, map)),
            _ => {}
        }
    }
    for (_, el) in fresh.iter_mut() {
        if let Ok(mut v) = serde_json::to_value(&*el) {
            remap(&mut v, &map);
            if let Ok(fixed) = serde_json::from_value::<Element>(v) {
                *el = fixed;
            }
        }
    }
}

/// Map keyed script outputs onto one transaction: known keys replace
/// their stored element (identity survives the regen), new keys insert,
/// vanished keys are removed with cascade. Returns the transaction and
/// the new key → id map for the element's `outputs`. Hosted ops
/// (openings pointing at same-run walls) are remapped onto stored ids
/// first — both the CLI and the app runner come through here, so the
/// remap lives here and neither caller can forget it.
pub fn reconcile(
    model: &Model,
    label: &str,
    prev: &[ScriptOutput],
    mut fresh: Vec<(String, Element)>,
) -> (Transaction, Vec<ScriptOutput>) {
    remap_fresh_ids(&mut fresh, prev);
    let mut tx = Transaction::new(label);
    let mut outputs = Vec::with_capacity(fresh.len());
    for (key, mut element) in fresh {
        match prev.iter().find(|o| o.key == key) {
            Some(known) => {
                element.id = known.id;
                match model.get(known.id) {
                    Some(before) => {
                        tx = tx.replace(before.clone(), element);
                    }
                    None => {
                        tx = tx.insert(element);
                    }
                }
                outputs.push(ScriptOutput { key, id: known.id });
            }
            None => {
                let id = element.id;
                tx = tx.insert(element);
                outputs.push(ScriptOutput { key, id });
            }
        }
    }
    let gone: Vec<ElementId> = prev
        .iter()
        .filter(|o| !outputs.iter().any(|n| n.key == o.key))
        .map(|o| o.id)
        .collect();
    tx = tx.remove_all_cascade(model, &gone);
    (tx, outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "# a partition generator\n# in: bays: int = 4\n# in: depth: float = 3.0\n# in: tall: bool = false\n# in: room: string = \"A\"\n# in: host: element\nx = 1\n";

    #[test]
    fn header_parses_all_kinds() {
        let decls = parse_front_matter(HEADER).unwrap();
        assert_eq!(decls.len(), 5);
        assert_eq!(decls[0].name, "bays");
        assert_eq!(decls[0].default, Some(ParamValue::Int(4)));
        assert_eq!(decls[1].default, Some(ParamValue::Float(3.0)));
        assert_eq!(decls[3].default, Some(ParamValue::Text("A".into())));
        assert_eq!(decls[4].kind, ParamKind::Element);
        assert_eq!(decls[4].default, None);
    }

    #[test]
    fn header_errors_carry_line_numbers() {
        let err = parse_front_matter("# in: x: furlong = 2\n").unwrap_err();
        assert!(err.contains("line 1") && err.contains("furlong"), "{err}");
        let err = parse_front_matter("# in: x: int\n").unwrap_err();
        assert!(err.contains("line 1"), "{err}");
        let err = parse_front_matter("# in: x: int = 1\n# in: x: int = 2\n").unwrap_err();
        assert!(err.contains("duplicate"), "{err}");
    }

    #[test]
    fn slug_and_entrance_point() {
        assert_eq!(slug("Partitions"), "partitions");
        assert_eq!(slug("Huis Wand 2"), "huis-wand-2");
        assert_eq!(slug(""), "script");
        assert_eq!(slug("---"), "script");
        assert_eq!(entrance_path("Partitions"), "scripts/partitions.py");
    }

    #[test]
    fn registry_upsert_remove_and_validate() {
        let mut r = ScriptRegistry::default();
        assert!(r.upsert("a", "scripts/a.py"));
        assert!(!r.upsert("a", "scripts/a.py"), "same mapping is no change");
        assert!(r.upsert("a", "scripts/a2.py"), "repointing changes");
        assert_eq!(r.path_of("a"), Some("scripts/a2.py"));
        assert_eq!(r.path_of("missing"), None);
        assert!(r.remove("a"));
        assert!(!r.remove("a"));
        r.validate().unwrap();

        r.upsert("b", "scripts/b.py");
        r.links.push(ScriptLink {
            name: "b".into(),
            path: "scripts/other.py".into(),
        });
        assert!(r.validate().is_err(), "duplicate entry names are invalid");
    }

    fn model_with_script(name: &str) -> (crate::Document, crate::ElementId) {
        let mut doc = crate::Document::from_model(crate::Model::new());
        let script = crate::Element::new(
            name,
            crate::ElementKind::ScriptObject(ScriptObject::new("OUTPUTS = []\n", "cpython")),
        );
        let id = script.id;
        doc.commit(crate::Transaction::new("add").insert(script))
            .unwrap();
        (doc, id)
    }

    #[test]
    fn set_link_creates_registry_and_points_the_block() {
        let (mut doc, id) = model_with_script("Partitions");
        let tx = set_link_tx(doc.model(), id, "Partitions", "scripts/partitions.py").unwrap();
        doc.commit(tx).unwrap();
        let (_, registry) = registry_of(doc.model()).expect("registry exists");
        assert_eq!(
            registry.path_of("Partitions"),
            Some("scripts/partitions.py")
        );
        match &doc.model().get(id).unwrap().kind {
            crate::ElementKind::ScriptObject(s) => {
                assert_eq!(s.link.as_deref(), Some("Partitions"))
            }
            _ => panic!("not a script block"),
        }
    }

    #[test]
    fn drop_link_prunes_unused_entries_but_keeps_shared_ones() {
        let (mut doc, a) = model_with_script("A");
        let script = crate::Element::new(
            "B",
            crate::ElementKind::ScriptObject(ScriptObject::new("OUTPUTS = []\n", "cpython")),
        );
        let b = script.id;
        doc.commit(crate::Transaction::new("add").insert(script))
            .unwrap();
        // Both blocks share one entry.
        let tx = set_link_tx(doc.model(), a, "shared", "scripts/shared.py").unwrap();
        doc.commit(tx).unwrap();
        let tx = set_link_tx(doc.model(), b, "shared", "scripts/shared.py").unwrap();
        doc.commit(tx).unwrap();

        let tx = drop_link_tx(doc.model(), a).unwrap();
        doc.commit(tx).unwrap();
        let (_, registry) = registry_of(doc.model()).expect("shared entry survives");
        assert_eq!(registry.path_of("shared"), Some("scripts/shared.py"));

        let tx = drop_link_tx(doc.model(), b).unwrap();
        doc.commit(tx).unwrap();
        assert!(
            registry_of(doc.model()).is_none(),
            "the emptied registry element goes too"
        );
    }

    fn mesh_named(name: &str) -> Element {
        use crate::{EditMesh, ElementKind, FreeMesh};
        use glam::DVec3;
        Element::new(
            name,
            ElementKind::FreeMesh(FreeMesh::new(
                None,
                DVec3::ZERO,
                EditMesh::cuboid(DVec3::new(1.0, 1.0, 1.0)),
            )),
        )
    }

    #[test]
    fn reconcile_inserts_then_replaces_by_key() {
        use crate::Op;
        let model = Model::new();
        let (tx, outputs) = reconcile(
            &model,
            "script:test",
            &[],
            vec![
                ("a".into(), mesh_named("m-a")),
                ("b".into(), mesh_named("m-b")),
            ],
        );
        assert_eq!(tx.ops.len(), 2);
        assert!(tx.ops.iter().all(|o| matches!(o, Op::Insert(_))));

        let mut model = Model::new();
        tx.apply(&mut model).unwrap();
        let (tx2, outputs2) = reconcile(
            &model,
            "script:test",
            &outputs,
            vec![("a".into(), mesh_named("m-a2"))],
        );
        // "a" replaces its stored id; "b" vanishes and is removed.
        assert_eq!(tx2.ops.len(), 2);
        assert!(tx2.ops.iter().any(|o| matches!(o, Op::Replace { .. })));
        assert_eq!(outputs2.len(), 1);
        assert_eq!(outputs2[0].id, outputs[0].id);
    }

    fn wall_named(name: &str, level: crate::ElementId) -> Element {
        use crate::{ElementKind, Wall};
        use glam::DVec2;
        Element::new(
            name,
            ElementKind::Wall(Wall::new(
                level,
                DVec2::new(0.0, 0.0),
                DVec2::new(4.0, 0.0),
                0.1,
                2.7,
            )),
        )
    }

    fn door_on(name: &str, host: crate::ElementId) -> Element {
        use crate::{ElementKind, Opening, OpeningFrame, OpeningKind};
        Element::new(
            name,
            ElementKind::Opening(Opening {
                host,
                kind: OpeningKind::Door,
                offset: 1.0,
                sill: 0.0,
                width: 0.9,
                height: 2.1,
                material: None,
                frame: OpeningFrame::default(),
                flip_hand: false,
                flip_facing: false,
            }),
        )
    }

    #[test]
    fn managed_outputs_reject_direct_replaces() {
        use crate::{Document, ElementKind, Level, ModelError, Transaction};
        let mut doc = Document::from_model(crate::Model::new());
        doc.commit(Transaction::new("level").insert(Element::new(
            "Ground",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 2.7,
            }),
        )))
        .unwrap();
        let level = doc
            .model()
            .levels()
            .find(|(e, _)| e.name == "Ground")
            .map(|(e, _)| e.id)
            .unwrap();
        let w = wall_named("wall w", level);
        let wall_id = w.id;
        let (tx, outputs) = reconcile(doc.model(), "script:test", &[], vec![("w".into(), w)]);
        doc.commit(tx).unwrap();
        // The script block records the key map, as run_to_transaction does.
        let mut script = ScriptObject::new("OUTPUTS = []\n", "cpython");
        script.outputs = outputs;
        doc.commit(Transaction::new("script").insert(Element::new(
            "wall script",
            ElementKind::ScriptObject(script),
        )))
        .unwrap();

        // A hand edit fails loudly, naming the managing script.
        let err = doc
            .edit(wall_id, "Move wall", |e| e.name = "hand moved".into())
            .unwrap_err();
        assert!(
            matches!(err, ModelError::ManagedByScript { .. }),
            "direct replace is locked: {err:?}"
        );
        // The runner's own transactions pass (labelled `script…`).
        let before = doc.model().get(wall_id).cloned().unwrap();
        let mut after = before.clone();
        after.name = "moved by script".into();
        doc.commit(Transaction::new("script rerun").replace(before, after))
            .expect("runner rewrites pass");
        // Removes stay allowed: the next run re-creates a known key.
        let gone = doc.model().get(wall_id).cloned().unwrap();
        doc.commit(Transaction::new("clear").remove_cascade(doc.model(), gone.id))
            .expect("removes pass");
        assert!(doc.model().get(wall_id).is_none());
    }

    #[test]
    fn reconcile_remaps_same_run_hosts_onto_stored_ids() {
        // The app runner and the CLI both come through reconcile: a
        // hosted fresh element points at its host's fresh id, which
        // was never inserted. Without the remap the re-run dangles
        // ("element … references missing element") and the commit
        // fails — exactly the app failure this guards.
        use crate::{Document, ElementKind, Level, Transaction};
        let mut doc = Document::from_model(crate::Model::new());
        doc.commit(Transaction::new("level").insert(Element::new(
            "Ground",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 2.7,
            }),
        )))
        .unwrap();
        let level = doc
            .model()
            .levels()
            .find(|(e, _)| e.name == "Ground")
            .map(|(e, _)| e.id)
            .unwrap();

        // First run: empty prev, everything inserts.
        let w1 = wall_named("wall w", level);
        let d1 = door_on("door d", w1.id);
        let (tx, outputs) = reconcile(
            doc.model(),
            "script:test",
            &[],
            vec![("w".into(), w1), ("d".into(), d1)],
        );
        doc.commit(tx).unwrap();

        // Second run: all-fresh ids, known keys — the door must come
        // out hosting the STORED wall, and the commit must succeed.
        let w2 = wall_named("wall w", level);
        let d2 = door_on("door d", w2.id);
        let (tx2, outputs2) = reconcile(
            doc.model(),
            "script:test",
            &outputs,
            vec![("w".into(), w2), ("d".into(), d2)],
        );
        doc.commit(tx2).expect("re-run with same-run hosts commits");
        assert_eq!(
            outputs2.iter().map(|o| o.id).collect::<Vec<_>>(),
            outputs.iter().map(|o| o.id).collect::<Vec<_>>()
        );
        let stored_wall = outputs.iter().find(|o| o.key == "w").unwrap().id;
        let stored_door = outputs.iter().find(|o| o.key == "d").unwrap().id;
        match &doc.model().get(stored_door).unwrap().kind {
            ElementKind::Opening(o) => assert_eq!(o.host, stored_wall),
            _ => panic!("not an opening"),
        }
    }
}
