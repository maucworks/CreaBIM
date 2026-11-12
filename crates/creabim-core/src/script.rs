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

/// The script block itself: source plus engine, inputs and outputs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScriptObject {
    pub source: String,
    pub engine: String,
    pub inputs: Vec<ParamInput>,
    pub outputs: Vec<ScriptOutput>,
}

impl ScriptObject {
    pub fn new(source: impl Into<String>, engine: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            engine: engine.into(),
            inputs: vec![],
            outputs: vec![],
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
        let rest = match rest.strip_prefix("in:").or_else(|| rest.strip_prefix("in ")) {
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

/// Map keyed script outputs onto one transaction: known keys replace
/// their stored element (identity survives the regen), new keys insert,
/// vanished keys are removed with cascade. Returns the transaction and
/// the new key → id map for the element's `outputs`.
pub fn reconcile(
    model: &Model,
    label: &str,
    prev: &[ScriptOutput],
    fresh: Vec<(String, Element)>,
) -> (Transaction, Vec<ScriptOutput>) {
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
            vec![("a".into(), mesh_named("m-a")), ("b".into(), mesh_named("m-b"))],
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
}
