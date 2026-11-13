//! Script outputs through IFC: arc walls export curved footprints
//! (not straightened chords), hosted openings cut them, and the
//! registry + script elements ride along silently — no entities,
//! no errors.

use creabim_core::{
    Document, Element, ElementKind, Level, Model, Opening, OpeningFrame, OpeningKind, ScriptObject,
    ScriptOutput, ScriptRegistry, Transaction, Wall,
};
use creabim_ifc::{export_ifc4, import_ifc, ExportOptions};
use glam::DVec2;

fn arc_model() -> Model {
    let mut doc = Document::from_model(Model::new());
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

    // Quarter-circle wall, radius 4: chord (4,0) -> (0,4), apex (2.83,2.83).
    // bulge = tan(sweep/4), the same value arc_wall() emits.
    let mut wall = Wall::new(level, DVec2::new(4.0, 0.0), DVec2::new(0.0, 4.0), 0.2, 2.7);
    wall.bulge = (std::f64::consts::FRAC_PI_2 / 4.0).tan();
    let wall_el = Element::new("Arc wall", ElementKind::Wall(wall));
    let wall_id = wall_el.id;
    doc.commit(Transaction::new("wall").insert(wall_el))
        .unwrap();

    let door = Element::new(
        "Arc door",
        ElementKind::Opening(Opening {
            host: wall_id,
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
    );
    let door_id = door.id;
    doc.commit(Transaction::new("door").insert(door)).unwrap();

    // The script machinery rides along: outputs recorded on the block,
    // entry recorded in the registry. Neither may leak into the file.
    let mut script = ScriptObject::new("OUTPUTS = []\n", "cpython");
    script.link = Some("arc".into());
    script.outputs = vec![
        ScriptOutput {
            key: "a".into(),
            id: wall_id,
        },
        ScriptOutput {
            key: "d".into(),
            id: door_id,
        },
    ];
    doc.commit(Transaction::new("script").insert(Element::new(
        "Arc script",
        ElementKind::ScriptObject(script),
    )))
    .unwrap();
    let mut registry = ScriptRegistry::default();
    registry.upsert("arc", "scripts/arc.py");
    doc.commit(Transaction::new("registry").insert(Element::new(
        "Script registry",
        ElementKind::ScriptRegistry(registry),
    )))
    .unwrap();
    doc.model().clone()
}

fn cartesian2(ifc: &str) -> Vec<[f64; 2]> {
    let mut out = vec![];
    for caps in ifc
        .split("IFCCARTESIANPOINT((")
        .skip(1)
        .map(|s| s.split(')').next().unwrap_or(""))
    {
        let nums: Vec<f64> = caps
            .split(',')
            .filter_map(|n| n.trim().parse().ok())
            .collect();
        if nums.len() == 2 {
            out.push([nums[0], nums[1]]);
        }
    }
    out
}

#[test]
fn arc_walls_export_curved_footprints() {
    let model = arc_model();
    let ifc = export_ifc4(&model, &ExportOptions::default());
    let count = |entity: &str| ifc.matches(&format!("={entity}(")).count();
    assert_eq!(count("IFCWALL"), 1);
    assert_eq!(count("IFCOPENINGELEMENT"), 1);
    // The script machinery exports nothing.
    assert!(!ifc.contains("scripts/arc.py"));

    // The footprint is written in the wall's placement frame: u along
    // the chord, w across. A straightened chord would keep |w| = 0.1
    // throughout; the arc swells to sagitta + half thickness ≈ 1.27.
    let pts = cartesian2(&ifc);
    let swell = pts.iter().map(|&[_, y]| y.abs()).fold(0.0_f64, f64::max);
    assert!(
        swell > 1.0,
        "footprint follows the arc (swell {swell:.2}, chord would stay 0.1)"
    );
    assert!(
        swell < 1.6,
        "footprint stays in the wall's bounds (swell {swell:.2})"
    );
}

#[test]
fn scripted_models_round_trip_through_ifc() {
    let model = arc_model();
    let ifc = export_ifc4(&model, &ExportOptions::default());
    let back = import_ifc(&ifc).expect("exports import").model;
    // The wall comes back (as a wall or, faceted, as a mesh — the
    // sloped-top precedent); nothing is lost to the void.
    let walls = back
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Wall(_)))
        .count();
    let meshes = back
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::FreeMesh(_)))
        .count();
    assert!(
        walls + meshes >= 1,
        "the arc wall survives the round trip in some class"
    );
}
